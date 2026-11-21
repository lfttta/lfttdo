import { describe, expect, it } from 'vitest';

import { emptyProjection, projectEvent, projectEvents } from './projection';

const NOW = '2026-11-17T00:00:00.000Z';
const meta = (extra = {}) => ({ live: true, now: () => NOW, model: 'qwen-plus', ...extra });

function run(events, { live = true, model = 'qwen-plus' } = {}) {
  let state = emptyProjection();
  for (const event of events) {
    state = projectEvent(state, event, { live, now: () => NOW, model });
  }
  return state;
}

describe('projectEvent：直播文本流', () => {
  it('turn_start + text_delta + done 生成一条助手消息并收尾', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '你' },
      { t: 'text_delta', text: '好' },
      { t: 'done', stop_reason: 'end_turn' },
    ]);

    expect(state.streaming).toBe(false);
    expect(state.messages).toHaveLength(1);
    const message = state.messages[0];
    expect(message.role).toBe('assistant');
    expect(message.model).toBe('qwen-plus');
    expect(message.createdAt).toBe(NOW);
    expect(message.blocks).toEqual([{ type: 'text', text: '你好' }]);
    expect(message.isStreaming).toBe(false);
    expect(message.stopped).toBeUndefined();
  });

  it('text_delta 合并进末尾文本块，不另起新块', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '第一段' },
      { t: 'text_delta', text: '第二段' },
    ]);
    expect(state.messages[0].blocks).toEqual([{ type: 'text', text: '第一段第二段' }]);
  });

  it('interrupted 的 done 给当前气泡打 stopped', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '写了一半' },
      { t: 'done', stop_reason: 'interrupted' },
    ]);
    expect(state.messages[0].stopped).toBe(true);
    expect(state.messages[0].isStreaming).toBe(false);
  });

  it('第二个 turn_start 关闭上一条气泡并新开一条（一轮多条助手气泡）', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '先查一下。' },
      { t: 'turn_start', turn: 2 },
      { t: 'text_delta', text: '结果是 42。' },
      { t: 'done', stop_reason: 'end_turn' },
    ]);
    expect(state.messages).toHaveLength(2);
    expect(state.messages[0].isStreaming).toBe(false);
    expect(state.messages[0].blocks).toEqual([{ type: 'text', text: '先查一下。' }]);
    expect(state.messages[1].blocks).toEqual([{ type: 'text', text: '结果是 42。' }]);
  });

  it('turn_start 缺 meta.model 时消息 model 为 null', () => {
    let state = emptyProjection();
    state = projectEvent(state, { t: 'turn_start', turn: 1 }, { live: true, now: () => NOW });
    expect(state.messages[0].model).toBeNull();
  });
});

describe('projectEvent：工具对', () => {
  const toolCall = {
    t: 'tool_call',
    call: { id: 'c1', name: 'fs.read', risk: 'read', input: { path: 'src/main.rs' } },
    risk: 'read',
  };

  it('tool_call 生成 running 工具卡，target 按 path 推导', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      toolCall,
    ]);
    const block = state.messages[0].blocks[0];
    expect(block).toMatchObject({
      type: 'tool',
      id: 'c1',
      name: 'fs.read',
      target: 'src/main.rs',
      risk: 'read',
      status: 'running',
    });
  });

  it('tool_result 按 call_id 更新状态与结果，不认「最后一张卡」', () => {
    const second = {
      t: 'tool_call',
      call: { id: 'c2', name: 'fs.read', risk: 'read', input: { path: 'b.txt' } },
      risk: 'read',
    };
    const state = run([
      { t: 'turn_start', turn: 1 },
      toolCall,
      second,
      {
        t: 'tool_result',
        call_id: 'c2',
        is_error: false,
        ms: 120,
        preview: '两个字节',
        images: [],
      },
    ]);
    const blocks = state.messages[0].blocks;
    expect(blocks[0].status).toBe('running');
    expect(blocks[1]).toMatchObject({ id: 'c2', status: 'complete', duration: 120, result: '两个字节' });
  });

  it('失败的 tool_result 写进 errorMessage，result 保持空', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      toolCall,
      { t: 'tool_result', call_id: 'c1', is_error: true, ms: 5, preview: '权限不足', images: [] },
    ]);
    expect(state.messages[0].blocks[0]).toMatchObject({
      status: 'error',
      errorMessage: '权限不足',
      result: '',
    });
  });

  it('重放的 ms:0 保持 0，由 UI 决定显示；图片追加为图片块', () => {
    const state = run(
      [
        { t: 'turn_start', turn: 1 },
        toolCall,
        {
          t: 'tool_result',
          call_id: 'c1',
          is_error: false,
          ms: 0,
          preview: '',
          images: [{ data: 'QUJD', mime: 'png' }],
        },
      ],
      { live: false },
    );
    const blocks = state.messages[0].blocks;
    expect(blocks[0]).toMatchObject({ status: 'complete', duration: 0 });
    expect(blocks[1]).toEqual({
      type: 'image',
      src: 'data:image/png;base64,QUJD',
      alt: '工具输出图片',
    });
  });

  it('target 的推导顺序是 command → pattern → query → url → 工具名', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      {
        t: 'tool_call',
        call: { id: 'c1', name: 'shell', risk: 'exec', input: { command: 'cargo test' } },
        risk: 'exec',
      },
      {
        t: 'tool_call',
        call: { id: 'c2', name: 'fetch_url', risk: 'read', input: { url: 'https://x' } },
        risk: 'read',
      },
      {
        t: 'tool_call',
        call: { id: 'c3', name: 'unknown_tool', risk: 'read', input: {} },
        risk: 'read',
      },
    ]);
    const blocks = state.messages[0].blocks;
    expect(blocks.map((block) => block.target)).toEqual([
      'cargo test',
      'https://x',
      'unknown_tool',
    ]);
  });
});

describe('projectEvent：权限与提问', () => {
  const request = {
    t: 'permission_request',
    id: 'p1',
    call: { id: 'c1', name: 'fs.write', risk: 'write', input: { path: 'a.txt' } },
    risk: 'write',
  };

  it('permission_request 生成待决权限卡；没有 resolved 事件就不变', () => {
    const state = run([{ t: 'turn_start', turn: 1 }, request]);
    expect(state.messages[0].blocks[0]).toEqual({
      type: 'permission',
      id: 'p1',
      tool: 'fs.write',
      args: { path: 'a.txt' },
      risk: 'write',
      decision: null,
    });
  });

  it('permission_resolved 按 id 映射 once/session/deny', () => {
    const once = run([
      { t: 'turn_start', turn: 1 },
      request,
      { t: 'permission_resolved', id: 'p1', decision: { allow: true, scope: 'once' } },
    ]);
    const session = run([
      { t: 'turn_start', turn: 1 },
      request,
      { t: 'permission_resolved', id: 'p1', decision: { allow: true, scope: 'session' } },
    ]);
    const deny = run([
      { t: 'turn_start', turn: 1 },
      request,
      { t: 'permission_resolved', id: 'p1', decision: { allow: false, reason: '用户拒绝' } },
    ]);
    expect(once.messages[0].blocks[0].decision).toBe('once');
    expect(session.messages[0].blocks[0].decision).toBe('session');
    expect(deny.messages[0].blocks[0].decision).toBe('deny');
  });

  it('已决的权限卡不会被第二条 resolved 覆盖', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      request,
      { t: 'permission_resolved', id: 'p1', decision: { allow: true, scope: 'once' } },
      { t: 'permission_resolved', id: 'p1', decision: { allow: false, reason: '迟到的拒绝' } },
    ]);
    expect(state.messages[0].blocks[0].decision).toBe('once');
  });

  it('question 生成待答提问卡，线上没有「已应答」事件', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'question', id: 'q1', question: '选哪个？', options: ['甲', '乙'] },
    ]);
    expect(state.messages[0].blocks[0]).toEqual({
      type: 'question',
      id: 'q1',
      question: '选哪个？',
      options: ['甲', '乙'],
      answer: null,
    });
  });
});

describe('projectEvent：用量与错误', () => {
  it('usage 事件更新全局用量，contextTokens 取 input_tokens，budget 跟事件走', () => {
    const state = run([
      { t: 'turn_start', turn: 1 },
      { t: 'usage', input_tokens: 900, output_tokens: 21, budget: 27904 },
    ]);
    expect(state.usage).toEqual({
      inputTokens: 900,
      outputTokens: 21,
      contextTokens: 900,
      contextBudget: 27904,
    });
  });

  it('非致命 error 产生 toast；致命 error 记 fatalError 并带 error toast', () => {
    const soft = run([
      { t: 'turn_start', turn: 1 },
      { t: 'error', where: 'tool', message: 'spawn 失败', fatal: false },
    ]);
    expect(soft.fatalError).toBeNull();
    expect(soft.toasts).toEqual([{ body: '工具出错：spawn 失败', type: 'error' }]);

    const fatal = run([
      { t: 'turn_start', turn: 1 },
      { t: 'error', where: 'model', message: '端点不可用', fatal: true },
    ]);
    expect(fatal.fatalError).toBe('端点不可用');
    expect(fatal.toasts).toEqual([{ body: '端点不可用', type: 'error' }]);
  });

  it('通知类事件只产生 toast，不改消息；toasts 只描述当前事件，不累积', () => {
    const events = [
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '正文' },
      { t: 'memory_saved', count: 2 },
      { t: 'context_compacted', dropped: 7, summary_chars: 512 },
      {
        t: 'workflow_step',
        workflow: '调研流水',
        step: '收集',
        index: 1,
        total: 3,
        status: 'running',
      },
      { t: 'experiment_run', command_tail: 'echo METRIC', repeat: 1, of: 3, metrics: null },
    ];
    const state = run(events);
    expect(state.messages).toHaveLength(1);
    // run() 只保留最后一个事件的状态：末尾的 experiment_run 是唯一还挂着的 toast。
    expect(state.toasts.map((toast) => toast.body)).toEqual(['实验复跑 1/3：echo METRIC']);
    // 逐事件核对各自的 toast 文案。
    const perEvent = events.slice(2).map((event) => {
      let state2 = emptyProjection();
      state2 = projectEvent({ ...state2, messages: state.messages }, event, meta());
      return state2.toasts.map((toast) => toast.body)[0];
    });
    expect(perEvent).toEqual([
      '已记住 2 条记忆',
      '已压缩上下文：丢弃 7 条，摘要 512 字',
      '工作流「调研流水」步骤 1/3：收集',
      '实验复跑 1/3：echo METRIC',
    ]);
  });
});

describe('projectEvents：历史重放', () => {
  it('user_message + turn_start 重建用户气泡与助手气泡，重放不流式', () => {
    const state = projectEvents([
      {
        t: 'user_message',
        blocks: [
          { type: 'text', text: '你好' },
          { type: 'image', data: 'QUJD', mime: 'png' },
        ],
      },
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '你好，需要什么？' },
      { t: 'done', stop_reason: 'end_turn' },
    ]);

    expect(state.streaming).toBe(false);
    expect(state.messages).toHaveLength(2);
    expect(state.messages[0]).toMatchObject({ role: 'user' });
    expect(state.messages[0].blocks).toEqual([
      { type: 'text', text: '你好' },
      { type: 'image', src: 'data:image/png;base64,QUJD', alt: '附件图片' },
    ]);
    expect(state.messages[1]).toMatchObject({ role: 'assistant', isStreaming: false });
    expect(state.messages[1].blocks).toEqual([{ type: 'text', text: '你好，需要什么？' }]);
  });

  it('重放两次发送得到两组气泡，user_message 不串进助手流', () => {
    const state = projectEvents([
      { t: 'user_message', blocks: [{ type: 'text', text: '第一句' }] },
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '第一答' },
      { t: 'user_message', blocks: [{ type: 'text', text: '第二句' }] },
      { t: 'turn_start', turn: 1 },
      { t: 'text_delta', text: '第二答' },
      { t: 'done', stop_reason: 'end_turn' },
    ]);
    expect(state.messages.map((message) => message.role)).toEqual([
      'user',
      'assistant',
      'user',
      'assistant',
    ]);
    expect(state.messages[3].blocks).toEqual([{ type: 'text', text: '第二答' }]);
  });

  it('空事件列表得到空投影', () => {
    const state = projectEvents([]);
    expect(state.messages).toEqual([]);
    expect(state.streaming).toBe(false);
  });
});
