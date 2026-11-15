/**
 * 演示用 mock 数据。所有内容只存在于前端内存中，刷新页面即回到初始状态。
 *
 * 约定：
 * - Message.blocks 只使用 text / code / tool / permission / question / image / file 七种块。
 * - tool / permission / question 的字段形状见 docs/agent-ui-contract.md，改动前先读契约。
 * - 数据里允许出现主题色、路径和 data URI；组件里不允许写死这些值。
 */

const previewSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="960" height="600" viewBox="0 0 960 600">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#0a0d1c"/>
      <stop offset="1" stop-color="#151b30"/>
    </linearGradient>
    <linearGradient id="accent" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#7c8cff"/>
      <stop offset="1" stop-color="#22d3ee"/>
    </linearGradient>
  </defs>
  <rect width="960" height="600" fill="url(#bg)"/>
  <circle cx="170" cy="20" r="230" fill="#4f46e5" opacity="0.22"/>
  <circle cx="890" cy="90" r="210" fill="#06b6d4" opacity="0.16"/>
  <rect x="28" y="28" width="190" height="544" rx="18" fill="#ffffff" opacity="0.05"/>
  <rect x="48" y="52" width="110" height="14" rx="7" fill="#7c8cff" opacity="0.9"/>
  <rect x="48" y="92" width="148" height="30" rx="9" fill="#7c8cff" opacity="0.22"/>
  <rect x="48" y="138" width="132" height="12" rx="6" fill="#ffffff" opacity="0.28"/>
  <rect x="48" y="166" width="116" height="12" rx="6" fill="#ffffff" opacity="0.18"/>
  <rect x="48" y="194" width="140" height="12" rx="6" fill="#ffffff" opacity="0.18"/>
  <rect x="244" y="28" width="688" height="48" rx="14" fill="#ffffff" opacity="0.05"/>
  <rect x="264" y="45" width="192" height="12" rx="6" fill="#ffffff" opacity="0.32"/>
  <rect x="244" y="94" width="688" height="356" rx="20" fill="#ffffff" opacity="0.04"/>
  <rect x="282" y="126" width="360" height="64" rx="14" fill="#7c8cff" opacity="0.28"/>
  <rect x="300" y="146" width="240" height="10" rx="5" fill="#ffffff" opacity="0.62"/>
  <rect x="300" y="164" width="170" height="9" rx="4.5" fill="#ffffff" opacity="0.38"/>
  <rect x="312" y="218" width="520" height="118" rx="16" fill="#ffffff" opacity="0.06"/>
  <rect x="334" y="240" width="128" height="12" rx="6" fill="#22d3ee" opacity="0.8"/>
  <rect x="334" y="266" width="440" height="10" rx="5" fill="#ffffff" opacity="0.24"/>
  <rect x="334" y="286" width="388" height="10" rx="5" fill="#ffffff" opacity="0.18"/>
  <rect x="334" y="306" width="300" height="10" rx="5" fill="#ffffff" opacity="0.14"/>
  <rect x="312" y="356" width="420" height="58" rx="14" fill="#ffffff" opacity="0.05"/>
  <rect x="332" y="376" width="190" height="12" rx="6" fill="#ffffff" opacity="0.32"/>
  <rect x="560" y="374" width="80" height="16" rx="8" fill="#7c8cff" opacity="0.55"/>
  <rect x="244" y="468" width="688" height="78" rx="18" fill="#ffffff" opacity="0.07"/>
  <rect x="266" y="490" width="170" height="12" rx="6" fill="#ffffff" opacity="0.4"/>
  <rect x="266" y="512" width="380" height="10" rx="5" fill="#ffffff" opacity="0.16"/>
  <rect x="834" y="488" width="52" height="36" rx="10" fill="url(#accent)" opacity="0.9"/>
  <text x="28" y="586" font-family="Segoe UI, Microsoft YaHei, sans-serif" font-size="13" fill="#b8c4e0" opacity="0.8">LFTTDO · Agent Workspace preview</text>
</svg>`;

const previewImage = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(previewSvg)}`;

export const initialSessions = [
  {
    id: 'session-1',
    title: '整理 Agent 界面的需求',
    updatedAt: '2026-11-15T14:32:00+08:00',
    messages: [
      {
        id: 'message-1-1',
        role: 'user',
        createdAt: '2026-11-15T14:28:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '我想把 lfttdo 的 Agent 工作台做成一个可点击的前端原型，先从哪里入手？',
          },
        ],
      },
      {
        id: 'message-1-2',
        role: 'assistant',
        createdAt: '2026-11-15T14:32:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '先搭外壳，再补内容。建议顺序：\n\n1. 用 AppShell + SideNav 固定左侧会话栏；\n2. 主列从上到下放工作文件夹栏、消息流、输入坞、状态栏；\n3. 先把纯文本对话跑通，再逐类加入工具卡、权限卡和提问卡。\n\n这样每一层都能单独验收，不会一开始就陷进玻璃样式里。',
          },
        ],
      },
    ],
  },
  {
    id: 'session-2',
    title: '检查 mock 数据文件',
    updatedAt: '2026-11-15T11:20:00+08:00',
    messages: [
      {
        id: 'message-2-1',
        role: 'user',
        createdAt: '2026-11-15T11:16:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '看一下 src/data/mockData.js 里有哪些导出，别改文件。',
          },
        ],
      },
      {
        id: 'message-2-2',
        role: 'assistant',
        createdAt: '2026-11-15T11:20:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '我只读这个文件，先确认它的导出面。',
          },
          {
            type: 'tool',
            name: 'read_file',
            target: 'src/data/mockData.js',
            status: 'complete',
            duration: 640,
            additions: 0,
            deletions: 0,
            result: 'initialSessions\ninitialSettings\nmemories\nskills\nmcpServers\nhooks\nfileMentions\nmodels\nnewSession',
            errorMessage: '',
          },
          {
            type: 'text',
            text: '导出符合契约：4 条会话、3 条记忆、5 个技能、3 个 MCP 服务器、4 个 Hook，以及 15 个文件提及路径。',
          },
          {
            type: 'file',
            path: 'src/data/mockData.js',
            name: 'mockData.js',
          },
        ],
      },
    ],
  },
  {
    id: 'session-3',
    title: '运行构建并修掉报错',
    updatedAt: '2026-11-14T18:05:00+08:00',
    messages: [
      {
        id: 'message-3-1',
        role: 'user',
        createdAt: '2026-11-14T17:58:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '运行一下开发服务器，再跑构建看看有没有报错。',
          },
        ],
      },
      {
        id: 'message-3-2',
        role: 'assistant',
        createdAt: '2026-11-14T18:05:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '先确认开发服务器能起来，再跑一次生产构建。',
          },
          {
            type: 'tool',
            name: 'run_command',
            target: 'npm run dev -- --host 127.0.0.1',
            status: 'complete',
            duration: 1240,
            additions: 0,
            deletions: 0,
            result: 'VITE v8.3.8  ready in 312 ms\n\n➜  Local:   http://127.0.0.1:5173/',
            errorMessage: '',
          },
          {
            type: 'tool',
            name: 'run_command',
            target: 'npm run build',
            status: 'error',
            duration: 2860,
            additions: 0,
            deletions: 0,
            result: 'vite v8.3.8 building for production...\ntransforming...',
            errorMessage: 'src/App.jsx:12:3: Unexpected closing tag. Did you mean </VStack>?',
          },
          {
            type: 'text',
            text: '构建失败的原因在 src/App.jsx 的 JSX 闭合标签。我准备只改这一处，把多余的 </div> 换成 </VStack>。',
          },
          {
            type: 'permission',
            id: 'permission-3-1',
            tool: 'write_file',
            args: {
              path: 'src/App.jsx',
              patch: '- </div>\n+ </VStack>',
            },
            risk: 'write',
            decision: null,
          },
        ],
      },
    ],
  },
  {
    id: 'session-4',
    title: '生成发布说明草稿',
    updatedAt: '2026-11-13T09:48:00+08:00',
    messages: [
      {
        id: 'message-4-1',
        role: 'user',
        createdAt: '2026-11-13T09:42:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '帮我把这次改动的发布说明写出来，再给我一张界面截图。',
          },
        ],
      },
      {
        id: 'message-4-2',
        role: 'assistant',
        createdAt: '2026-11-13T09:48:00+08:00',
        blocks: [
          {
            type: 'text',
            text: '我按「新增 / 改进 / 修复」三段来写，截图先用 mock 的界面预览。',
          },
          {
            type: 'code',
            language: 'markdown',
            title: 'docs/release-notes.md',
            code: '# v0.2.0\n\n## 新增\n\n- Agent 工作台外壳：会话侧栏、工作文件夹栏、消息流、输入坞与状态栏。\n- 工具卡、权限卡、提问卡与图片/文件产物。\n- 设置、技能、MCP、Hooks 四个面板。\n\n## 改进\n\n- 统一深蓝黑玻璃主题 `lfttdo-noir`。\n- 输入区支持 `/` 技能与 `@` 文件提及。\n\n## 修复\n\n- 修正会话删除后自动选中的逻辑。\n- 修正构建时的 JSX 闭合标签错误。',
          },
          {
            type: 'image',
            src: previewImage,
            alt: '深色玻璃风格的 Agent 工作台预览',
          },
          {
            type: 'file',
            path: 'docs/release-notes.md',
            name: 'release-notes.md',
          },
          {
            type: 'question',
            id: 'question-4-1',
            question: '这份发布说明先发给谁？',
            options: ['团队内部评审', '公开渠道', '先存草稿'],
            answer: null,
          },
        ],
      },
    ],
  },
];

export const initialSettings = {
  endpoint: 'http://127.0.0.1:11434/v1',
  model: 'qwen2.5-coder:7b',
  hasApiKey: false,
  contextBudget: 120000,
  fontSize: 15,
  accent: '#7c8cff',
  permissionMode: 'rules',
  planMode: false,
  persona: '',
  roots: ['D:\\Projects\\demo'],
};

export const memories = [
  {
    id: 'memory-1',
    kind: 'preference',
    body: '界面和回复默认用中文，代码注释也用中文。',
    dueAt: null,
  },
  {
    id: 'memory-2',
    kind: 'fact',
    body: '练习项目位于 D:\\Projects\\demo，使用 React 19 + Vite 8。',
    dueAt: null,
  },
  {
    id: 'memory-3',
    kind: 'todo',
    body: '给会话列表补上导出 Markdown 的入口。',
    dueAt: '2026-11-20',
  },
];

export const skills = [
  {
    name: 'commit',
    description: '把暂存区改动整理成一条提交信息',
    argumentHint: '[范围]',
    enabled: true,
  },
  {
    name: 'review',
    description: '按审查清单检查当前改动',
    argumentHint: '[文件或分支]',
    enabled: true,
  },
  {
    name: 'explain',
    description: '用中文解释一段代码或报错',
    argumentHint: '<问题>',
    enabled: true,
  },
  {
    name: 'test',
    description: '为指定模块补一组测试',
    argumentHint: '[模块路径]',
    enabled: false,
  },
  {
    name: 'release-notes',
    description: '根据提交记录生成发布说明',
    argumentHint: '[版本号]',
    enabled: true,
  },
];

export const mcpServers = [
  {
    name: 'filesystem',
    transport: 'stdio',
    status: 'connected',
    trust: 'ask',
  },
  {
    name: 'github',
    transport: 'streamable-http',
    status: 'connecting',
    trust: 'allow',
  },
  {
    name: 'postgres',
    transport: 'stdio',
    status: 'disconnected',
    trust: 'block',
  },
];

export const hooks = [
  {
    id: 'hook-1',
    event: 'PreToolUse',
    command: 'node scripts/guard-write.mjs',
    trusted: true,
    source: 'user',
  },
  {
    id: 'hook-2',
    event: 'PostToolUse',
    command: 'npm run lint --silent',
    trusted: false,
    source: 'workspace',
  },
  {
    id: 'hook-3',
    event: 'SessionStart',
    command: 'node scripts/load-context.mjs',
    trusted: true,
    source: 'workspace',
  },
  {
    id: 'hook-4',
    event: 'Stop',
    command: 'node scripts/notify-done.mjs',
    trusted: false,
    source: 'user',
  },
];

export const fileMentions = [
  'src/App.jsx',
  'src/main.jsx',
  'src/data/mockData.js',
  'src/components/SessionSidebar.jsx',
  'src/components/WorkspaceBar.jsx',
  'src/components/ChatPane.jsx',
  'src/components/MessageTurn.jsx',
  'src/components/ToolCards.jsx',
  'src/components/Composer.jsx',
  'src/components/StatusBar.jsx',
  'src/components/panels/SettingsPanel.jsx',
  'src/components/panels/SkillsPanel.jsx',
  'src/components/panels/McpPanel.jsx',
  'src/components/panels/HooksPanel.jsx',
  'docs/agent-ui-contract.md',
];

export const models = [
  'qwen2.5-coder:7b',
  'llama3.1:8b',
  'deepseek-coder-v2:16b',
];

export function newSession(title = '新对话') {
  const id = `session-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

  return {
    id,
    title: title.trim() || '新对话',
    updatedAt: new Date().toISOString(),
    messages: [],
  };
}
