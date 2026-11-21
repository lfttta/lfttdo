/**
 * AgentEvent → 界面消息的纯投影（spec §9）。
 *
 * 直播与历史重放走同一个函数，区别只在 meta.live：
 * - live=true：`turn_start` 先把上一条还在流式的助手气泡关掉，再开一条新的；
 *   `done` 才整体收尾（interrupted 会给当前气泡打 stopped 标记）。
 * - live=false（重放）：一律不流式；Rust 的重放流末尾恒有 done（db.rs:707）。
 *
 * 状态是一个可整体替换的对象 { messages, usage, streaming, nextId }；每次事件返回
 * 新对象，React 侧直接把 messages 写回对应会话。toasts/fatalError 只描述当前这一条
 * 事件产生的副作用，由调用方消费，不累积。
 */

export function emptyProjection() {
  return { messages: [], usage: null, streaming: false, nextId: 1 };
}

const WHERE_LABELS = {
  model: '模型',
  tool: '工具',
  mcp: 'MCP',
  loop: '循环',
  store: '存储',
};

function nowIso(meta) {
  if (typeof meta?.now === 'function') return meta.now();
  return new Date().toISOString();
}

/** content block 的线上形状 → 界面块；tool_call/tool_result 不出现在用户消息里（Plan A 不可达），忽略。 */
function mapContentBlocks(blocks) {
  return (Array.isArray(blocks) ? blocks : []).flatMap((block) => {
    if (!block) return [];
    if (block.type === 'text') return [{ type: 'text', text: String(block.text ?? '') }];
    if (block.type === 'image') {
      // mime 是裸 "png"/"jpeg"（Rust ContentBlock），data URL 要补 image/ 前缀。
      return [
        {
          type: 'image',
          src: `data:image/${block.mime};base64,${block.data}`,
          alt: '附件图片',
        },
      ];
    }
    return [];
  });
}

/** spec §11.4 的 target 推导顺序；都不命中回落工具名。 */
export function deriveTarget(input, name) {
  const candidate = input?.path ?? input?.command ?? input?.pattern ?? input?.query ?? input?.url;
  const text = candidate == null ? '' : String(candidate);
  return text || String(name ?? '当前会话');
}

function lastAssistantIndex(messages) {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === 'assistant') return i;
  }
  return -1;
}

function newMessage(next, meta, extra) {
  const id = `agent-message-${next.nextId}`;
  next.nextId += 1;
  return { id, role: 'assistant', createdAt: nowIso(meta), blocks: [], model: null, isStreaming: false, ...extra };
}

/** 确保存在一条助手消息可挂块（Rust 直播一定先发 turn_start，这里是防御）。 */
function ensureAssistant(next, meta) {
  const index = lastAssistantIndex(next.messages);
  if (index >= 0) return index;
  const message = newMessage(next, meta, { isStreaming: next.streaming });
  next.messages = [...next.messages, message];
  return next.messages.length - 1;
}

function appendToAssistant(next, meta, block) {
  const index = ensureAssistant(next, meta);
  const message = next.messages[index];
  next.messages[index] = { ...message, blocks: [...message.blocks, block] };
}

function decisionToUi(decision) {
  if (decision?.allow === true) return decision.scope === 'session' ? 'session' : 'once';
  return 'deny';
}

/** done 后关闭流式；interrupted 给被截断的气泡留 stopped 标记（design §9 done 行）。 */
function finish(next, event, live) {
  next.messages = next.messages.map((message) => {
    if (!message.isStreaming) return message;
    const stopped = live && event?.stop_reason === 'interrupted';
    return { ...message, isStreaming: false, ...(stopped ? { stopped: true } : {}) };
  });
  next.streaming = false;
}

export function projectEvent(state, event, meta = {}) {
  const live = Boolean(meta.live);
  const next = { ...state, messages: state.messages, toasts: [], fatalError: null };
  if (!event || typeof event.t !== 'string') return next;

  switch (event.t) {
    case 'user_message': {
      const message = {
        id: `agent-message-${next.nextId}`,
        role: 'user',
        createdAt: nowIso(meta),
        blocks: mapContentBlocks(event.blocks),
      };
      next.nextId += 1;
      next.messages = [...next.messages, message];
      break;
    }

    case 'turn_start': {
      // 一轮用户输入可能产生多条助手气泡；先关掉还在流式的那条。
      const closed = next.messages.map((message) =>
        message.isStreaming ? { ...message, isStreaming: false } : message
      );
      const message = newMessage(next, meta, {
        isStreaming: live,
        turn: event.turn,
        model: meta.model ?? null,
      });
      next.messages = [...closed, message];
      if (live) next.streaming = true;
      break;
    }

    case 'text_delta': {
      const text = String(event.text ?? '');
      if (!text) break;
      const index = ensureAssistant(next, meta);
      const message = next.messages[index];
      const blocks = message.blocks;
      const last = blocks[blocks.length - 1];
      const merged =
        last && last.type === 'text'
          ? [...blocks.slice(0, -1), { ...last, text: last.text + text }]
          : [...blocks, { type: 'text', text }];
      next.messages[index] = { ...message, blocks: merged };
      break;
    }

    case 'tool_call': {
      const call = event.call ?? {};
      appendToAssistant(next, meta, {
        type: 'tool',
        id: call.id,
        name: call.name,
        target: deriveTarget(call.input, call.name),
        risk: event.risk ?? call.risk,
        status: 'running',
        duration: 0,
        additions: 0,
        deletions: 0,
        result: '',
        errorMessage: '',
      });
      break;
    }

    case 'tool_result': {
      // 按 call_id 定位工具块（spec §9.3：不写「最后一张卡」）；图片块插在工具卡后面。
      let target = null;
      for (let i = next.messages.length - 1; i >= 0 && !target; i -= 1) {
        const blocks = next.messages[i].blocks;
        for (let b = blocks.length - 1; b >= 0; b -= 1) {
          if (blocks[b].type === 'tool' && blocks[b].id === event.call_id) {
            target = { messageIndex: i, blockIndex: b };
            break;
          }
        }
      }
      if (!target) break;
      const message = next.messages[target.messageIndex];
      const block = message.blocks[target.blockIndex];
      const images = Array.isArray(event.images) ? event.images : [];
      const imageBlocks = images
        .filter((image) => image?.data)
        .map((image) => ({
          type: 'image',
          src: `data:image/${image.mime};base64,${image.data}`,
          alt: '工具输出图片',
        }));
      const updated = {
        ...block,
        status: event.is_error ? 'error' : 'complete',
        // 重放不还原耗时（db.rs:656 是 ms:0），0 保持 0，由 UI 决定要不要显示。
        duration: event.ms ?? block.duration,
        result: event.is_error ? '' : String(event.preview ?? ''),
        errorMessage: event.is_error ? String(event.preview ?? '工具执行失败') : block.errorMessage,
      };
      const blocks = message.blocks.slice();
      blocks.splice(target.blockIndex, 1, updated, ...imageBlocks);
      next.messages[target.messageIndex] = { ...message, blocks };
      break;
    }

    case 'permission_request': {
      const call = event.call ?? {};
      appendToAssistant(next, meta, {
        type: 'permission',
        id: event.id,
        tool: call.name,
        args: call.input ?? {},
        risk: event.risk ?? call.risk,
        decision: null,
      });
      break;
    }

    case 'permission_resolved': {
      // 只有这一条事件能改权限卡（spec §10.3）；命令返回值只决定 toast。
      const decision = decisionToUi(event.decision);
      next.messages = next.messages.map((message) => ({
        ...message,
        blocks: message.blocks.map((block) =>
          block.type === 'permission' && block.id === event.id && block.decision == null
            ? { ...block, decision }
            : block
        ),
      }));
      break;
    }

    case 'question': {
      appendToAssistant(next, meta, {
        type: 'question',
        id: event.id,
        question: String(event.question ?? ''),
        options: Array.isArray(event.options) ? event.options.map(String) : [],
        answer: null,
      });
      break;
    }

    case 'usage': {
      next.usage = {
        inputTokens: event.input_tokens ?? 0,
        outputTokens: event.output_tokens ?? 0,
        contextTokens: event.input_tokens ?? 0,
        contextBudget: event.budget ?? state.usage?.contextBudget ?? null,
      };
      break;
    }

    case 'context_compacted': {
      next.toasts = [
        {
          body: `已压缩上下文：丢弃 ${event.dropped ?? 0} 条，摘要 ${event.summary_chars ?? 0} 字`,
        },
      ];
      break;
    }

    case 'memory_saved': {
      next.toasts = [{ body: `已记住 ${event.count ?? 0} 条记忆` }];
      break;
    }

    case 'workflow_step': {
      next.toasts = [
        {
          body: `工作流「${event.workflow ?? ''}」步骤 ${event.index ?? 0}/${event.total ?? 0}：${event.step ?? ''}`,
        },
      ];
      break;
    }

    case 'experiment_run': {
      next.toasts = [
        {
          body: `实验复跑 ${event.repeat ?? 0}/${event.of ?? 0}：${event.command_tail ?? ''}`,
        },
      ];
      break;
    }

    case 'error': {
      if (event.fatal) {
        next.fatalError = String(event.message ?? '发生致命错误');
        next.toasts = [{ body: next.fatalError, type: 'error' }];
      } else {
        const where = WHERE_LABELS[event.where] ?? '内核';
        next.toasts = [{ body: `${where}出错：${event.message ?? ''}`, type: 'error' }];
      }
      break;
    }

    case 'done': {
      finish(next, event, live);
      break;
    }

    default:
      // 未知事件：忽略，不崩。Rust 新增变体时前端先保持可用。
      break;
  }

  return next;
}

/**
 * 重放一组历史事件（session_messages 的返回值）。末尾的 done 在重放里只负责
 * 收尾语义，不产生流式状态。
 */
export function projectEvents(events, { model = null } = {}) {
  let state = emptyProjection();
  const list = Array.isArray(events) ? events : [];
  for (const event of list) {
    state = projectEvent(state, event, { live: false, model });
  }
  return state;
}
