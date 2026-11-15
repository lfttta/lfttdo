import { useEffect, useRef, useState } from 'react';

/**
 * 本地 mock agent：唯一产生助手回复、流式文本、工具状态、权限卡与提问卡的地方。
 * 不发起真实网络请求，也不读写真实文件；所有内容只存在于 React state。
 *
 * 与 docs/agent-ui-contract.md §7.13 的结构保持一致：
 * send / stop / isStreaming / respondPermission / answerQuestion / retryLast。
 */

const QUESTION_SCRIPT = /发布|截图|说明|草稿|提问/;
const COMMAND_SCRIPT = /运行|命令|构建|build|启动|dev/;
const PERMISSION_SCRIPT = /权限|写入|修改|修复|改一下|编辑/;
const READ_SCRIPT = /读|看一下|检查|文件|导出/;
const WEB_SCRIPT = /搜索|联网|网页|抓取|搜一下/;

const READ_TARGET = 'src/data/mockData.js';
const READ_RESULT = [
  'initialSessions',
  'initialSettings',
  'memories',
  'skills',
  'mcpServers',
  'hooks',
  'fileMentions',
  'models',
  'newSession',
].join('\n');

const COMMAND_TARGET = 'npm run dev -- --host 127.0.0.1';
const COMMAND_RESULT = ['VITE v8.3.8  ready in 312 ms', '', '➜  Local:   http://127.0.0.1:5173/'].join('\n');

const WRITE_TARGET = 'src/App.jsx';
const WRITE_RESULT = '已写入 src/App.jsx';

const QUESTION_TEXT = '这份发布说明先发给谁？';
const QUESTION_OPTIONS = ['团队内部评审', '公开渠道', '先存草稿'];

const ANSWER_MESSAGE = '收到，我按这个方向继续。';

let idCounter = 0;

function makeId(prefix, suffix) {
  idCounter += 1;
  return `${prefix}-${Date.now()}-${idCounter}${suffix ? `-${suffix}` : ''}`;
}

function estimateTokens(text) {
  return Math.max(1, Math.ceil(String(text ?? '').length / 2));
}

function textTool(name, target) {
  return {
    type: 'tool',
    name,
    target,
    status: 'running',
    duration: 0,
    additions: 0,
    deletions: 0,
    result: '',
    errorMessage: '',
  };
}

function writeTool() {
  return {
    type: 'tool',
    name: 'write_file',
    target: WRITE_TARGET,
    status: 'running',
    duration: 0,
    additions: 0,
    deletions: 0,
    result: '',
    errorMessage: '',
  };
}

function errorTool(name, target, message) {
  return {
    type: 'tool',
    name,
    target,
    status: 'error',
    duration: 0,
    additions: 0,
    deletions: 0,
    result: '',
    errorMessage: message,
  };
}

function pickScript(text) {
  const probe = String(text ?? '').toLowerCase();
  if (QUESTION_SCRIPT.test(probe)) return 'question';
  if (COMMAND_SCRIPT.test(probe)) return 'command';
  if (PERMISSION_SCRIPT.test(probe)) return 'permission';
  if (READ_SCRIPT.test(probe)) return 'read';
  if (WEB_SCRIPT.test(probe)) return 'web';
  return 'default';
}

export function useMockAgent({ sessions, settings, setSessions, setUsage }) {
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;

  const [isStreaming, setIsStreaming] = useState(false);
  const [streamingSessionId, setStreamingSessionId] = useState(null);
  const isStreamingRef = useRef(false);

  const runIdRef = useRef(0);
  const timersRef = useRef(new Set());
  const pendingRunRef = useRef(null);
  const sessionAllowedRef = useRef(new Map());
  const startPlanRef = useRef(null);
  const [startSignal, setStartSignal] = useState(0);

  useEffect(
    () => () => {
      // 卸载后让所有在途脚本失效，并清掉全部定时器。
      runIdRef.current += 1;
      pendingRunRef.current = null;
      timersRef.current.forEach((timer) => clearTimeout(timer));
      timersRef.current.clear();
    },
    []
  );

  // 启动脚本要等 setSessions 真正提交：send/retryLast 把计划写入 ref，
  // 再递增 startSignal；effect 在提交后读取计划并开始运行。
  useEffect(() => {
    const plan = startPlanRef.current;
    if (!plan || plan.consumed) return;
    plan.consumed = true;
    // stop() 可能赶在这次提交之前执行：此时不再启动脚本。
    if (!isStreamingRef.current) return;
    if (!plan.found) {
      isStreamingRef.current = false;
      setIsStreaming(false);
      return;
    }
    if (plan.kind === 'send') {
      setUsage((prev) => {
        const current = prev ?? { inputTokens: 0, outputTokens: 0, contextTokens: 0 };
        return { ...current, inputTokens: current.inputTokens + plan.inputTokens };
      });
    }
    beginRun(plan.sessionId, plan.assistantId, plan.text, plan.inputTokens);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startSignal]);

  const setStreaming = (next, sessionId) => {
    isStreamingRef.current = next;
    setIsStreaming(next);
    if (next) {
      if (sessionId) setStreamingSessionId(sessionId);
    } else {
      setStreamingSessionId(null);
    }
  };

  const schedule = (runId, delay, callback) => {
    const timer = setTimeout(() => {
      timersRef.current.delete(timer);
      if (runIdRef.current !== runId) return;
      callback();
    }, delay);
    timersRef.current.add(timer);
    return timer;
  };

  const sessionAllowed = (sessionId, tool) => {
    const allowed = sessionAllowedRef.current.get(sessionId);
    return Boolean(allowed && allowed.has(tool));
  };

  const allowSessionTool = (sessionId, tool) => {
    if (!sessionId || !tool) return;
    const map = sessionAllowedRef.current;
    const set = map.get(sessionId) ?? new Set();
    set.add(tool);
    map.set(sessionId, set);
  };

  const patchAssistant = (sessionId, assistantId, updater) => {
    setSessions((prev) =>
      prev.map((session) =>
        session.id === sessionId
          ? {
              ...session,
              messages: session.messages.map((message) =>
                message.id === assistantId ? updater(message) : message
              ),
            }
          : session
      )
    );
  };

  const appendBlock = (ctx, block) => {
    patchAssistant(ctx.sessionId, ctx.assistantId, (message) => ({
      ...message,
      blocks: [...message.blocks, block],
    }));
  };

  const appendText = (ctx, chunk) => {
    patchAssistant(ctx.sessionId, ctx.assistantId, (message) => {
      const { blocks } = message;
      const lastIndex = blocks.length - 1;
      if (lastIndex >= 0 && blocks[lastIndex].type === 'text') {
        const nextBlocks = blocks.slice();
        nextBlocks[lastIndex] = {
          ...nextBlocks[lastIndex],
          text: nextBlocks[lastIndex].text + chunk,
        };
        return { ...message, blocks: nextBlocks };
      }
      return { ...message, blocks: [...blocks, { type: 'text', text: chunk }] };
    });
  };

  const updateTool = (ctx, name, target, patch) => {
    patchAssistant(ctx.sessionId, ctx.assistantId, (message) => {
      let index = -1;
      message.blocks.forEach((block, blockIndex) => {
        if (
          block.type === 'tool' &&
          block.name === name &&
          block.target === target &&
          block.status === 'running'
        ) {
          index = blockIndex;
        }
      });
      if (index < 0) return message;
      const blocks = message.blocks.slice();
      blocks[index] = { ...blocks[index], ...patch };
      return { ...message, blocks };
    });
  };

  const streamText = (ctx, text, onDone) => {
    if (runIdRef.current !== ctx.runId) return;
    const full = String(text ?? '');
    ctx.output += full;
    if (!full) {
      onDone?.();
      return;
    }
    let offset = 0;
    const step = () => {
      if (runIdRef.current !== ctx.runId) return;
      if (offset >= full.length) {
        onDone?.();
        return;
      }
      const size = 2 + Math.floor(Math.random() * 3);
      appendText(ctx, full.slice(offset, offset + size));
      offset += size;
      schedule(ctx.runId, 20 + Math.floor(Math.random() * 21), step);
    };
    step();
  };

  const finishRun = (ctx) => {
    if (runIdRef.current !== ctx.runId) return;
    pendingRunRef.current = null;

    const now = new Date().toISOString();
    setSessions((prev) =>
      prev.map((session) =>
        session.id === ctx.sessionId
          ? {
              ...session,
              updatedAt: now,
              messages: session.messages.map((message) =>
                message.id === ctx.assistantId ? { ...message, isStreaming: false } : message
              ),
            }
          : session
      )
    );

    const budget = Math.max(1, Number(settingsRef.current?.contextBudget) || 1);
    const outputTokens = estimateTokens(ctx.output);
    setUsage((prev) => {
      const current = prev ?? { inputTokens: 0, outputTokens: 0, contextTokens: 0 };
      return {
        inputTokens: current.inputTokens,
        outputTokens: current.outputTokens + outputTokens,
        contextTokens: Math.min(budget, current.contextTokens + ctx.inputTokens + outputTokens),
      };
    });

    setStreaming(false);
  };

  const requestPermission = (ctx, request, onDecision) => {
    const permissionId = makeId('permission');
    appendBlock(ctx, {
      type: 'permission',
      id: permissionId,
      tool: request.tool,
      args: request.args,
      risk: request.risk,
      decision: null,
    });
    pendingRunRef.current = {
      permissionId,
      tool: request.tool,
      sessionId: ctx.sessionId,
      resume: onDecision,
    };
  };

  const runReadScript = (ctx) => {
    streamText(ctx, '我只读这个文件，先确认它的导出面。', () => {
      appendBlock(ctx, textTool('read_file', READ_TARGET));
      const startedAt = Date.now();
      const delay = 800 + Math.floor(Math.random() * 401);
      schedule(ctx.runId, delay, () => {
        updateTool(ctx, 'read_file', READ_TARGET, {
          status: 'complete',
          duration: Date.now() - startedAt,
          result: READ_RESULT,
        });
        streamText(ctx, '导出符合契约，字段都在。', () => {
          appendBlock(ctx, {
            type: 'file',
            path: READ_TARGET,
            name: 'mockData.js',
          });
          finishRun(ctx);
        });
      });
    });
  };

  const executeCommand = (ctx) => {
    appendBlock(ctx, textTool('run_command', COMMAND_TARGET));
    const startedAt = Date.now();
    const delay = 1000 + Math.floor(Math.random() * 501);
    schedule(ctx.runId, delay, () => {
      updateTool(ctx, 'run_command', COMMAND_TARGET, {
        status: 'complete',
        duration: Date.now() - startedAt,
        result: COMMAND_RESULT,
      });
      streamText(ctx, '开发服务器已就绪。', () => finishRun(ctx));
    });
  };

  const runCommandScript = (ctx) => {
    streamText(ctx, '先确认开发服务器能起来，再跑一次生产构建。', () => {
      const config = settingsRef.current ?? {};
      if (config.planMode) {
        appendBlock(ctx, errorTool('run_command', COMMAND_TARGET, '请先退出 Plan 模式'));
        finishRun(ctx);
        return;
      }
      const autoAllowed =
        config.permissionMode === 'trusted' ||
        (config.permissionMode === 'rules' && sessionAllowed(ctx.sessionId, 'run_command'));
      if (autoAllowed) {
        executeCommand(ctx);
        return;
      }
      requestPermission(
        ctx,
        {
          tool: 'run_command',
          args: { command: COMMAND_TARGET },
          risk: 'exec',
        },
        (decision) => {
          if (decision === 'deny') {
            streamText(ctx, '已拒绝，我不会执行这个命令。', () => finishRun(ctx));
            return;
          }
          executeCommand(ctx);
        }
      );
    });
  };

  const executeWrite = (ctx) => {
    appendBlock(ctx, writeTool());
    const startedAt = Date.now();
    const delay = 800 + Math.floor(Math.random() * 401);
    schedule(ctx.runId, delay, () => {
      updateTool(ctx, 'write_file', WRITE_TARGET, {
        status: 'complete',
        duration: Date.now() - startedAt,
        additions: 1,
        deletions: 1,
        result: WRITE_RESULT,
      });
      streamText(ctx, '已写入，继续下一步。', () => finishRun(ctx));
    });
  };

  const runPermissionScript = (ctx) => {
    streamText(ctx, '这一步需要写入文件，我先请求你的许可。', () => {
      const config = settingsRef.current ?? {};
      if (config.planMode) {
        appendBlock(ctx, errorTool('write_file', WRITE_TARGET, '请先退出 Plan 模式'));
        finishRun(ctx);
        return;
      }
      const autoAllowed =
        config.permissionMode === 'trusted' ||
        (config.permissionMode === 'rules' && sessionAllowed(ctx.sessionId, 'write_file'));
      if (autoAllowed) {
        executeWrite(ctx);
        return;
      }
      requestPermission(
        ctx,
        {
          tool: 'write_file',
          args: {
            path: WRITE_TARGET,
            patch: '- </div>\n+ </VStack>',
          },
          risk: 'write',
        },
        (decision) => {
          if (decision === 'deny') {
            streamText(ctx, '已拒绝，我不会修改文件。', () => finishRun(ctx));
            return;
          }
          executeWrite(ctx);
        }
      );
    });
  };

  const runQuestionScript = (ctx) => {
    streamText(ctx, '我整理了一版发布说明草稿。', () => {
      appendBlock(ctx, {
        type: 'code',
        language: 'markdown',
        title: 'docs/release-notes.md',
        code: '# v0.2.0\n\n## 新增\n- …\n\n## 改进\n- …\n\n## 修复\n- …',
      });
      appendBlock(ctx, {
        type: 'file',
        path: 'docs/release-notes.md',
        name: 'release-notes.md',
      });
      const questionId = makeId('question');
      appendBlock(ctx, {
        type: 'question',
        id: questionId,
        question: QUESTION_TEXT,
        options: QUESTION_OPTIONS,
        answer: null,
      });
      pendingRunRef.current = {
        questionId,
        tool: null,
        sessionId: ctx.sessionId,
        resume: () => finishRun(ctx),
      };
    });
  };

  const runWebScript = (ctx, text) => {
    streamText(ctx, '我查一下公开资料，再把结论整理给你。', () => {
      const target = String(text ?? '').trim().slice(0, 40) || 'Astryx 组件文档';
      appendBlock(ctx, textTool('web_search', target));
      const startedAt = Date.now();
      const delay = 900 + Math.floor(Math.random() * 501);
      schedule(ctx.runId, delay, () => {
        updateTool(ctx, 'web_search', target, {
          status: 'complete',
          duration: Date.now() - startedAt,
          result: [
            '1. Astryx 0.7.16 组件文档 — ChatComposer / ChatComposerInput',
            '2. ChatComposerInput triggers — 技能菜单与文件提及',
            '3. 本地演示：不发起真实网络请求',
          ].join('\n'),
        });
        streamText(
          ctx,
          '整理好了：优先看 Astryx 官方组件文档，再回到本项目的契约核对字段。这里不发起真实网络请求，只演示搜索型工具的结果卡。',
          () => finishRun(ctx)
        );
      });
    });
  };

  const runDefaultScript = (ctx) => {
    streamText(
      ctx,
      '我先把这件事拆成三步：确认现状、做一个最小改动、再验证结果。你可以继续补充目标或约束。',
      () => finishRun(ctx)
    );
  };

  const startScript = (ctx, text) => {
    if (runIdRef.current !== ctx.runId) return;
    const kind = pickScript(text);
    if (kind === 'question') {
      runQuestionScript(ctx);
      return;
    }
    if (kind === 'command') {
      runCommandScript(ctx);
      return;
    }
    if (kind === 'permission') {
      runPermissionScript(ctx);
      return;
    }
    if (kind === 'read') {
      runReadScript(ctx);
      return;
    }
    if (kind === 'web') {
      runWebScript(ctx, text);
      return;
    }
    runDefaultScript(ctx);
  };

  const beginRun = (sessionId, assistantId, text, inputTokens) => {
    const runId = runIdRef.current + 1;
    runIdRef.current = runId;
    const ctx = {
      runId,
      sessionId,
      assistantId,
      inputTokens,
      output: '',
    };
    setStreaming(true, sessionId);
    schedule(runId, 250, () => startScript(ctx, text));
  };

  const send = (text, sessionId, images = []) => {
    const trimmed = String(text ?? '').trim();
    if (!trimmed || isStreamingRef.current) return;
    // 同步确认会话存在：不能把「找到了」写在 setSessions 的更新器里再让 effect 读，
    // 两处执行的先后顺序不保证，回复会偶尔根本不启动。
    if (!sessionsRef.current.some((session) => session.id === sessionId)) return;

    const now = new Date().toISOString();
    const userId = makeId('message', 'u');
    const assistantId = makeId('message', 'a');
    const inputTokens = estimateTokens(trimmed) + 200;
    const title = trimmed.replace(/\s+/g, ' ').slice(0, 16);
    const pendingImages = Array.isArray(images) ? images : [];
    const plan = {
      kind: 'send',
      found: true,
      consumed: false,
      sessionId,
      assistantId,
      text: trimmed,
      inputTokens,
    };
    startPlanRef.current = plan;

    setSessions((prev) => {
      let found = false;
      const next = prev.map((session) => {
        if (session.id !== sessionId) return session;
        found = true;
        const shouldTitle =
          session.messages.length === 0 && (!session.title || session.title === '新对话');
        return {
          ...session,
          title: shouldTitle ? title : session.title,
          updatedAt: now,
          messages: [
            ...session.messages,
            {
              id: userId,
              role: 'user',
              createdAt: now,
              blocks: [
                { type: 'text', text: trimmed },
                ...pendingImages.map((image) => ({
                  type: 'image',
                  src: image.url,
                  alt: image.name,
                })),
              ],
            },
            {
              id: assistantId,
              role: 'assistant',
              createdAt: now,
              model: settingsRef.current?.model,
              isStreaming: true,
              blocks: [],
            },
          ],
        };
      });
      if (!found) return prev;
      plan.found = true;
      return next;
    });

    // 先亮起“停止”按钮，防止 250ms 延迟内重复发送。
    setStreaming(true, sessionId);
    setStartSignal((value) => value + 1);
  };

  const stop = () => {
    runIdRef.current += 1;
    pendingRunRef.current = null;
    timersRef.current.forEach((timer) => clearTimeout(timer));
    timersRef.current.clear();

    const now = new Date().toISOString();
    setSessions((prev) =>
      prev.map((session) => {
        let streaming = false;
        const messages = session.messages.map((message) => {
          if (!message.isStreaming) return message;
          streaming = true;
          return { ...message, isStreaming: false, stopped: true };
        });
        return streaming ? { ...session, updatedAt: now, messages } : session;
      })
    );
    setStreaming(false);
  };

  const respondPermission = (permissionId, decision) => {
    if (!permissionId || !['once', 'session', 'deny'].includes(decision)) return;

    const pending = pendingRunRef.current;
    if (pending && pending.permissionId === permissionId) {
      const now = new Date().toISOString();
      setSessions((prev) =>
        prev.map((session) => {
          if (session.id !== pending.sessionId) return session;
          let changed = false;
          const messages = session.messages.map((message) => {
            let messageChanged = false;
            const blocks = message.blocks.map((block) => {
              if (block.type !== 'permission' || block.id !== permissionId) return block;
              if (block.decision != null) return block;
              messageChanged = true;
              return { ...block, decision };
            });
            if (!messageChanged) return message;
            changed = true;
            return { ...message, blocks };
          });
          return changed ? { ...session, updatedAt: now, messages } : session;
        })
      );

      if (decision === 'session') allowSessionTool(pending.sessionId, pending.tool);
      pendingRunRef.current = null;
      pending.resume(decision);
      return;
    }

    // 静态权限卡（例如 session-3 的 permission-3-1）：直接更新并补一条助手消息。
    setSessions((prev) => {
      let applied = false;
      let appliedSessionId = null;
      let appliedTool = null;
      let appliedArgs = null;

      const next = prev.map((session) => {
        let sessionApplied = false;
        const messages = session.messages.map((message) => {
          // 停止后遗留在屏幕上的权限卡不再驱动脚本，也不追加“已执行”消息。
          if (message.stopped) return message;
          let messageApplied = false;
          const blocks = message.blocks.map((block) => {
            if (block.type !== 'permission' || block.id !== permissionId) return block;
            if (block.decision != null) return block;
            messageApplied = true;
            appliedTool = block.tool;
            appliedArgs = block.args ?? {};
            return { ...block, decision };
          });
          if (!messageApplied) return message;
          sessionApplied = true;
          return { ...message, blocks };
        });
        if (!sessionApplied) return session;

        applied = true;
        appliedSessionId = session.id;
        const now = new Date().toISOString();
        const resultMessage =
          decision === 'deny'
            ? {
                id: makeId('message', 'a'),
                role: 'assistant',
                createdAt: now,
                blocks: [{ type: 'text', text: '已拒绝，不会执行这一步。' }],
              }
            : {
                id: makeId('message', 'a'),
                role: 'assistant',
                createdAt: now,
                blocks: [
                  {
                    type: 'tool',
                    name: appliedTool,
                    target: String(appliedArgs?.path ?? appliedArgs?.command ?? '当前会话'),
                    status: 'complete',
                    duration: 800,
                    additions: 1,
                    deletions: 1,
                    result: `已执行：${appliedTool}`,
                    errorMessage: '',
                  },
                ],
              };
        return { ...session, updatedAt: now, messages: [...messages, resultMessage] };
      });

      if (applied && decision === 'session' && appliedSessionId && appliedTool) {
        allowSessionTool(appliedSessionId, appliedTool);
      }
      return applied ? next : prev;
    });
  };

  const answerQuestion = (questionId, answer) => {
    const trimmed = String(answer ?? '').trim();
    if (!questionId || !trimmed) return;

    const pending = pendingRunRef.current;
    const now = new Date().toISOString();
    let applied = false;

    setSessions((prev) => {
      const next = prev.map((session) => {
        let changed = false;
        const messages = session.messages.map((message) => {
          // 停止后遗留在屏幕上的提问卡不再驱动脚本，也不追加回答与助手消息。
          if (message.stopped) return message;
          let messageChanged = false;
          const blocks = message.blocks.map((block) => {
            if (block.type !== 'question' || block.id !== questionId) return block;
            if (block.answer != null) return block;
            messageChanged = true;
            return { ...block, answer: trimmed };
          });
          if (!messageChanged) return message;
          changed = true;
          return { ...message, blocks };
        });
        if (!changed) return session;
        applied = true;
        return {
          ...session,
          updatedAt: now,
          messages: [
            ...messages,
            {
              id: makeId('message', 'u'),
              role: 'user',
              createdAt: now,
              blocks: [{ type: 'text', text: trimmed }],
            },
            {
              id: makeId('message', 'a'),
              role: 'assistant',
              createdAt: now,
              blocks: [{ type: 'text', text: ANSWER_MESSAGE }],
            },
          ],
        };
      });
      return applied ? next : prev;
    });

    if (pending && pending.questionId === questionId) {
      pendingRunRef.current = null;
      pending.resume(trimmed);
    }
  };

  const retryLast = (sessionId) => {
    if (isStreamingRef.current) return;

    const targetSession = sessionsRef.current.find((session) => session.id === sessionId);
    if (!targetSession) return;

    let targetAssistantIndex = -1;
    targetSession.messages.forEach((message, index) => {
      if (message.role === 'assistant') targetAssistantIndex = index;
    });
    if (targetAssistantIndex < 0) return;

    const targetMessages = targetSession.messages.slice(0, targetAssistantIndex);
    let targetUser = null;
    for (let index = targetMessages.length - 1; index >= 0; index -= 1) {
      if (targetMessages[index].role === 'user') {
        targetUser = targetMessages[index];
        break;
      }
    }
    if (!targetUser) return;

    const targetText = targetUser.blocks
      .filter((block) => block.type === 'text')
      .map((block) => block.text)
      .join('\n\n');

    const assistantId = makeId('message', 'a');
    const plan = {
      kind: 'retry',
      found: true,
      consumed: false,
      sessionId,
      assistantId,
      text: targetText,
      inputTokens: 0,
    };
    startPlanRef.current = plan;

    setSessions((prev) => {
      let found = false;
      const next = prev.map((session) => {
        if (session.id !== sessionId) return session;

        let lastAssistantIndex = -1;
        session.messages.forEach((message, index) => {
          if (message.role === 'assistant') lastAssistantIndex = index;
        });
        if (lastAssistantIndex < 0) return session;

        const remaining = session.messages.slice(0, lastAssistantIndex);
        let userMessage = null;
        for (let index = remaining.length - 1; index >= 0; index -= 1) {
          if (remaining[index].role === 'user') {
            userMessage = remaining[index];
            break;
          }
        }
        if (!userMessage) return session;

        const text = userMessage.blocks
          .filter((block) => block.type === 'text')
          .map((block) => block.text)
          .join('\n\n');

        found = true;
        plan.text = text;
        const now = new Date().toISOString();
        return {
          ...session,
          updatedAt: now,
          messages: [
            ...remaining,
            {
              id: assistantId,
              role: 'assistant',
              createdAt: now,
              model: settingsRef.current?.model,
              isStreaming: true,
              blocks: [],
            },
          ],
        };
      });
      if (!found) return prev;
      plan.found = true;
      return next;
    });

    setStreaming(true, sessionId);
    setStartSignal((value) => value + 1);
  };

  return {
    send,
    stop,
    isStreaming,
    streamingSessionId,
    respondPermission,
    answerQuestion,
    retryLast,
  };
}

export default useMockAgent;
