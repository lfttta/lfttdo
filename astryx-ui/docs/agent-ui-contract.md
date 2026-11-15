# Agent 界面共享契约

版本：v1（2026-11-15）
项目：`D:\aido\Astryx-界面制作\Astryx测试1`
权威来源：`docs/superpowers/specs/2026-11-15-agent-interface-design.md` + 本契约。

本文件是四个实现者的唯一共享边界。实现时以本文件的 props、回调参数和数据结构为准；不要凭记忆猜 Astryx 组件 props，也不要改本文件。契约没写到的地方，先运行 `npx astryx component <Name>` 或 `npx astryx docs <topic>` 核对，再把结论写进自己的实现说明，不要改契约。

---

## 0. 架构师已完成的交付物（只读）

| 文件 | 状态 | 说明 |
|---|---|---|
| `astryx-themes.js` | 已生成 | 已包含 `neutral` 与 `lfttdo-noir`，`defaultThemeSlug = "lfttdo-noir"`。不要手改。 |
| `src/themes/lfttdo-noir/**` | 已生成 | `astryx theme add` 写入的 app 副本；每次主题构建会重写。不要手改。 |
| `themes/lfttdo-noir/**` | 已生成 | 主题编辑器源与构建物；不要手改。 |
| `src/main.jsx` | 已重写 | 保留 `reset.css` / `astryx.css` import，使用 `<Theme theme={themes[defaultThemeSlug]} mode="dark">`。不要改。 |
| `src/data/mockData.js` | 已创建 | 导出 `initialSessions`、`initialSettings`、`memories`、`skills`、`mcpServers`、`hooks`、`fileMentions`、`models`、`newSession`。不要改。 |
| `package.json` | 已更新 | 已安装 `@heroicons/react@^2.2.0`。不要改。 |
| `docs/agent-ui-contract.md` | 本文件 | 架构师维护。不要改。 |

验证记录（本轮实际执行）：

- `npm install @heroicons/react@^2.2.0` → `added 1 package, and audited 173 packages`；`package.json` 依赖为 `"@heroicons/react": "^2.2.0"`。
- `node --check src/data/mockData.js` → 通过；动态 import 输出 9 个导出、4 条会话、15 条文件提及。
- `npx astryx doctor` → `pass: 18 / warn: 3 / fail: 0`。警告是 `theme-fonts`（外部 Figtree 字体）与 `cli-on-path`（`node_modules/.bin/astryx.cmd` 不是 CLI），不影响本轮 UI 实现。
- `npx astryx doctor --built dist`（最终）→ `pass: 19 / warn: 3 / fail: 0`；警告同上，另含 `themes` 汇总项，均不阻塞交付。
- 主题的 `--png` 检查失败（`ERR_THEME_NOT_CHECKED`：`Unknown file extension ".css"`），没有生成 PNG。详见 §12。

---

## 1. 文件所有权（必须遵守）

| 文件 | 唯一所有者 | 允许做的事 |
|---|---|---|
| `src/App.jsx` | **外壳实现者** | 整体重写为应用外壳与状态编排；只有此人可编辑。 |
| `src/components/SessionSidebar.jsx` | **外壳实现者** | 侧栏、会话行、面板入口。 |
| `src/components/WorkspaceBar.jsx` | **外壳实现者** | 工作文件夹栏。 |
| `src/components/ChatPane.jsx` | **消息流实现者** | 只渲染 `ChatMessageList`。 |
| `src/components/MessageTurn.jsx` | **消息流实现者** | 单条消息、气泡、日期以外的消息内容。 |
| `src/components/ToolCards.jsx` | **消息流实现者** | 工具卡、权限卡、提问卡、图片卡、文件卡。 |
| `src/components/QuestionDialog.jsx` | **消息流实现者** | 模型提问的居中弹窗；「稍后回答」后消息流只留紧凑回退提示。 |
| `src/components/Composer.jsx` | **输入与模拟实现者** | 输入坞、附件、`/` 与 `@` 菜单。 |
| `src/components/StatusBar.jsx` | **输入与模拟实现者** | token 与上下文占用条。 |
| `src/hooks/useMockAgent.js` | **输入与模拟实现者** | 唯一的 mock 回复/流式/权限/提问逻辑。 |
| `src/components/panels/SettingsPanel.jsx` | **面板实现者** | 设置面板。 |
| `src/components/panels/SkillsPanel.jsx` | **面板实现者** | 技能面板。 |
| `src/components/panels/McpPanel.jsx` | **面板实现者** | MCP 面板与 JSON 编辑器。 |
| `src/components/panels/HooksPanel.jsx` | **面板实现者** | Hooks 面板。 |
| `src/main.jsx`、`src/data/mockData.js`、`astryx-themes.js`、`src/themes/**`、`themes/**`、`package.json`、`AGENTS.md`、`docs/superpowers/specs/**`、本契约 | **架构师** | 实现者只读；不要编辑、不要格式化、不要重排。 |

规则：

- 四个实现者只改自己名下的文件；**不要修改别人的文件来解决自己的问题**。跨文件问题在结果里写明，由总控协调。
- 不新建 `src/components` 下的公共文件、样式文件或工具文件；`QuestionDialog.jsx` 已按本契约归入消息流实现者名下，不需要再新建其他公共组件。
- `src/App.jsx` 是唯一例外：只有外壳实现者可以写；另外三人按“不编辑 `App.jsx`”执行，不得覆盖、不得在它里面加 import。
- `src/main.jsx` 已经接好主题；不要让 App 再改 `defaultThemeSlug`。
- `src/data/mockData.js`、主题文件、`package.json`、`astryx-themes.js` 任何人都不要编辑；契约文件也不要编辑。需要改数据或主题时，在结果里说明，由总控处理。

---

## 2. 全局禁令

以下禁令对所有实现者生效：

1. **不要运行 `npm run build`、`vite build` 或 `astryx doctor --built`。** 集成完成后由总控统一构建与验收。
2. **不要运行 `npm install` / `npm uninstall`。** 依赖已固定；Heroicons 已安装。
3. **不要编辑 `src/main.jsx`、`src/data/mockData.js`、`astryx-themes.js`、`src/themes/**`、`themes/**`、`package.json`。**
4. **不要写 raw `<div>` / `<span>` 做布局或排版。** 布局只用 `VStack`、`HStack`、`Grid`、`Card`、`Section`、`AppShell`、`SideNav`、`ChatLayout` 等 Astryx 组件。
   - 唯一例外：图片用原生 `<img>`（放在 `AspectRatio` 里）；文件选择用一个 `hidden` 的 `<input type="file">`（不参与布局）。这两个例外不得扩散到按钮、文本、容器。
5. **组件代码里不要出现裸 hex / 裸 px。** 颜色、圆角、间距、字体、阴影全部走主题 token（`var(--color-*)`、`var(--radius-*)`、`var(--spacing-*)`）或组件 props。
   - `src/data/mockData.js` 里的主题色、路径、SVG data URI 不算组件代码，可以保留。
   - `SideNav` 的 `resizable` 数值、`Slider` 的 `min/max`、`defineTheme` 的 `typography.scale.base` 是 Astryx/主题 API 的数据值，不是 CSS；不要把它们改写成 `style={{ width: '...px' }}`。
   - 需要固定尺寸时用 `rem` 字符串，例如 `width="7.5rem"`、`maxWidth="20rem"`、`maxHeight="16rem"`。
6. **不要写 CSS 文件、不要 `@apply`、不要 `:root` 覆盖、不要 `!important`。** 玻璃效果由 `lfttdo-noir` 主题提供；组件里不要再写 `backdrop-filter`、渐变、阴影。
7. **不要使用 `xstyle`。** 本项目没有 StyleX 编译器；`xstyle` 会直接报错。也不要用 Tailwind/utility class。
8. **不要猜组件 props。** 使用本契约列出的签名；契约没列的 Astryx 组件，先 `npx astryx component <Name> --props` 核对。不要用 `Select`、`Radio`、`Checkbox`、`Image` 这些不存在的组件（见 §11）。
9. **不要做真实网络、真实文件、真实 MCP/Hooks 执行。** 所有“打开文件夹”“保存配置”“重启应用”“导入/导出”都是 toast + 内存状态。
10. **不要持久化。** 不用 `localStorage`、`sessionStorage`、IndexedDB、Cookie；刷新必须回到 `initialSessions` / `initialSettings`。
11. **不要用 `window.alert` / `window.confirm` / `window.prompt`。** 破坏性操作在行内二次确认；反馈用 `useToast`。
12. **不要新增路由、不要改 `index.html`、不要改 `vite.config.js`。**
13. **不要用 emoji 代替图标。** 图标统一用 `@heroicons/react/24/outline`，直接作为 JSX 组件传给 Astryx 的 `icon`/`startContent`/`endContent`。
14. **不要引入第三方状态库。** React state + `useMockAgent` 足够。
15. **不要用 `dangerouslySetInnerHTML`。** Markdown 用 `Markdown` 组件。

---

## 3. 顶层架构与数据流

```
main.jsx
└─ <Theme theme={themes[defaultThemeSlug]} mode="dark">
   └─ App
      └─ <Theme theme={runtimeTheme} mode="dark">        // 运行时字号/强调色
         ├─ <AppShell sideNav={<SessionSidebar … />}>
         │  └─ <VStack height="100%" gap={0}>
         │     ├─ <WorkspaceBar … />
         │     ├─ <ChatLayout composer={<Composer … />}>
         │     │  └─ <ChatPane … />                     // ChatPane 只返回 ChatMessageList
         │     └─ <StatusBar … />
         │  </AppShell>
         ├─ <SettingsPanel … />
         ├─ <SkillsPanel … />
         ├─ <McpPanel … />
         └─ <HooksPanel … />
```

职责边界：

- **App**：唯一持有 `sessions`、`activeId`、`settings`、`memories`、`skills`、`mcpServers`、`hooks`、`workspacePath`、`panel`、`composerValue`、`pendingImages`、`usage`、`mcpJson`、`mcpValidation`、`workspaceHooksEnabled`；调用一次 `useMockAgent`；把回调传给子组件。
- **SessionSidebar**：只渲染 `SideNav`；会话排序、行内重命名/删除确认、面板入口。
- **ChatPane**：只渲染 `ChatMessageList`；日期分隔和 `MessageTurn` 列表。
- **MessageTurn**：单条 `Message` 的气泡；文本/代码直接渲染，工具类块交给 `ToolCards`。
- **ToolCards**：工具卡、权限卡、提问卡、图片、文件产物；待回答提问由 `QuestionDialog` 承载时跳过内联卡，弹窗收起后显示紧凑的「继续回答」提示。
- **QuestionDialog**：居中弹窗显示待回答的提问；提交回答，或关闭后把问题交还消息流里的紧凑提示。
- **Composer**：受控输入、附件抽屉、`/` 技能菜单、`@` 文件菜单、发送/停止。
- **StatusBar**：token 统计 + 上下文占用条。
- **useMockAgent**：唯一产生助手回复、流式文本、工具状态、权限卡、提问卡的地方；不改 UI。
- **四个面板**：只读 props，不直接改 App 的 state；所有变更通过回调。

数据流：

```
Composer.onSend(text)
  → App.handleSend
  → useMockAgent.send(text, activeId, pendingImages)
  → setSessions(...)（追加用户消息 + 助手消息）
  → 定时器推送文本/工具/权限/提问
  → ChatPane 重渲染 MessageTurn / ToolCards
  → setUsage(...) 更新 StatusBar
权限卡按钮 → App.handlePermission → useMockAgent.respondPermission
提问弹窗提交 → App.handleQuestionAnswer → useMockAgent.answerQuestion
提问弹窗「稍后回答」→ App 记录 dismissedQuestionId → 消息流显示「继续回答」紧凑提示
重试按钮 → App.handleRetry → useMockAgent.retryLast(activeId)
```

---

## 4. 数据类型（唯一真相）

以下类型是 JavaScript 对象形状；不要加字段、不要改名。所有 `id` 都是非空字符串；所有时间都是 ISO 8601 字符串。

### 4.1 Session / Message / Block

```js
Session = {
  id: string,
  title: string,
  updatedAt: string,              // ISO，排序用
  messages: Message[],
}

Message = {
  id: string,
  role: 'user' | 'assistant',
  createdAt: string,              // ISO
  blocks: Block[],
  isStreaming?: boolean,          // 默认 false；仅运行时助手消息使用
  stopped?: boolean,              // 默认 false；用户点停止后为 true
}

Block =
  | TextBlock
  | CodeBlock
  | ToolBlock
  | PermissionBlock
  | QuestionBlock
  | ImageBlock
  | FileBlock

TextBlock = {
  type: 'text',
  text: string,
}

CodeBlock = {
  type: 'code',
  language: string,               // 'markdown' | 'javascript' | 'json' | 'text' | ...
  title?: string,                 // 可选
  code: string,
}

ToolBlock = {
  type: 'tool',
  name: string,                   // 'read_file' | 'run_command' | 'write_file' | ...
  target: string,                 // 文件路径 / 命令 / 查询
  status: 'pending' | 'running' | 'complete' | 'error',
  duration: number,               // 毫秒；未完成可为 0
  additions: number,              // 非编辑工具为 0
  deletions: number,              // 非编辑工具为 0
  result: string,                 // 纯文本输出；没有则 ''
  errorMessage: string,           // 没有错误则 ''
}

PermissionBlock = {
  type: 'permission',
  id: string,                     // 全局唯一
  tool: string,
  args: Record<string, unknown>,   // 必须是可 JSON.stringify 的对象
  risk: 'read' | 'write' | 'exec',
  decision: null | 'once' | 'session' | 'deny',
}

QuestionBlock = {
  type: 'question',
  id: string,                     // 全局唯一
  question: string,
  options: string[],              // 2–6 条
  answer: null | string,
}

ImageBlock = {
  type: 'image',
  src: string,                    // data URI / URL
  alt: string,
}

FileBlock = {
  type: 'file',
  path: string,
  name: string,                   // basename
}
```

注意：

- `ToolBlock` 没有 `id`；React key 用消息 id + 数组下标或 `name/target` 组合。
- `ToolBlock.result` / `errorMessage` 是字符串，不是对象；渲染时放进 `CodeBlock`。
- `PermissionBlock.args` 是对象；渲染时 `JSON.stringify(args, null, 2)`。
- `QuestionBlock.options` 是字符串数组；`answer` 是选项原文或用户自定义输入。
- `Message.isStreaming` / `stopped` 只在运行时出现；`mockData.js` 初始数据可以省略（默认 false）。

### 4.2 Settings / Usage / 附件

```js
Settings = {
  endpoint: string,
  model: string,
  hasApiKey: boolean,
  contextBudget: number,
  fontSize: number,
  accent: string,
  permissionMode: 'rules' | 'ask' | 'trusted',
  planMode: boolean,
  persona: string,
  roots: string[],
}

Usage = {
  inputTokens: number,
  outputTokens: number,
  contextTokens: number,
}

PendingImage = {
  id: string,
  name: string,
  url: string,                    // FileReader 读出的 data URL；不是 blob:
}

PanelId = 'settings' | 'skills' | 'mcp' | 'hooks'

PermissionDecision = 'once' | 'session' | 'deny'
```

`initialSettings` 的精确值（来自 `src/data/mockData.js`）：

```js
{
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
}
```

`usage` 初始值由 App 自己建：`{ inputTokens: 0, outputTokens: 0, contextTokens: 0 }`。

### 4.3 其余 mock 类型

```js
Memory = {
  id: string,
  kind: 'fact' | 'preference' | 'todo',
  body: string,
  dueAt: string | null,           // 'YYYY-MM-DD' 或 null
}

Skill = {
  name: string,
  description: string,
  argumentHint: string,
  enabled: boolean,
}

McpServer = {
  name: string,
  transport: 'stdio' | 'streamable-http',
  status: 'connected' | 'connecting' | 'disconnected',
  trust: 'allow' | 'ask' | 'block',
}

Hook = {
  id: string,
  event: 'PreToolUse' | 'PostToolUse' | 'SessionStart' | 'Stop',
  command: string,
  trusted: boolean,
  source: 'user' | 'workspace',
}

McpValidation = {
  status: 'idle' | 'valid' | 'invalid',
  message: string,
}
```

`src/data/mockData.js` 的覆盖情况（实现者可用它自测）：

| 会话 | 覆盖 |
|---|---|
| `session-1` 整理 Agent 界面的需求 | 纯文本、多段 Markdown |
| `session-2` 检查 mock 数据文件 | `read_file` 工具卡、结果、文件产物 |
| `session-3` 运行构建并修掉报错 | `run_command` 完成 + 错误、`permission` 待处理（write） |
| `session-4` 生成发布说明草稿 | `code`、`image`、`file`、`question` 待回答 |

`memories` 3 条、`skills` 5 条、`mcpServers` 3 条、`hooks` 4 条、`fileMentions` 15 条、`models` 3 个。具体内容以文件为准，不要改。

---

## 5. 主题与视觉约定

### 5.1 主题事实（来自生成物）

`astryx-themes.js`：

```js
export const themes = {
  "neutral": neutralTheme,
  "lfttdo-noir": lfttdoNoirTheme,
};
export const defaultThemeSlug = "lfttdo-noir";
```

`src/themes/lfttdo-noir/lfttdo-noir.css` 的关键 token（实际文件行号）：

- `src/themes/lfttdo-noir/lfttdo-noir.css:197` → `--color-background-body: #0a0d1c;`
- `:201` → `--color-accent: light-dark(#7c8cff, #7c8cff);`
- `:196` → `--color-background-surface: light-dark(#1f2535, #202533);`
- `:198` → `--color-background-card: light-dark(#161b29, #171b27);`
- `:627` → `background: radial-gradient(circle at 12% 0%, rgb(79 70 229 / 0.32), transparent 42%), radial-gradient(circle at 92% 8%, rgb(6 182 212 / 0.2), transparent 38%), var(--color-background-body);`
- `card` / `dialog` / `drawer` / `popover` / `dropdown-menu` 等表面：`color-mix(in srgb, var(--color-background-card) 72%, transparent)` + `backdrop-filter: blur(14px) saturate(1.4)` + `1px solid rgb(255 255 255 / 0.22)`。

也就是说：**深蓝黑底、靛蓝强调色、两团低饱和径向光晕、半透明玻璃表面、白色细边**已经由主题提供。组件里不要再写这些。

### 5.2 组件里允许的样式方式

1. 先用组件 props：`variant`、`padding`、`gap`、`elevation`、`size`、`color`、`width`、`maxWidth`、`isLabelHidden` 等。
2. 需要 token 时用 `style`，但只允许 `var(--...)` 或数据值，例如：

```jsx
<img
  src={block.src}
  alt={block.alt}
  style={{
    width: '100%',
    height: '100%',
    objectFit: 'cover',
    borderRadius: 'var(--radius-element)',
  }}
/>
```

3. 固定尺寸用 `rem` 字符串，例如 `width="7.5rem"`、`maxWidth="20rem"`、`maxHeight="16rem"`、`width="4.5rem"`。
4. 不要写 `style={{ color: '#fff' }}`、`style={{ width: 320 }}`、`style={{ padding: '12px 16px' }}`。

### 5.3 运行时字号与强调色

App 必须用 `defineTheme` 把 `settings.fontSize` / `settings.accent` 真正作用到主题层，然后再包一层 `<Theme>`：

```jsx
import { defineTheme, Theme } from '@astryxdesign/core/theme';
import { themes, defaultThemeSlug } from '../astryx-themes';

const runtimeTheme = useMemo(
  () =>
    defineTheme({
      extends: themes[defaultThemeSlug],
      typography: { scale: { base: settings.fontSize, ratio: 1.2 } },
      tokens: {
        '--color-accent': settings.accent,
        '--color-text-accent': settings.accent,
        '--color-icon-accent': settings.accent,
        '--focus-outline-color': settings.accent,
      },
    }),
  [settings.fontSize, settings.accent]
);

return <Theme theme={runtimeTheme} mode="dark">…</Theme>;
```

- `settings.fontSize` 只允许 13–18。
- `settings.accent` 只允许 4 个预设：`#7c8cff`（靛蓝）、`#22d3ee`（青色）、`#f472b6`（粉色）、`#34d399`（绿色）。不要做任意 color picker；这四个在深底上都可读。
- 不要用 `:root` 或 `document.documentElement.style` 改主题。

---

## 6. Astryx 组件使用速查

### 6.1 import 路径

```js
import { AppShell } from '@astryxdesign/core/AppShell';
import { SideNav, SideNavHeading, SideNavItem, SideNavSection } from '@astryxdesign/core/SideNav';
import {
  ChatLayout,
  ChatMessage,
  ChatMessageBubble,
  ChatMessageList,
  ChatMessageMetadata,
  ChatSystemMessage,
  ChatToolCalls,
} from '@astryxdesign/core/Chat';
import { AspectRatio } from '@astryxdesign/core/AspectRatio';
import { Card } from '@astryxdesign/core/Card';
import { Button } from '@astryxdesign/core/Button';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Badge } from '@astryxdesign/core/Badge';
import { Token } from '@astryxdesign/core/Token';
import { Text } from '@astryxdesign/core/Text';
import { Heading } from '@astryxdesign/core/Heading';
import { VStack } from '@astryxdesign/core/VStack';
import { HStack } from '@astryxdesign/core/HStack';
import { Grid } from '@astryxdesign/core/Grid';
import { ProgressBar } from '@astryxdesign/core/ProgressBar';
import { Drawer } from '@astryxdesign/core/Drawer';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { Switch } from '@astryxdesign/core/Switch';
import { Selector } from '@astryxdesign/core/Selector';
import { RadioList, RadioListItem } from '@astryxdesign/core/RadioList';
import { SegmentedControl, SegmentedControlItem } from '@astryxdesign/core/SegmentedControl';
import { Slider } from '@astryxdesign/core/Slider';
import { CodeBlock } from '@astryxdesign/core/CodeBlock';
import { Markdown } from '@astryxdesign/core/Markdown';
import { Banner } from '@astryxdesign/core/Banner';
import { Avatar } from '@astryxdesign/core/Avatar';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { List, ListItem } from '@astryxdesign/core/List';
import { MoreMenu } from '@astryxdesign/core/MoreMenu';
import { Timestamp } from '@astryxdesign/core/Timestamp';
import { useToast } from '@astryxdesign/core/Toast';
import { createStaticSource, TypeaheadItem } from '@astryxdesign/core/Typeahead';
import { Theme, defineTheme } from '@astryxdesign/core/theme';
```

图标：

```js
import {
  ArrowPathIcon,
  BoltIcon,
  CheckIcon,
  Cog6ToothIcon,
  CpuChipIcon,
  DocumentDuplicateIcon,
  DocumentTextIcon,
  FolderIcon,
  PencilIcon,
  PhotoIcon,
  PlusIcon,
  ServerStackIcon,
  SparklesIcon,
  TrashIcon,
  XMarkIcon,
} from '@heroicons/react/24/outline';
```

Astryx 的 `icon` prop 接受 `ComponentType<SVGProps>`，所以直接传 Heroicon 组件，例如 `icon={<PlusIcon />}`。不要传字符串（字符串只用于 Astryx 内置语义图标名）。

### 6.2 关键 props（已核对 0.7.16）

| 组件 | 关键 props |
|---|---|
| `AppShell` | `children`、`sideNav`、`topNav`、`banner`、`contentPadding`、`height='fill'`、`variant='wash'|'surface'|'section'|'elevated'` |
| `SideNav` | `header`、`topContent`、`children`、`footer`、`footerIcons`、`collapsible`（`{isCollapsed,onCollapsedChange,hasButton,buttonLabel}`）、`resizable`（`{defaultWidth,minWidth,maxWidth,autoSaveId}`） |
| `SideNavHeading` | `heading`（必填）、`subheading`、`icon`、`menu`、`headerEndContent` |
| `SideNavSection` | `title`（必填）、`subtitle`、`children`、`endContent` |
| `SideNavItem` | `label`（必填）、`icon`、`selectedIcon`、`isSelected`、`onClick`、`actions`、`endContent`、`children`、`size` |
| `ChatLayout` | `children`、`composer`（必填）、`emptyState`、`scrollRef`、`density='compact'|'balanced'|'spacious'` |
| `ChatMessageList` | `children`、`emptyState`、`scrollToTopAction`、`density`、`gap`、`align`、`isStreaming` |
| `ChatMessage` | `sender`（必填）、`children`（必填）、`avatar`、`name`、`metadata`、`density` |
| `ChatMessageBubble` | `children`（必填）、`variant='filled'|'ghost'`、`name`、`metadata`、`group`、`width` |
| `ChatMessageMetadata` | `timestamp`、`footer`、`status='sending'|'sent'|'delivered'|'read'|'error'` |
| `ChatSystemMessage` | `children`（必填）、`variant='default'|'divider'`、`icon` |
| `ChatToolCalls` | `calls`（必填，`ChatToolCallItem[]`）、`label`、`isExpanded`、`defaultIsExpanded`、`onExpandedChange` |
| `ChatComposer` | `onSubmit`（必填）、`onStop`、`isStopShown`、`value`、`onChange`、`placeholder`、`isDisabled`、`density`、`elevation`、`drawer`、`headerActions`、`headerContext`、`input`、`footerActions`、`sendActions`、`sendButton`、`status`、`statusPosition` |
| `ChatComposerInput` | `handleRef`、`value`、`onChange`、`placeholder`、`maxRows`、`triggers`、`debounceMs`、`hasHistory`、`label`、`isDisabled`、`onPaste`、`pasteAsToken`、`onFiles`、`onSubmit`、`onKeyDown`、`enterKeyHint` |
| `ChatComposerDrawer` | `children`（必填）、`count`、`label`、`collapsedSummary`、`isCollapsed`、`defaultIsCollapsed`、`onCollapsedChange` |
| `Card` | `children`、`padding`、`variant='default'|'transparent'|'muted'|'blue'|...`、`elevation='none'|'low'|'med'|'high'`、`width`、`maxWidth`、`minHeight` |
| `Button` | `label`（必填）、`variant`、`size`、`elevation`、`isLoading`、`isInterruptible`、`isDisabled`、`icon`、`isIconOnly`、`width`、`children`、`endContent`、`tooltip`、`onClick`、`clickAction`、`type` |
| `IconButton` | `label`（必填）、`icon`（必填）、`variant`、`size`、`elevation`、`isLoading`、`isDisabled`、`tooltip`、`onClick`、`clickAction`、`width` |
| `Badge` | `label`（必填）、`variant`、`icon` |
| `Token` | `label`（必填）、`size`、`color`、`icon`、`isDisabled`、`onRemove`、`onClick`、`href`、`description`、`endContent` |
| `Text` | `type`、`children`、`size`、`color`、`weight`、`display`、`as`、`maxLines`、`hasTruncateTooltip`、`wordBreak`、`textWrap`、`justify` |
| `Heading` | `level=1..6`（核对过的用法）、`children` |
| `VStack` / `HStack` | `gap`、`padding*`、`isScrollable`、`width`、`height`、`maxWidth`、`minHeight`、`hAlign`/`vAlign`、`justify`、`align`、`wrap`、`children` |
| `Grid` | `columns`、`gap`、`rowGap`、`columnGap`、`children` |
| `ProgressBar` | `label`（必填）、`value`、`max`、`isLabelHidden`、`hasValueLabel`、`formatValueLabel`、`variant='accent'|'success'|'warning'|'error'|'neutral'`、`isIndeterminate`、`marks` |
| `Drawer` | `isOpen`（必填）、`onOpenChange`（必填）、`label`（必填）、`children`（必填）、`side='start'|'end'`、`width`、`hasScrim`、`purpose='required'|'form'|'info'`、`padding` |
| `TextInput` | `label`（必填）、`value`（必填）、`onChange`、`type`、`size`、`isLabelHidden`、`description`、`placeholder`、`status`、`hasClear`、`hasAutoFocus`、`width`、`onEnter` |
| `TextArea` | `label`（必填）、`value`（必填）、`onChange`、`rows`、`maxLength`、`isLabelHidden`、`description`、`placeholder`、`status`、`width` |
| `Switch` | `label`（必填）、`value`（必填）、`onChange`、`isLabelHidden`、`description`、`size`、`isDisabled`、`labelPosition`、`labelSpacing` |
| `Selector` | `label`（必填）、`options`（必填）、`value`、`onChange`、`hasClear`、`hasSearch`、`placeholder`、`size`、`variant`、`isDisabled`、`isLabelHidden`、`description`、`width` |
| `RadioList` | `label`（必填）、`value`（必填）、`onChange`（必填）、`children`（必填）、`orientation`、`isDisabled`、`size`、`isLabelHidden`、`description` |
| `RadioListItem` | `label`（必填）、`value`（必填）、`description`、`isDisabled`、`startContent`、`endContent` |
| `SegmentedControl` | `value`（必填）、`onChange`（必填）、`label`（必填）、`size`、`layout`、`isDisabled`、`children` |
| `SegmentedControlItem` | `value`（必填）、`label`（必填）、`isLabelHidden`、`icon`、`isDisabled` |
| `Slider` | `label`（必填）、`value`（必填）、`onChange`、`onChangeEnd`、`min`、`max`、`step`、`formatValue`、`valueDisplay`、`marks`、`isDisabled` |
| `CodeBlock` | `code`（必填）、`language`、`title`、`hasLanguageLabel`、`hasLineNumbers`、`isDiff`、`hasCopyButton`、`onCopy`、`onCopyError`、`maxHeight`、`size`、`width`、`container` |
| `Markdown` | `children`（必填 string）、`display`、`density`、`headingLevelStart`、`isStreaming`、`onLinkClick`、`contentWidth`、`contentAlign` |
| `Banner` | `status`（必填）、`title`（必填）、`description`、`icon`、`isDismissable`、`onDismiss`、`endContent`、`container` |
| `Avatar` | `name`、`src`、`alt`、`size`、`shape`、`tooltip` |
| `EmptyState` | `title`（必填）、`description`、`icon`、`actions`、`headingLevel`、`isCompact` |
| `List` | `children`、`density`、`hasDividers`、`edge`、`header` |
| `ListItem` | `label`（必填）、`description`、`startContent`、`endContent`、`onClick`、`isDisabled`、`isSelected`、`itemKey` |
| `MoreMenu` | `items`（必填）、`label`、`variant`、`size`、`icon`、`isDisabled`、`placement`、`alignment`、`onOpenChange` |
| `Timestamp` | `value`（必填）、`format`、`timezoneID`、`autoThreshold`、`hasTooltip`、`isTimezoneShown`、`isLive` |
| `useToast` | 返回 `showToast(options)`；options：`body`、`type='info'|'error'`、`isAutoHide`、`autoHideDuration`、`endContent`、`uniqueID`、`collisionBehavior` |

`ChatToolCalls` 的 `calls` 项形状（映射用）：

```js
{
  key: string,
  name: string,
  status: 'pending' | 'running' | 'complete' | 'error',
  target: string,
  duration: number,
  additions: number,
  deletions: number,
  errorMessage: string,
  resultDetail: ReactNode,        // 用 <CodeBlock code={tool.result} ... />
}
```

`Drawer` 没有 header 子组件；需要标题时自己组合 `HStack` + `Heading` + `IconButton`。

`ChatComposerInput` 的 trigger 形状（来自 0.7.16 source）：

```js
const trigger = {
  character: '/',
  searchSource: createStaticSource(items), // items: SearchableItem[]
  renderItem: (item) => <TypeaheadItem item={item} description="…" />,
  onSelect: (item) => string | { value, label, variant? }, // 返回 string = 纯文本
  emptySearchText: '没有匹配项',
  menuLabel: '技能快捷菜单',
};
```

`createStaticSource` 的 item 形状：`{ id, label, auxiliaryData? }`。`onSelect` 返回字符串时，插入纯文本；这正是本项目 `/` 技能引导语需要的。

### 6.3 使用约定

- **`Badge` 只用于数量**（如附件数、未读数）。风险、状态、来源、transport 等离散标签用 `Token`。这是 AGENTS.md 的规则，也避免 Badge 语义被滥用。
- 密集列表（技能、MCP、Hooks、记忆、文件提及）用 `List`/`ListItem`，不要用 Card 包列表。
- 权限卡、提问卡、文件卡是离散卡片，用 `Card`；工具调用用 `ChatToolCalls`。
- 所有可点击的图标控件用 `IconButton` 并给 `label`；不要用裸 `<button>`。
- `gap` / `padding` 传数字（spacing step），不要传字符串。`VStack gap={3}` 正确，`gap="3"` 错误。
- `width` / `maxWidth` / `maxHeight` 传 `SizeValue`；数字会被当成 px，所以本项目统一用 `rem` 字符串或 `100%`。
- `Markdown` / `CodeBlock` 的 `children` / `code` 必须是字符串，不要传 ReactNode。
- `Text` 的 `as` 默认 `span`；需要段落时用 `Markdown` 或 `Text as="p"`。不要用 `<p>`/`<div>` 包布局。
- `useToast` 在组件内调用；不要在最顶层 App 的同一组件里调用 `LayerProvider`（本项目不需要手动 LayerProvider）。

---

## 7. 组件契约

### 7.1 `SessionSidebar`

文件：`src/components/SessionSidebar.jsx`
所有者：外壳实现者

```js
SessionSidebar({
  sessions,          // Session[]
  activeId,          // string
  collapsed,         // boolean
  onToggleCollapse,  // () => void
  onSelect,          // (sessionId: string) => void
  onCreate,          // () => void
  onRename,          // (sessionId: string, title: string) => void
  onDelete,          // (sessionId: string) => void
  onExport,          // (sessionId: string) => void
  onOpenPanel,       // (panel: 'settings' | 'skills' | 'mcp' | 'hooks') => void
})
```

渲染与行为：

1. 渲染 `SideNav`：
   - `header={<SideNavHeading heading="LFTTDO" subheading="Agent 工作台" icon={<SparklesIcon />} />}`
   - `resizable={{ defaultWidth: 240, minWidth: 200, maxWidth: 320 }}`。不要传 `autoSaveId`：本项目不持久化（刷新必须回到初始状态）。
   - `collapsible={{ isCollapsed: collapsed, onCollapsedChange: () => onToggleCollapse(), hasButton: true, buttonLabel: collapsed ? '展开侧栏' : '收起侧栏' }}`
   - `footerIcons` 放四个 `IconButton`（`size="sm"`、`variant="ghost"`）：
     - `label="设置"`、`icon={<Cog6ToothIcon />}`、`onClick={() => onOpenPanel('settings')}`
     - `label="技能"`、`icon={<SparklesIcon />}`、`onClick={() => onOpenPanel('skills')}`
     - `label="MCP"`、`icon={<ServerStackIcon />}`、`onClick={() => onOpenPanel('mcp')}`
     - `label="Hooks"`、`icon={<BoltIcon />}`、`onClick={() => onOpenPanel('hooks')}`
   - 收起态也保留四个 `IconButton`（`SideNav` 的 footerIcons 会处理），每个都有可访问名称。
2. `topContent`：
   - 展开：`<Button label="新建对话" variant="primary" icon={<PlusIcon />} width="100%" onClick={onCreate} />`
   - 收起：`<IconButton label="新建对话" icon={<PlusIcon />} variant="primary" onClick={onCreate} />`
3. 会话列表：
   - 用 `useMemo` 复制并按 `Date.parse(updatedAt)` 倒序排序；**不要原地 sort `sessions`**。
   - `SideNavSection title="会话列表"`。
   - 普通行：`SideNavItem label={session.title} isSelected={session.id === activeId} onClick={() => onSelect(session.id)} actions={<MoreMenu size="sm" items={[...]} />} />`。
   - `MoreMenu.items`：
     ```js
     [
       { label: '重命名', onClick: () => setRenamingId(session.id) },
       { label: '导出', onClick: () => onExport(session.id) },
       { type: 'divider' },
       { label: '删除', variant: 'destructive', onClick: () => setDeletingId(session.id) },
     ]
     ```
   - 重命名行（`renamingId === session.id`）：用 `HStack gap={1} align="center"` + `TextInput label="重命名" isLabelHidden value={draft} onChange={setDraft} onEnter={commit} width="100%"` + `IconButton label="提交重命名" icon={<CheckIcon />}` + `IconButton label="取消" icon={<XMarkIcon />}`。`commit` 只在 `draft.trim()` 非空时调用 `onRename(session.id, draft.trim())`；然后清空本地 state。
   - 删除行（`deletingId === session.id`）：用 `VStack gap={2}` + `Text>删除会话？</Text>` + `HStack gap={2}` + `Button label="确认删除" variant="destructive" size="sm" onClick={() => { onDelete(session.id); setDeletingId(null); }}` + `Button label="取消" variant="ghost" size="sm" onClick={() => setDeletingId(null)}`。**只有确认后才调用 `onDelete`**；不要用 `window.confirm`。
   - 行内不能同时出现重命名和删除确认；打开一个时清掉另一个。
4. 空列表：
   - App 保证 `sessions.length >= 1`。如果收到空数组，渲染 `null` 或空 section，不要显示“空态”；由 App 负责补一条“新对话”。
5. 排序后点击 `onSelect`，不要改 `updatedAt`。

### 7.2 `WorkspaceBar`

文件：`src/components/WorkspaceBar.jsx`
所有者：外壳实现者

```js
WorkspaceBar({
  workspacePath,   // string | null
  onChoose,        // () => void
  onClear,         // () => void
  onReveal,        // () => void
})
```

渲染与行为：

- 返回一个 `HStack`：`align="center"`、`gap={3}`、`paddingInline={4}`、`paddingBlock={2}`、`width="100%"`。
- 左侧：
  - `workspacePath` 为 `null`：`<Text type="label" color="secondary">默认位置</Text>`
  - 否则：`<Text type="label" color="secondary">工作文件夹</Text>` + `<Token label={workspacePath} icon={<FolderIcon />} onClick={onReveal} size="md" />`
- 右侧：
  - `<Button label="选择文件夹" variant="secondary" size="sm" onClick={onChoose} />`
  - `<Button label="清除" variant="ghost" size="sm" onClick={onClear} isDisabled={!workspacePath} />`
- 路径很长时由 `Token`/`Text` 自身截断；不要加 `style={{ maxWidth: ... }}`。
- 不要在这里读文件系统；`onReveal` 只触发 App 的 toast。

### 7.3 `App`（外壳编排）

文件：`src/App.jsx`
所有者：外壳实现者

App 不是 UI 组件契约的一部分，但四个实现者的集成依赖它。按以下状态和回调实现：

#### 状态

```js
const [sessions, setSessions] = useState(initialSessions);
const [activeId, setActiveId] = useState(initialSessions[0].id);
const [collapsed, setCollapsed] = useState(false); // <900px 时用 matchMedia 自动置 true（可选）
const [settings, setSettings] = useState(initialSettings);
const [memories, setMemories] = useState(initialMemories);
const [skills, setSkills] = useState(initialSkills);
const [mcpServers, setMcpServers] = useState(initialMcpServers);
const [hooks, setHooks] = useState(initialHooks);
const [workspacePath, setWorkspacePath] = useState(initialSettings.roots[0] ?? null);
const [panel, setPanel] = useState(null);            // PanelId | null
const [composerValue, setComposerValue] = useState('');
const [pendingImages, setPendingImages] = useState([]); // PendingImage[]
const [usage, setUsage] = useState({ inputTokens: 0, outputTokens: 0, contextTokens: 0 });
const [mcpJson, setMcpJson] = useState(makeInitialMcpJson(initialMcpServers));
const [mcpValidation, setMcpValidation] = useState({ status: 'idle', message: '' });
const [workspaceHooksEnabled, setWorkspaceHooksEnabled] = useState(false);
const [dismissedQuestionId, setDismissedQuestionId] = useState(null);

const { showToast } = useToast();
const agent = useMockAgent({ sessions, settings, setSessions, setUsage });
```

`makeInitialMcpJson(servers)` 是本文件内的纯函数，返回类似：

```json
{
  "mcpServers": {
    "filesystem": { "transport": "stdio", "trust": "ask", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "D:\\Projects\\demo"] },
    "github": { "transport": "streamable-http", "trust": "allow", "url": "https://api.githubcopilot.com/mcp/" },
    "postgres": { "transport": "stdio", "trust": "block", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-postgres"] }
  }
}
```

`activeSession` 用 `sessions.find(s => s.id === activeId) ?? sessions[0]`；不要假设 `find` 一定有结果。

`pendingQuestion` 从 `activeSession` 的消息中倒序查找：跳过 `stopped` 消息，取最后一条 `type === 'question'` 且 `answer == null` 的块。`QuestionDialog` 在 `pendingQuestion && pendingQuestion.id !== dismissedQuestionId` 时打开；提交回答时清空 `dismissedQuestionId`，关闭时把它设为当前问题 id，消息流显示紧凑的「模型提问 · 待回答」提示。点「继续回答」把 `dismissedQuestionId` 清空，弹窗重新打开。

#### 回调

```js
onToggleCollapse: () => setCollapsed(v => !v)     // collapsed 也可放在 App state
onSelect: (id) => { setActiveId(id); setPanel(null); }
onCreate: () => {
  const next = newSession('新对话');
  setSessions(prev => [next, ...prev]);
  setActiveId(next.id);
  setComposerValue('');
  setPendingImages([]);
  setUsage({ inputTokens: 0, outputTokens: 0, contextTokens: 0 });
}
onRename: (id, title) => setSessions(prev => prev.map(s => s.id === id ? { ...s, title } : s))
onDelete: (id) => {
  const remaining = sessions.filter(s => s.id !== id);
  if (remaining.length === 0) {
    const fresh = newSession('新对话');
    setSessions([fresh]);
    setActiveId(fresh.id);
  } else {
    setSessions(remaining);
    if (id === activeId || !remaining.some(s => s.id === activeId)) {
      const mostRecent = [...remaining].sort(
        (a, b) => (Date.parse(b.updatedAt) || 0) - (Date.parse(a.updatedAt) || 0)
      )[0];
      setActiveId(mostRecent.id);
    }
  }
  setComposerValue('');
  setPendingImages([]);
}
onExport: (id) => exportSessionMarkdown(id)   // Blob + 下载 + toast
onOpenPanel: (next) => setPanel(prev => prev === next ? null : next)
onClosePanel: () => setPanel(null)
```

`exportSessionMarkdown(id)`：

- 找到 session；标题清洗非法字符：`title.replace(/[\\/:*?"<>|]/g, '-')`。
- 内容至少包含：`# ${title}`、每轮的 `## 用户` / `## 助手`、文本块原文、代码块 fenced code、工具块 `name`/`target`/`status`/`result`、权限/提问的最终状态。
- 用 `new Blob([markdown], { type: 'text/markdown;charset=utf-8' })`、`URL.createObjectURL`、临时 `<a download>` 点击，然后 `URL.revokeObjectURL`。
- 成功 toast `已导出：${filename}`；失败 toast `导出失败`。

`onChoose`：`setWorkspacePath('D:\\Projects\\demo\\agent-ui')` + toast `已选择工作文件夹`。
`onClear`：`setWorkspacePath(null)` + toast `已清除工作文件夹`。
`onReveal`：toast `已在资源管理器中显示`。
`onBanner`（如预留）：不需要。

设置：

```js
onSettingChange: (key, value) => {
  setSettings(prev => {
    if (key === 'fontSize') value = Math.min(18, Math.max(13, Number(value) || 15));
    if (key === 'contextBudget') value = Math.min(200000, Math.max(20000, Number(value) || 120000));
    if (key === 'roots' && !Array.isArray(value)) return prev;
    return { ...prev, [key]: value };
  });
}
```

记忆：

```js
onAddMemory: ({ kind, body, dueAt }) => {
  const text = body.trim();
  if (!text) return;
  setMemories(prev => [{ id: `memory-${Date.now()}`, kind, body: text, dueAt: dueAt || null }, ...prev]);
  showToast({ body: '已记住' });
}
onUpdateMemory: (id, patch) => setMemories(prev => prev.map(m => m.id === id ? { ...m, ...patch } : m))
onDeleteMemory: (id) => setMemories(prev => prev.filter(m => m.id !== id))
onClearMemories: () => { setMemories([]); showToast({ body: '已清空长期记忆' }); }
```

人设：

```js
onImportPersona: () => {
  // 创建 hidden <input type="file" accept=".txt,text/plain">
  // FileReader.readAsText；成功后 onSettingChange('persona', text)，toast '已导入人设'
  // 失败 toast '导入失败'
}
onExportPersona: () => {
  // settings.persona 或内置模板 -> Blob text/plain -> 下载 'persona.txt' -> toast '已导出人设'
}
```

技能 / MCP / Hooks：

```js
onToggleSkill: (name, enabled) => setSkills(prev => prev.map(s => s.name === name ? { ...s, enabled } : s))
onUseSkill: (name) => {
  setComposerValue(prev => prev ? `${prev} 使用「${name}」技能：` : `使用「${name}」技能：`);
  setPanel(null);
  showToast({ body: `使用「${name}」技能：` });
}
onSetTrust: (name, trust) => setMcpServers(prev => prev.map(s => s.name === name ? { ...s, trust } : s))
onMcpJsonChange: (text) => { setMcpJson(text); setMcpValidation({ status: 'idle', message: '' }); }
onMcpValidate: () => { /* JSON.parse；valid/invalid；更新 mcpValidation */ }
onMcpSave: () => { /* JSON.parse；valid 时 toast '配置已保存，重启应用后生效。当前会话仍按启动时的连接与工具运行。'；invalid 时设置 mcpValidation */ }
onToggleHookTrust: (id, trusted) => setHooks(prev => prev.map(h => h.id === id ? { ...h, trusted } : h))
onToggleWorkspaceHooks: (enabled) => { setWorkspaceHooksEnabled(enabled); showToast({ body: enabled ? '已启用工作区级 hooks' : '已停用工作区级 hooks' }); }
```

`onMcpValidate` / `onMcpSave` 的校验规则：

- `JSON.parse(jsonText)` 成功，且结果是对象、`mcpServers` 是对象 → `{ status: 'valid', message: '配置格式正确' }`。
- 否则 `{ status: 'invalid', message: 'JSON 解析失败：' + error.message }`。
- 保存只做 toast，不把 JSON 写回 `mcpServers`（设计明确“当前会话仍按启动时的连接运行”）。

图片 / 发送 / 权限 / 提问：

```js
onAddFiles: (files) => {
  const images = Array.from(files).filter(f => f.type.startsWith('image/'));
  const room = Math.max(0, 4 - pendingImages.length);
  const accepted = images.slice(0, room);
  const ignored = images.length - accepted.length;
  // FileReader.readAsDataURL 每个 accepted；成功后 setPendingImages(prev => [...prev, {...}])
  // 用函数式更新合并，避免连续选择时丢失已读出的图片
  // ignored > 0 时 toast `一次最多 4 张，已忽略 ${ignored} 张`
}
onRemoveImage: (id) => setPendingImages(prev => prev.filter(img => img.id !== id))
onSend: (text) => {
  const trimmed = text.trim();
  if (!trimmed || agent.isStreaming) return;
  agent.send(trimmed, activeId, pendingImages);
  setComposerValue('');
  setPendingImages([]);
}
onStop: () => agent.stop()
onRetry: () => agent.retryLast(activeId)
onPermission: (permissionId, decision) => agent.respondPermission(permissionId, decision)
onQuestionAnswer: (questionId, answer) => {
  setDismissedQuestionId(null);
  agent.answerQuestion(questionId, answer);
}
onReopenQuestion: () => setDismissedQuestionId(null)
onModelChange: (model) => {
  onSettingChange('model', model);
  showToast({ body: `已切换到 ${model}` });
}
```

App 的 JSX 结构（必须保持这个组合）：

```jsx
<Theme theme={runtimeTheme} mode="dark">
  <AppShell
    height="fill"
    contentPadding={0}
    variant="elevated"
    sideNav={<SessionSidebar ... />}
  >
    <VStack height="100%" gap={0} minHeight={0}>
      <WorkspaceBar ... />
      <ChatLayout
        density="spacious"
        composer={<Composer ... />}
      >
        <ChatPane ... />
      </ChatLayout>
      <StatusBar ... />
    </VStack>
  </AppShell>

  <SettingsPanel ... />
  <SkillsPanel ... />
  <McpPanel ... />
  <HooksPanel ... />
  <QuestionDialog
    question={pendingQuestion}
    isOpen={questionDialogOpen}
    onOpenChange={(open) => {
      if (!open) setDismissedQuestionId(pendingQuestion?.id ?? null);
    }}
    onAnswer={onQuestionAnswer}
  />
</Theme>
```

`QuestionDialog` 与四个面板一样作为 `Theme` 顶层子节点、`AppShell` 兄弟节点渲染；不要塞进消息流或主列。

四个面板作为 `Theme` 的子节点、`AppShell` 的兄弟节点渲染；不要把它们塞进主列的 `VStack`（Drawer 用顶层浮层，塞进主列会造成布局占位）。

### 7.4 `ChatPane`

文件：`src/components/ChatPane.jsx`
所有者：消息流实现者

```js
ChatPane({
  session,             // Session
  isStreaming,         // boolean
  settings,            // Settings
  usage,               // Usage（保留参数；本组件不渲染状态栏）
  contextBudget,       // number（保留参数；本组件不渲染状态栏）
  onPermission,        // (permissionId: string, decision: 'once'|'session'|'deny') => void
  onQuestionAnswer,    // (questionId: string, answer: string) => void
  onReopenQuestion,    // () => void（点「继续回答」重新打开弹窗）
  hidePendingQuestion, // boolean（弹窗打开时隐藏内联待答卡）
  onRetry,             // () => void
})
```

要求：

- **只返回 `ChatMessageList`**，不要包 `ChatLayout`、不要包 `VStack`、不要渲染 token 条。
- `ChatMessageList` 使用：`density="spacious"`、`gap={3}`、`isStreaming={isStreaming}`、`emptyState={<EmptyState title="开始一段新对话" description="有什么都可以聊，也能拖张图进来。" />}`。
- 日期分隔：在第一条消息前，以及 `createdAt` 的本地日期变化时，插入 `<ChatSystemMessage variant="divider"><Timestamp value={message.createdAt} format="date" /></ChatSystemMessage>`。
- 映射消息：
  ```jsx
  <MessageTurn
    key={message.id}
    turn={message}
    isLast={index === session.messages.length - 1}
    isStreaming={isStreaming && index === session.messages.length - 1}
    settings={settings}
    onPermission={onPermission}
    onQuestionAnswer={onQuestionAnswer}
    onReopenQuestion={onReopenQuestion}
    hidePendingQuestion={hidePendingQuestion}
    onRetry={onRetry}
  />
  ```
- 不做“展开更早的 N 条”（mock 数据均小于 40 条）；不要为此加额外控件。
- `usage` / `contextBudget` 只接收，不渲染；不要删除签名（App 会传）。

### 7.5 `MessageTurn`

文件：`src/components/MessageTurn.jsx`
所有者：消息流实现者

```js
MessageTurn({
  turn,               // Message
  isLast,             // boolean
  isStreaming,        // boolean
  settings,           // Settings
  onPermission,       // (permissionId, decision) => void
  onQuestionAnswer,   // (questionId, answer) => void
  onReopenQuestion,   // () => void
  hidePendingQuestion, // boolean（居中弹窗打开时为 true）
  onRetry,            // () => void
})
```

渲染规则：

1. 用户消息：
   ```jsx
   <ChatMessage sender="user">
     <ChatMessageBubble
       metadata={<ChatMessageMetadata timestamp={<Timestamp value={turn.createdAt} format="time" />} status="sent" />}
     >
       {content}
     </ChatMessageBubble>
   </ChatMessage>
   ```
2. 助手消息：
   ```jsx
   <ChatMessage sender="assistant" avatar={<Avatar name="LFTTDO" size="sm" />}>
     <ChatMessageBubble
       variant="ghost"
       name="LFTTDO"
       metadata={<ChatMessageMetadata timestamp={<Timestamp value={turn.createdAt} format="time" />} footer={<Text type="supporting" color="secondary">{turn.model ?? settings.model}</Text>} />}
     >
       {content}
     </ChatMessageBubble>
   </ChatMessage>
   ```
3. `content` 的构造：
   - 用一个 `VStack gap={2}` 包住所有块。
   - `text` 块 → `<Markdown isStreaming={isStreaming && isLastText}>{block.text}</Markdown>`。
   - `code` 块 → `<CodeBlock code={block.code} language={block.language} title={block.title} hasCopyButton size="sm" maxHeight="16rem" />`。
   - 连续的 `tool` / `permission` / `question` / `image` / `file` 块作为一段，交给 `<ToolCards blocks={run} onPermission={onPermission} onQuestionAnswer={onQuestionAnswer} onReopenQuestion={onReopenQuestion} disabled={Boolean(turn.stopped)} hidePendingQuestion={Boolean(hidePendingQuestion)} />`。
   - 待回答的提问在 `hidePendingQuestion` 为 true 时由 `QuestionDialog` 承载，`ToolCards` 跳过该内联卡；弹窗收起后显示「继续回答」紧凑提示；已回答的提问始终保留在消息流里。
   - 不要打乱原始 `blocks` 顺序；文本/代码与工具类块交替时按顺序分段。
4. `turn.stopped` 为 true 时，在气泡内容末尾显示 `<Text type="supporting" color="secondary">已停止</Text>`。
5. 重试：
   - 条件：`isLast && turn.role === 'assistant' && !isStreaming && !turn.stopped`。
   - 在气泡下方用 `HStack gap={1}` 渲染 `<Button label="重试这一轮" variant="ghost" size="sm" icon={<ArrowPathIcon />} onClick={onRetry} />`。
6. 复制：
   - 助手消息且有 `text` 块时，在同一 `HStack` 渲染 `<Button label="复制" variant="ghost" size="sm" icon={<DocumentDuplicateIcon />} onClick={copyAllText} />`。
   - `copyAllText` 把所有 `text` 块用 `\n\n` 拼起来，调用 `navigator.clipboard.writeText`；成功 `showToast({ body: '已复制' })`，失败 `showToast({ body: '复制失败', type: 'error' })`。
   - 用 `useToast`；不要用 `window.prompt`。
7. 不要在 `MessageTurn` 里渲染日期分隔；那是 `ChatPane` 的职责。
8. 不要实现“思考过程”折叠：本期 Block 联合类型没有 `thinking` 块。不要为了它改数据结构。

### 7.6 `ToolCards`

文件：`src/components/ToolCards.jsx`
所有者：消息流实现者

```js
ToolCards({
  blocks,             // Block[]（可能含任意类型；本组件忽略 text/code）
  onPermission,       // (permissionId, decision) => void
  onQuestionAnswer,   // (questionId, answer) => void
  onReopenQuestion,   // () => void（从紧凑提示重新打开弹窗）
  disabled,           // boolean（消息 stopped 后禁用权限/提问按钮）
  hidePendingQuestion, // boolean（居中弹窗打开时跳过待答内联卡）
})
```

内部渲染：

1. 按顺序遍历 `blocks`：
   - 连续的 `tool` 块 → 合并成一个 `<ChatToolCalls calls={...} />`。
   - `permission` → `<PermissionCard permission={block} onDecide={onPermission} />`。
   - `question` → 已回答时渲染 `<QuestionCard question={block} />`；待回答且 `hidePendingQuestion` 为 true 时跳过（弹窗承载）；待回答且弹窗已收起时渲染紧凑的 `PendingQuestionNote`（「模型提问 · 待回答」+「继续回答」），不重复渲染表单。
   - `image` → `<ImageCard image={block} />`。
   - `file` → `<FileCard file={block} />`。
   - `text` / `code` → 跳过（由 `MessageTurn` 渲染）。
2. `ChatToolCalls` 映射：
   ```jsx
   calls={run.map((tool, index) => ({
     key: `${turnKey}-tool-${index}`,
     name: tool.name,
     status: tool.status,
     target: tool.target,
     duration: tool.duration,
     additions: tool.additions,
     deletions: tool.deletions,
     errorMessage: tool.errorMessage || undefined,
     resultDetail: tool.result
       ? <CodeBlock code={tool.result} language="text" size="sm" hasCopyButton maxHeight="16rem" />
       : undefined,
   }))}
   ```
   `turnKey` 可用 `blocks[0]?.target` + index，或用 `useId()`；不要依赖不存在的数据字段。
3. 权限卡：
   - `<Card elevation="low" padding={3}>`，内部 `VStack gap={2}`。
   - 标题行：`<Text type="label">需要你的许可</Text>` + 风险 `Token`：
     - `read` → `color="gray"`，label `只读`
     - `write` → `color="orange"`，label `写入`
     - `exec` → `color="red"`，label `执行`
   - 工具名：`<Text type="code">{permission.tool}</Text>`。
   - 参数：`<CodeBlock code={JSON.stringify(permission.args, null, 2)} language="json" size="sm" hasCopyButton maxHeight="12rem" />`。
   - `decision === null` 时：
     ```jsx
     <HStack gap={2} wrap="wrap">
       <Button label="仅本次" variant="primary" size="sm" onClick={() => onDecide(permission.id, 'once')} />
       <Button label="本会话内都允许" variant="secondary" size="sm" onClick={() => onDecide(permission.id, 'session')} />
       <Button label="拒绝" variant="destructive" size="sm" onClick={() => onDecide(permission.id, 'deny')} />
     </HStack>
     ```
   - `decision !== null` 时：
     - `deny` → `<Token label="已拒绝" color="red" />` + `<Text type="supporting" color="secondary">已拒绝，不会执行这一步。</Text>`
     - `once` → `<Token label="已放行" color="green" />` + `<Text type="supporting" color="secondary">已放行（仅本次）。</Text>`
     - `session` → `<Token label="已放行" color="green" />` + `<Text type="supporting" color="secondary">已放行（本会话内）。</Text>`
   - 不要在这里直接改数据；只调用 `onDecide`。
4. 提问卡：
   - `<Card elevation="low" padding={3}>`，内部 `VStack gap={2}`。
   - `<Text type="label">模型提问</Text>` + `<Text>{question.question}</Text>`。
   - `answer === null`：
     - `RadioList label="选项" isLabelHidden value={selected} onChange={setSelected}`，每个选项一个 `<RadioListItem label={option} value={option} />`。
     - `<TextInput label="自定义回答" placeholder="也可以直接输入回答" value={custom} onChange={setCustom} />`。
     - `<Button label="提交" variant="primary" size="sm" isDisabled={!selected && !custom.trim() || disabled} onClick={() => onAnswer(question.id, (custom.trim() || selected))} />`。
     - 选择 radio 时清空 `custom`；输入 custom 时可保留选择，但提交时 custom 优先。
   - `answer !== null`：`<Token label="已回答" color="green" />` + `<Text type="supporting" color="secondary">{question.answer}</Text>`，不再显示控件。
   - 本地 state 用 `key={question.id}` 或 `useEffect` 重置；同一卡片重渲染不能丢选择。
   - 待回答的提问不在这里渲染表单：弹窗打开时跳过，弹窗收起后由紧凑的 `PendingQuestionNote` 提供「继续回答」入口。
5. 图片卡：
   ```jsx
   <VStack gap={1} maxWidth="20rem">
     <AspectRatio ratio={16 / 10}>
       <img src={image.src} alt={image.alt} style={{ width: '100%', height: '100%', objectFit: 'cover', borderRadius: 'var(--radius-element)' }} />
     </AspectRatio>
     <Text type="supporting" color="secondary">工具返回的图片</Text>
   </VStack>
   ```
   最多 4 张时可以用 `Grid columns={2} gap={2}`；单张就保持 `maxWidth="20rem"`。
6. 文件卡：
   - `<Card elevation="none" padding={2}>`，内部 `HStack gap={2} align="center"`。
   - `DocumentTextIcon` + `VStack gap={0}`：
     - `<Text type="label">{file.name}</Text>`
     - `<Token label={file.path} size="sm" icon={<FolderIcon />} onClick={() => showToast({ body: '已在资源管理器中显示' })} />`
   - 也可以加一个 `Button label="打开位置" variant="ghost" size="sm"`，同样只 toast。
7. `useToast` 在 `ToolCards` 顶层调用；`showToast` 传 `{ body: '…' }`。
8. 消息被停止（`disabled`）后，权限按钮、提问「提交」和「继续回答」都必须禁用；停止的消息不能再驱动脚本。

### 7.6.1 `QuestionDialog`

文件：`src/components/QuestionDialog.jsx`
所有者：消息流实现者

```js
QuestionDialog({
  question,            // QuestionBlock | null
  isOpen,              // boolean
  onOpenChange,        // (open: boolean) => void
  onAnswer,            // (questionId: string, answer: string) => void
})
```

要求：

- 用 `Dialog isOpen={isOpen} onOpenChange={onOpenChange} width={520} purpose="form"` 做居中弹窗；不要用 Drawer，也不要塞进消息流。
- 内容用 `Layout`：`DialogHeader title="模型提问" subtitle="回答后继续当前任务"`、`LayoutContent`、带分隔线的 `LayoutFooter`。
- `LayoutContent` 里依次是问题正文、`RadioList`（有选项时）和 `TextInput label="自定义回答" placeholder="也可以直接输入回答"`。
- 页脚右侧放 `Button label="稍后回答" variant="ghost"` 与 `Button label="提交" variant="primary"`；回答为空时禁用提交。
- 提交值取 `custom.trim() || selected`；提交后由 App 调 `agent.answerQuestion`，弹窗随 `pendingQuestion` 消失。
- 「稍后回答」关闭按钮、Esc、点遮罩都走 `onOpenChange(false)`；App 记录 `dismissedQuestionId` 后，消息流显示「模型提问 · 待回答」紧凑提示，点「继续回答」重新打开弹窗。
- 用 `question.id` 变化重置本地选项和自定义输入；不要跨问题复用选择。
- 已回答的问题不在弹窗里回放，只在消息流里保留「已回答 + 回答内容」记录。

### 7.7 `Composer`

文件：`src/components/Composer.jsx`
所有者：输入与模拟实现者

```js
Composer({
  value,              // string
  onChange,           // (value: string) => void
  onSend,             // (text: string) => void
  onStop,             // () => void
  isStreaming,        // boolean
  pendingImages,      // PendingImage[]
  onAddFiles,         // (files: File[]) => void
  onRemoveImage,      // (id: string) => void
  skills,             // Skill[]
  files,              // string[]
  models,             // string[]（可切换的模型列表）
  modelName,          // string（当前模型）
  onModelChange,      // (model: string) => void
  usage,              // Usage
  contextBudget,      // number
})
```

实现要求：

1. 主输入用 `ChatComposer`：
   ```jsx
   <ChatComposer
     value={value}
     onChange={onChange}
     onSubmit={(text) => onSend(text.trim())}
     onStop={onStop}
     isStopShown={isStreaming}
     elevation="low"
     placeholder="说点什么…（Enter 发送，Shift+Enter 换行）"
     input={<ChatComposerInput ... />}
     drawer={...}
     headerActions={...}
     headerContext={...}
     footerActions={...}
   />
   ```
   `ChatComposer` 的 `onSubmit` 已经处理 Enter/Shift+Enter/IME；不要自己监听 Enter。
2. `ChatComposerInput`：
   ```jsx
   <ChatComposerInput
     triggers={[skillTrigger, fileTrigger]}
     onFiles={onAddFiles}
     hasHistory
     maxRows={6}
     label="消息输入"
     placeholder="说点什么…（Enter 发送，Shift+Enter 换行）"
     enterKeyHint="send"
   />
   ```
   不要在 `ChatComposerInput` 上再传 `value`/`onChange`；它从 `ChatComposer` 的 context 拿受控值。
3. `/` 技能 trigger：
   ```js
   const skillItems = skills
     .filter(s => s.enabled)
     .map(s => ({
       id: s.name,
       label: `/${s.name}`,
       auxiliaryData: { description: s.description, argumentHint: s.argumentHint },
     }));
   const skillSource = createStaticSource(skillItems);
   const skillTrigger = {
     character: '/',
     menuLabel: '技能快捷菜单',
     emptySearchText: '没有匹配的技能',
     searchSource: skillSource,
     renderItem: (item) => (
       <TypeaheadItem
         item={item}
         description={`${item.auxiliaryData.description} ${item.auxiliaryData.argumentHint}`}
       />
     ),
     onSelect: (item) => `使用「${item.id}」技能：`,
   };
   ```
   停用的技能不进入 `skillItems`。
4. `@` 文件 trigger：
   ```js
   const fileItems = files.slice(0, 20).map(path => ({ id: path, label: path }));
   const fileSource = createStaticSource(fileItems);
   const fileTrigger = {
     character: '@',
     menuLabel: '文件提及菜单',
     emptySearchText: '没有匹配的文件',
     searchSource: fileSource,
     renderItem: (item) => <TypeaheadItem item={item} />,
     onSelect: (item) => `@${item.id}`,
   };
   ```
   15 条 mock 路径全部可用；最多 20 条。
5. 附件抽屉：
   - `pendingImages.length > 0` 时传 `drawer`：
     ```jsx
     <ChatComposerDrawer count={pendingImages.length} label="图片">
       <HStack gap={2} wrap="wrap">
         {pendingImages.map(img => (
           <VStack key={img.id} gap={1} width="4.5rem">
             <img src={img.url} alt={img.name} style={{ width: '100%', height: '4.5rem', objectFit: 'cover', borderRadius: 'var(--radius-element)' }} />
             <IconButton label={`移除 ${img.name}`} icon={<XMarkIcon />} size="sm" variant="ghost" onClick={() => onRemoveImage(img.id)} />
           </VStack>
         ))}
       </HStack>
     </ChatComposerDrawer>
     ```
   - `pendingImages.length === 0` 时不传 `drawer`（传 `undefined`）。
6. `headerActions`：
   - `<Button label="添加图片" variant="ghost" size="sm" icon={<PhotoIcon />} onClick={() => fileInputRef.current?.click()} />`
   - 一个 hidden 的 `<input ref={fileInputRef} type="file" accept="image/*" multiple hidden onChange={handleFileInput} />`。
   - `handleFileInput` 调 `onAddFiles(Array.from(e.target.files ?? []))`，然后 `e.target.value = ''`。
7. `headerContext`：
   ```jsx
   <HStack gap={2} align="center">
     <Text type="supporting" color="secondary">上下文</Text>
     <VStack width="7.5rem">
       <ProgressBar
         label="上下文占用"
         isLabelHidden
         value={usage.contextTokens}
         max={contextBudget}
         variant={usage.contextTokens / contextBudget > 0.85 ? 'error' : 'accent'}
       />
     </VStack>
     <Text type="supporting" color="secondary">{Math.round((usage.contextTokens / Math.max(1, contextBudget)) * 100)}%</Text>
   </HStack>
   ```
8. `footerActions`：
   - 用 `DropdownMenu` 做模型下拉切换：`button={{ label: modelName, variant: 'ghost', size: 'sm', icon: <CpuChipIcon /> }}`，`items` 来自 `models`，当前项 `endContent: '当前'`，点击调用 `onModelChange(model)`。
   - 流式进行中 `isDisabled`，避免中途换模型。
   - 设置面板的「模型名」输入框与这里共用 `settings.model`，两边保持同步。
9. 空输入保护：`onSend` 前 `value.trim()` 为空则不调用；`ChatComposer` 的发送按钮也应处于禁用/无动作状态。
10. 不要自己实现毛玻璃底栏；`ChatLayout` 已经提供 frosted glass dock。

### 7.8 `StatusBar`

文件：`src/components/StatusBar.jsx`
所有者：输入与模拟实现者

```js
StatusBar({
  usage,           // Usage
  contextBudget,   // number
})
```

要求：

- 返回 `VStack gap={1}`（需要放 85% 警告时）或 `HStack`。
- 第一行 `HStack`：`justify="between"`、`align="center"`、`paddingInline={4}`、`paddingBlockStart={1}`、`paddingBlockEnd={2}`、`width="100%"`。
- 左侧：`<Text type="supporting" color="secondary">本轮 token：输入 {usage.inputTokens} · 输出 {usage.outputTokens}</Text>`。
- 右侧：`<HStack gap={2} align="center">` + `<Text type="supporting" color="secondary">{pct}%</Text>` + `<VStack width="7.5rem"><ProgressBar label="上下文占用" isLabelHidden value={usage.contextTokens} max={contextBudget} variant={pct > 85 ? 'error' : 'accent'} /></VStack>`。
- `pct = Math.round((usage.contextTokens / Math.max(1, contextBudget)) * 100)`；超过 100 显示 100（clamp）。
- `pct > 85` 时，在下一行显示 `<Text type="supporting" color="error">上下文较满：下次发送将自动压缩较早的对话</Text>`。
- 不要用 raw px 给占用条定宽；用 `VStack width="7.5rem"`。
- 可选：给 token 文本加 `aria-live="polite"`；如果 Astryx 组件不透传，则跳过。

### 7.9 `SettingsPanel`

文件：`src/components/panels/SettingsPanel.jsx`
所有者：面板实现者

```js
SettingsPanel({
  isOpen,             // boolean
  onClose,            // () => void
  settings,           // Settings
  onSettingChange,    // (key: string, value: any) => void
  memories,           // Memory[]
  onAddMemory,        // ({ kind, body, dueAt }) => void
  onUpdateMemory,     // (id: string, patch: Partial<Memory>) => void
  onDeleteMemory,     // (id: string) => void
  onClearMemories,    // () => void
  onImportPersona,    // () => void
  onExportPersona,    // () => void
})
```

外壳：

```jsx
<Drawer
  isOpen={isOpen}
  onOpenChange={(open) => { if (!open) onClose(); }}
  label="设置"
  side="end"
  width="min(45rem, 92vw)"
  padding={4}
  purpose="info"
>
  <VStack gap={4} height="100%">
    <HStack justify="between" align="center">
      <VStack gap={0}>
        <Heading level={2}>设置</Heading>
        <Text type="supporting" color="secondary">模型、界面、权限与长期记忆</Text>
      </VStack>
      <IconButton label="关闭设置" icon={<XMarkIcon />} variant="ghost" onClick={onClose} />
    </HStack>
    <HStack align="start" gap={4}>
      <VStack gap={1} width="10rem">{/* 左侧分区导航 */}</VStack>
      <VStack gap={3} width="100%">{/* 右侧内容 */}</VStack>
    </HStack>
  </VStack>
</Drawer>
```

左侧分区导航：`模型`、`界面`、`权限`、`文件访问`、`人设`、`长期记忆`、`储存`。用 `Button`：
```jsx
<Button
  label={label}
  variant={active ? 'secondary' : 'ghost'}
  size="sm"
  width="100%"
  onClick={() => setActiveSection(id)}
/>
```

右侧分区内容：

1. **模型**
   - `TextInput label="接口地址（base_url）" value={settings.endpoint} onChange={(v) => onSettingChange('endpoint', v)}`
   - `TextInput label="模型名" value={settings.model} onChange={(v) => onSettingChange('model', v)}`
   - `TextInput label="API Key" type="password" value={apiKey} onChange={setApiKey} placeholder={settings.hasApiKey ? '已配置，留空不修改' : '未配置'}`
   - `HStack gap={2}`：
     - `Button label="保存" variant="primary" size="sm" onClick={handleSaveKey}` → `onSettingChange('hasApiKey', true)`、清空 `apiKey`、toast `已保存 API Key（演示）`。
     - `Button label="清除已存的 key" variant="destructive" size="sm"` → 行内二次确认；确认后 `onSettingChange('hasApiKey', false)`、toast `已清除 API Key`。
   - `Text type="supporting" color="secondary">当前：{settings.hasApiKey ? '已配置' : '未配置'}</Text>`
   - `Slider label="上下文预算" min={20000} max={200000} step={10000} value={settings.contextBudget} onChange={(v) => onSettingChange('contextBudget', v)} formatValue={(v) => Math.round(v / 1000) + 'k'} valueDisplay="text"`
2. **界面**
   - `Text type="supporting" color="secondary">当前主题：lfttdo-noir（深色固定）</Text>`，用 `Token label="lfttdo-noir" color="purple"`。
   - `Slider label="字号" min={13} max={18} step={1} value={settings.fontSize} onChange={(v) => onSettingChange('fontSize', v)} valueDisplay="text"`
   - `Selector label="强调色" value={settings.accent} onChange={(v) => onSettingChange('accent', v)} options={[{value:'#7c8cff',label:'靛蓝'},{value:'#22d3ee',label:'青色'},{value:'#f472b6',label:'粉色'},{value:'#34d399',label:'绿色'}]}`
   - 说明文字：`字号和强调色即时生效；刷新页面后回到初始值。`
3. **权限**
   - `RadioList label="权限模式" value={settings.permissionMode} onChange={(v) => onSettingChange('permissionMode', v)}`：
     - `RadioListItem value="rules" label="规则（默认）" description="读取自动放行；写入和命令逐条询问。"`
     - `RadioListItem value="ask" label="全部询问" description="写入和命令每次都弹权限卡。"`
     - `RadioListItem value="trusted" label="信任模式" description="自动放行，风险自担。"`
   - `Switch label="Plan 模式" description="开启后写入和命令会被直接拒绝，直到退出。" value={settings.planMode} onChange={(v) => onSettingChange('planMode', v)}`
4. **文件访问**
   - `List density="compact"` + `ListItem` 每个 root：`label={root}`、`startContent={<FolderIcon />}`、`endContent` 是 `IconButton`，其 `label` 为 `移除 ${root}`，`onClick` 调 `onSettingChange('roots', settings.roots.filter(r => r !== root))`。
   - 添加行：`TextInput label="新增可访问目录" value={newRoot} onChange={setNewRoot}` + `Button label="添加" size="sm" variant="secondary" isDisabled={!newRoot.trim()} onClick={...}` → `onSettingChange('roots', [...settings.roots, newRoot.trim()])`。
   - 说明：`仅演示，不访问真实文件系统。`
5. **人设**
   - `TextArea label="人设（系统提示词）" rows={6} value={settings.persona} onChange={(v) => onSettingChange('persona', v)}`
   - `HStack gap={2}`：`Button label="从 .txt 导入" variant="secondary" size="sm" onClick={onImportPersona}`、`Button label="导出 .txt" variant="secondary" size="sm" onClick={onExportPersona}`、`Button label="恢复内置" variant="ghost" size="sm" onClick={() => onSettingChange('persona', '')}`。
6. **长期记忆**
   - `memories.length === 0` 时：`Text type="supporting" color="secondary">还没有长期记忆</Text>`。
   - 否则 `List density="compact" hasDividers` + `ListItem`：
     - `label={memory.body}`
     - `description` 由 `kindLabel(memory.kind)` 和 `memory.dueAt` 拼接：有到期日时追加 ` · 到期 YYYY-MM-DD`。
     - `startContent={<Token label={kindLabel(memory.kind)} color={kindColor(memory.kind)} size="sm" />}`
     - `endContent={<HStack gap={1}><IconButton label="编辑记忆" icon={<PencilIcon />} size="sm" variant="ghost" onClick={() => setEditingId(memory.id)} /><IconButton label="删除记忆" icon={<TrashIcon />} size="sm" variant="ghost" onClick={() => onDeleteMemory(memory.id)} /></HStack>}`
   - 编辑态：用 `TextArea` 绑定 `body`，`Selector` 绑定 kind，`TextInput` 绑定 dueAt；保存调 `onUpdateMemory(id, { kind, body, dueAt })`。
   - 新增表单：`Selector label="类型" options={[{value:'fact',label:'事实'},{value:'preference',label:'偏好'},{value:'todo',label:'待办'}]}`、`TextInput label="内容"`、`TextInput label="到期日（可选）" placeholder="YYYY-MM-DD"`、`Button label="新增" variant="secondary" size="sm"`。
   - `Button label="清空全部记忆" variant="destructive" size="sm"` → 行内二次确认；确认后 `onClearMemories()`。
7. **储存**
   - `Text type="supporting" color="secondary">演示数据只保存在浏览器内存，刷新后回到预置状态。</Text>`
   - `HStack gap={2}`：`Button label="打开数据目录" variant="secondary" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开数据目录' })}`、`Button label="打开工作区目录" variant="ghost" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开工作区目录' })}`。

标签映射：

```js
const KIND_LABEL = { fact: '事实', preference: '偏好', todo: '待办' };
const KIND_COLOR = { fact: 'blue', preference: 'green', todo: 'orange' };
```

`useToast` 用于保存 Key、清除 Key、导入/导出人设、打开目录等局部反馈。不要直接改 `settings` / `memories`。

### 7.10 `SkillsPanel`

文件：`src/components/panels/SkillsPanel.jsx`
所有者：面板实现者

```js
SkillsPanel({
  isOpen,          // boolean
  onClose,         // () => void
  skills,          // Skill[]
  onToggleSkill,   // (skillName: string, enabled: boolean) => void
  onUseSkill,      // (skillName: string) => void
})
```

外壳：`Drawer`，`label="技能"`、`width="min(28.75rem, 92vw)"`、`padding={4}`；标题 `技能`，副标题 `启用后会出现在输入框的 / 菜单`。

内容：

- `List density="compact" hasDividers` + `ListItem`：
  - `label={<Text type="code">/{skill.name}</Text>}`
  - `description` 为 `skill.description` + ` · ` + `skill.argumentHint`
  - `endContent={<HStack gap={2} align="center"><Switch label="启用" isLabelHidden value={skill.enabled} onChange={(v) => onToggleSkill(skill.name, v)} /><Button label="用这个技能" variant="secondary" size="sm" onClick={() => onUseSkill(skill.name)} /></HStack>}`
- 底部 `HStack gap={2}`：
  - `Button label="刷新" variant="secondary" size="sm" onClick={() => showToast({ body: '已刷新技能目录' })}`
  - `Button label="打开技能文件夹" variant="ghost" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开技能文件夹' })}`
  - `Button label="关闭" variant="ghost" size="sm" onClick={onClose}`
- `useToast` 只用于这两个 mock 操作；技能启用/停用通过回调。

### 7.11 `McpPanel`

文件：`src/components/panels/McpPanel.jsx`
所有者：面板实现者

```js
McpPanel({
  isOpen,          // boolean
  onClose,         // () => void
  servers,         // McpServer[]
  onSetTrust,      // (name: string, trust: 'allow'|'ask'|'block') => void
  jsonText,        // string
  onJsonChange,    // (text: string) => void
  onValidate,      // () => void
  onSave,          // () => void
  validation,      // McpValidation
})
```

外壳：`Drawer`，`label="MCP"`、`width="min(45rem, 92vw)"`、`padding={4}`；标题 `MCP 服务器`，副标题 `连接状态、信任档与 mcp.json`。

内容：

1. 安全提示：`<Banner status="warning" title="安全提示" description="配置文件可能包含 env/headers，不要在共享屏幕时打开。" />`
2. 服务器列表：`servers.length === 0` 时 `Text>还没有配置服务器。点击「编辑配置」直接写 mcp.json。</Text>`；否则 `List density="compact" hasDividers` + `ListItem`：
   - `label={server.name}`
   - `description={server.transport}`
   - `startContent`：状态 `Token`
     - `connected` → `{ label: '已连接', color: 'green' }`
     - `connecting` → `{ label: '连接中…', color: 'yellow' }`
     - `disconnected` → `{ label: '未连接', color: 'gray' }`
   - `endContent`：`<Selector label="信任" isLabelHidden value={server.trust} onChange={(v) => onSetTrust(server.name, v)} options={[{value:'allow',label:'本服务器全部允许'},{value:'ask',label:'每次询问'},{value:'block',label:'禁止'}]} size="sm" />`
   - transport 也可用 `Token`：`stdio` → `color="blue"`，`streamable-http` → `color="purple"`。
3. JSON 编辑器：
   - `<TextArea label="mcp.json" rows={14} value={jsonText} onChange={onJsonChange} />`
   - `HStack gap={2}`：`Button label="校验" variant="secondary" size="sm" onClick={onValidate}`、`Button label="保存并重启" variant="primary" size="sm" onClick={onSave}`。
   - `validation.status === 'valid'` → `<Banner status="success" title="配置格式正确" description={validation.message} />`
   - `validation.status === 'invalid'` → `<Banner status="error" title="配置格式有误" description={validation.message} />`
   - `idle` → 不显示。
4. 底部 `Button label="关闭" variant="ghost" size="sm" onClick={onClose}`。
5. 不要真的重连/断开 MCP；状态只来自 mock 数据。`onSave` 只 toast。

### 7.12 `HooksPanel`

文件：`src/components/panels/HooksPanel.jsx`
所有者：面板实现者

```js
HooksPanel({
  isOpen,                     // boolean
  onClose,                    // () => void
  hooks,                      // Hook[]
  onToggleTrust,              // (hookId: string, trusted: boolean) => void
  workspaceHooksEnabled,      // boolean
  onToggleWorkspaceHooks,     // (enabled: boolean) => void
})
```

外壳：`Drawer`，`label="Hooks"`、`width="min(28.75rem, 92vw)"`、`padding={4}`；标题 `Hooks`，副标题 `事件、命令与信任状态`。

内容：

1. `Switch label="启用工作区级 hooks"`，`description` 显示 `当前：已启用` 或 `当前：未启用`；`value={workspaceHooksEnabled}`、`onChange={handleWorkspaceToggle}`。
   - `handleWorkspaceToggle(next)`：如果 `next === false`，直接 `onToggleWorkspaceHooks(false)`。
   - 如果 `next === true` 且当前未启用，先在开关下方显示行内确认：`<Banner status="warning" title="确认启用工作区级 hooks？" description="工作区 hooks 会执行仓库里的命令，请确认来源可信。" endContent={<HStack gap={2}><Button label="确认启用" variant="destructive" size="sm" onClick={() => { onToggleWorkspaceHooks(true); setConfirming(false); }} /><Button label="取消" variant="ghost" size="sm" onClick={() => setConfirming(false)} /></HStack>} />`。确认后才调用回调。
2. Hook 列表：`List density="compact" hasDividers` + `ListItem`：
   - `label={<Text type="code">{hook.event}</Text>}`
   - `description={<Text type="code">{hook.command}</Text>}`
   - `startContent={<Token label={hook.source === 'user' ? '用户作用域' : '工作区级'} color={hook.source === 'user' ? 'blue' : 'purple'} size="sm" />}`
   - `endContent`：
     - `hook.trusted` 时：`<HStack gap={1}><Token label="信任" color="green" size="sm" /><Button label="撤销信任" variant="ghost" size="sm" onClick={() => setConfirmingHookId(hook.id)} /></HStack>`
     - 未信任时：`<Button label="信任" variant="secondary" size="sm" onClick={() => onToggleTrust(hook.id, true)} />`
   - 撤销信任的二次确认：当 `confirmingHookId === hook.id` 时，显示 `HStack` + `Text>撤销信任？</Text>` + `Button label="确认撤销" variant="destructive" size="sm" onClick={() => { onToggleTrust(hook.id, false); setConfirmingHookId(null); }}` + `Button label="取消"`。
3. 底部 `HStack gap={2} wrap="wrap"`：
   - `Button label="打开 hooks.json" variant="secondary" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开 hooks.json' })}`
   - `Button label="打开数据目录" variant="ghost" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开数据目录' })}`
   - `Button label="打开工作区目录" variant="ghost" size="sm" onClick={() => showToast({ body: '已在资源管理器中打开工作区目录' })}`
   - `Button label="关闭" variant="ghost" size="sm" onClick={onClose}`
4. 不执行命令、不写文件、不读 SHA-256；`trusted` 只是 mock 状态。

### 7.13 `useMockAgent`

文件：`src/hooks/useMockAgent.js`
所有者：输入与模拟实现者

```js
useMockAgent({
  sessions,        // Session[]（当前值；用于发送/重试前同步确认会话存在）
  settings,        // Settings（当前值；内部用 ref 读最新）
  setSessions,     // React state setter: (updater) => void
  setUsage,        // React state setter: (Usage | ((prev: Usage) => Usage)) => void
})
```

返回：

```js
{
  send(text, sessionId, images = []),  // (string, string, PendingImage[]) => void
  stop(),                              // () => void
  isStreaming,                         // boolean（全局是否有一轮在跑）
  streamingSessionId,                  // string | null（正在流式的那条会话）
  respondPermission(permissionId, decision), // (string, 'once'|'session'|'deny') => void
  answerQuestion(questionId, answer),  // (string, string) => void
  retryLast(sessionId),                // (string) => void
}
```

行为要求（必须按此实现，四个实现者按这个联调）：

#### `send(text, sessionId, images)`

1. 如果 `text.trim()` 为空、`isStreamingRef.current` 为 true、或找不到 `sessionId` 对应的 session，直接返回。
2. 生成用户消息：
   ```js
   {
     id: `message-${Date.now()}-u`,
     role: 'user',
     createdAt: new Date().toISOString(),
     blocks: [
       { type: 'text', text: text.trim() },
       ...images.map((img, i) => ({ type: 'image', src: img.url, alt: img.name })),
     ],
   }
   ```
3. 如果该 session 是空会话且标题是 `新对话` 或空，把标题设为 `text.trim().replace(/\s+/g, ' ').slice(0, 16)`。
4. 追加助手占位消息：
   ```js
   {
     id: `message-${Date.now()}-a`,
     role: 'assistant',
     createdAt: new Date().toISOString(),
     isStreaming: true,
     blocks: [],
   }
   ```
5. `setUsage` 至少更新 `inputTokens`：`estimateTokens(text) + 200`（system 估算），`contextTokens` 在结束时累计。
6. 250ms 后按脚本开始流式；`isStreaming` 状态设为 true。

#### 脚本选择

按优先级匹配用户文本：

| 优先级 | 条件 | 脚本 |
|---|---|---|
| 1 | 文本匹配 `/发布|截图|说明|草稿|提问/` | `question` |
| 2 | 文本匹配 `/运行|命令|构建|build|启动|dev/` | `command` |
| 3 | 文本匹配 `/权限|写入|修改|修复|改一下|编辑/` | `permission` |
| 4 | 文本匹配 `/读|看一下|检查|文件|导出/` | `read` |
| 5 | 其他 | `default` |

各脚本：

- **read**
  - 文本："我只读这个文件，先确认它的导出面。"
  - 工具：`read_file`，target `src/data/mockData.js`，`running` → 800–1200ms 后 `complete`，`duration` 为实际等待，`additions:0`、`deletions:0`，`result` 为导出列表字符串。
  - 文本："导出符合契约，字段都在。"
  - `file` 块：`{ type:'file', path:'src/data/mockData.js', name:'mockData.js' }`。
- **command**
  - 文本："先确认开发服务器能起来，再跑一次生产构建。"
  - 风险 `exec`。如果 `settings.planMode` → 直接追加 `run_command` 工具块，`status:'error'`，`errorMessage:'请先退出 Plan 模式'`，结束。
  - 如果 `settings.permissionMode === 'trusted'` → 直接执行。
  - 如果 `settings.permissionMode === 'rules'` 且该 session 已记录 `run_command` 的 session 允许 → 直接执行。
  - 否则追加 `permission` 块：`{ type:'permission', id:'permission-'+Date.now(), tool:'run_command', args:{ command:'npm run dev -- --host 127.0.0.1' }, risk:'exec', decision:null }`，暂停并等待 `respondPermission`。
  - 放行后：`run_command` `running` → 1000–1500ms 后 `complete`，`result` 为 Vite 启动输出；结束时文本："开发服务器已就绪。"
  - 拒绝后：文本："已拒绝，我不会执行这个命令。"
- **permission**
  - 文本："这一步需要写入文件，我先请求你的许可。"
  - 风险 `write`。`planMode` → 工具块 `write_file` `status:'error'`、`errorMessage:'请先退出 Plan 模式'`，结束。
  - `trusted` → 直接执行 `write_file` 成功。
  - 否则追加 `permission` 块：`{ type:'permission', id:'permission-'+Date.now(), tool:'write_file', args:{ path:'src/App.jsx', patch:'- </div>\n+ </VStack>' }, risk:'write', decision:null }`，暂停。
  - 放行后：`write_file` `running` → 800–1200ms 后 `complete`，`additions:1`、`deletions:1`、`result:'已写入 src/App.jsx'`；文本："已写入，继续下一步。"
  - 拒绝后：文本："已拒绝，我不会修改文件。"
- **question**
  - 文本："我整理了一版发布说明草稿。"
  - `code` 块：`{ type:'code', language:'markdown', title:'docs/release-notes.md', code:'# v0.2.0\n\n## 新增\n- …\n\n## 改进\n- …\n\n## 修复\n- …' }`。
  - `file` 块：`{ type:'file', path:'docs/release-notes.md', name:'release-notes.md' }`。
  - `question` 块：`{ type:'question', id:'question-'+Date.now(), question:'这份发布说明先发给谁？', options:['团队内部评审','公开渠道','先存草稿'], answer:null }`，暂停。
- **default**
  - 文本流式输出："我先把这件事拆成三步：确认现状、做一个最小改动、再验证结果。你可以继续补充目标或约束。"
  - 不产生工具卡。

文本流式：每 20–40ms 追加 2–4 个字符；用 `setSessions` 的不可变更新。工具/权限/提问之间的文本可以一次推送完，不必逐字。

#### 权限判定

```js
function needsPermission(risk, settings, sessionId, tool) {
  if (risk === 'read') return false;
  if (settings.planMode) return 'deny-plan';
  if (settings.permissionMode === 'trusted') return false;
  if (settings.permissionMode === 'rules' && sessionAllowedRef.current.get(sessionId)?.has(tool)) return false;
  return true;
}
```

- `respondPermission(permissionId, decision)`：
  - 在所有 session 中找 `type:'permission' && id === permissionId`。
  - 如果找不到或已解决，no-op。
  - 把 `decision` 写回块：`once` / `session` / `deny`。
  - 如果 `decision === 'session'`，把该 permission 的 `tool` 加入 `sessionAllowedRef.current[sessionId]` 的 Set。
  - 如果该 permission 属于当前正在等待的运行（`pendingRunRef.current?.permissionId === permissionId`），调用 `pendingRunRef.current.resume(decision)`；不要额外追加消息。
  - 如果是静态 mock 权限（`pendingRunRef` 没有它）：
    - `deny`：追加一条助手消息，`text: '已拒绝，不会执行这一步。'`。
    - `once`/`session`：追加一条助手消息，包含工具块 `{ type:'tool', name: permission.tool, target: String(permission.args.path ?? permission.args.command ?? '当前会话'), status:'complete', duration: 800, additions: 1, deletions: 1, result:'已执行：' + permission.tool, errorMessage:'' }`。
  - 更新该 session 的 `updatedAt`。
- **静态权限卡必须可点**：`session-3` 的 `permission-3-1` 就是静态数据；`respondPermission` 必须能更新它并追加上述消息，不能只处理运行中的权限。

#### 提问判定

- `answerQuestion(questionId, answer)`：
  - 在所有 session 中找 `type:'question' && id === questionId`。
  - 如果找不到或 `answer !== null`，no-op。
  - 写回 `answer`。
  - 追加一条用户消息（`blocks:[{type:'text', text: answer}]`）和一条助手消息（`text:'收到，我按这个方向继续。'`）。
  - 如果该 question 属于当前运行，则恢复脚本；否则到此结束。
  - 更新 `updatedAt`。

#### `retryLast(sessionId)`

- 如果 `isStreamingRef.current` 为 true，no-op（重试按钮在流式中不显示）。
- 找到该 session 最后一条 `role === 'assistant'` 的消息，截断该消息及其之后的所有消息。
- 找到截断后最后一条 `role === 'user'` 的消息，取出其 `text` 块和 `image` 块。
- 用同样的文本/图片重新生成：先追加一个助手占位消息，250ms 后按脚本流式；不要再次追加用户消息。
- 如果找不到用户消息，no-op。

#### `stop()`

- 递增 `runIdRef`（让所有在途定时器失效），清空 `pendingRunRef`。
- 找到当前流式的助手消息，设置 `isStreaming: false`、`stopped: true`。
- `isStreaming` state 设为 false。
- 已经推送的文本/工具卡保留；不删除。
- 如果当前在等权限/提问，取消等待，助手消息标记 stopped；权限/提问卡保留但不再驱动后续脚本。

#### 其他

- `isStreaming` 是单一布尔：同一时间只能有一个运行。等待权限/提问时保持 `true`（用户仍可点停止）；按钮显示“停止”。
- 所有 `setSessions` 更新必须不可变：`prev.map(...)`，不要 push/splice 原数组。
- `updatedAt` 在用户发送、助手开始、助手结束、权限/提问解决时更新；流式每个字符更新会不断重排侧栏，禁止。
- 所有定时器在组件卸载时清理：`useEffect(() => () => { runIdRef.current++; timers.forEach(clearTimeout); }, [])`。
- 估算 token：
  ```js
  const estimateTokens = (text) => Math.max(1, Math.ceil(String(text).length / 2));
  ```
  完成时 `setUsage(prev => ({ inputTokens: prev.inputTokens + input, outputTokens: prev.outputTokens + output, contextTokens: Math.min(settings.contextBudget, prev.contextTokens + input + output) }))`。
- 不要在 hook 里 import 任何 React UI 组件；只 import React hooks。

---

## 8. 交互细则（与实现者对齐）

### 8.1 会话

- `SessionSidebar` 排序按 `updatedAt` 倒序；App 不承诺顺序。
- 新建：App 用 `newSession('新对话')` 生成，插到数组最前，设为 active；清空草稿与附件。
- 删除：行内二次确认；App 删除后如果还有会话，选最近更新的一条；如果没有，自动 `newSession('新对话')`，界面永不为空。
- 重命名：行内输入；空标题不提交；不改 `updatedAt`。
- 导出：App 生成 Markdown 并下载；`SessionSidebar` 只调 `onExport(id)`。
- 切换会话：保留各自消息；草稿是 App 全局一份，切换时不清空（简化），但新建/删除时清空。`useMockAgent` 的流式不跨会话：`send` 时传入当前 `activeId`；切换会话不会停止正在进行的运行（用户仍可切回看到流式）。

### 8.2 工作文件夹

- 初始 `D:\Projects\demo`。
- 选择文件夹：mock 改为 `D:\Projects\demo\agent-ui`，toast。
- 清除：`null`，UI 显示“默认位置”。
- 点击路径 chip：toast `已在资源管理器中显示`。
- 不读写真实文件。

### 8.3 消息流

- 日期分隔：按本地日期分组；`Timestamp format="date"`。
- 用户气泡右对齐、助手左对齐由 `ChatMessage sender` 处理；助手用 `variant="ghost"`，用户用默认 `filled`。
- 助手头像 `Avatar name="LFTTDO" size="sm"`。
- 工具卡在助手气泡内；权限卡/已回答提问卡同级（仍在 `ChatMessageBubble` 的 `VStack` 内）。待回答提问由顶层 `QuestionDialog` 居中显示；点「稍后回答」后，气泡内只显示「模型提问 · 待回答」紧凑提示和「继续回答」按钮。
- 最后一条助手消息下方：复制 + 重试；重试只在 `isLast && !isStreaming && !stopped` 时显示。
- 停止：助手消息末尾显示“已停止”，不删除已生成内容。
- 不实现通知条/致命错误条/思考折叠；它们的 Block 类型不在本期契约里。工具错误用 `ChatToolCalls` 的 `status="error"` + `errorMessage` 表达。

### 8.4 输入区

- Enter 发送，Shift+Enter 换行，IME 组合中不发送：由 `ChatComposer`/`ChatComposerInput` 处理，不要自己监听。
- `/` 菜单只列启用技能；选中插入 `使用「技能名」技能：`。
- `@` 菜单列 15 条相对路径，最多 20；选中插入 `@路径`。
- 粘贴/拖拽图片：`ChatComposerInput.onFiles` → App `onAddFiles`；最多 4 张；超出 toast `一次最多 4 张，已忽略 N 张`。
- 附件缩略图 72×72 级别：用 `width="4.5rem"` 的 `VStack` + 图片高度 `4.5rem`；可移除。
- 发送中：`isStopShown` 显示停止；`onStop` 调 hook。空输入不发送。
- 模型下拉在输入坞 footer：按钮显示当前 `modelName`，菜单项来自 `models`；流式中禁用切换；切换调 `onModelChange` 并 toast，设置面板「模型名」同步。每条助手消息记录发送时的模型（`turn.model`），历史消息不随之后切换回改。

### 8.5 状态栏

- 输入/输出 token 来自 `usage`；上下文占用 `usage.contextTokens / contextBudget`。
- >85% 变危险色 + “上下文较满：下次发送将自动压缩较早的对话”。
- 条宽用 `7.5rem` 包裹，不写 px。

### 8.6 面板

- 四个面板互斥：App 的 `panel` 只有一个值；`onOpenPanel(next)` 如果 next 已是当前值则关闭（toggle），否则打开 next。
- 四类面板都用 `Drawer` 右侧全高；模型提问用居中的 `QuestionDialog`，不要用 `Drawer` 承载提问。
- 关闭：`Drawer` 的 `onOpenChange(false)`、头部关闭按钮、底部关闭按钮都调 `onClose`。
- 所有保存/刷新/打开都是 mock + toast；不写磁盘。
- MCP 的保存不改变 `mcpServers`；Hooks 的信任只改 state；技能启用只改 state。
- 设置的字号/强调色即时生效（App 的 runtime theme）；刷新页面回到初始值。

### 8.7 文案

必须使用以下文案（可增加辅助说明，但不要改主文案）：

- 侧栏：`新建对话`、`重命名`、`导出`、`删除`、`删除会话？`、`确认删除`、`取消`、`提交重命名`、`设置`、`技能`、`MCP`、`Hooks`、`展开侧栏`、`收起侧栏`、`会话列表`。
- 工作文件夹：`工作文件夹`、`默认位置`、`选择文件夹`、`清除`。
- 消息：`需要你的许可`、`仅本次`、`本会话内都允许`、`拒绝`、`已放行`、`已拒绝`、`模型提问`、`模型提问 · 待回答`、`继续回答`、`回答后继续当前任务`、`自定义回答`、`稍后回答`、`提交`、`复制`、`已复制`、`复制失败`、`重试这一轮`、`已停止`、`工具返回的图片`、`开始一段新对话`、`有什么都可以聊，也能拖张图进来。`
- 输入：`说点什么…（Enter 发送，Shift+Enter 换行）`、`添加图片`、`移除`、`一次最多 4 张，已忽略 N 张`、`技能快捷菜单`、`文件提及菜单`、`发送`、`发送中…`、`停止`、`停止中…`。
- 状态栏：`本轮 token：输入 X · 输出 Y`、`上下文较满：下次发送将自动压缩较早的对话`。
- 设置：`设置`、`模型`、`接口地址（base_url）`、`模型名`、`API Key`、`当前`、`未配置`、`保存`、`清除已存的 key`、`确认清除`、`界面`、`主题`、`字号`、`强调色`、`权限`、`权限模式`、`规则（默认）`、`全部询问`、`信任模式`、`Plan 模式`、`文件访问`、`可访问目录`、`人设`、`人设（系统提示词）`、`从 .txt 导入`、`导出 .txt`、`恢复内置`、`长期记忆`、`事实`、`偏好`、`待办`、`还没有长期记忆`、`清空全部记忆`、`储存`、`默认储存位置`。
- 技能：`技能`、`启用`、`用这个技能`、`刷新`、`打开技能文件夹`、`关闭`。
- MCP：`MCP 服务器`、`编辑 mcp.json`、`已连接`、`未连接`、`连接中…`、`本服务器全部允许`、`每次询问`、`禁止`、`校验`、`保存并重启`、`配置格式正确`、`配置格式有误`、`配置已保存，重启应用后生效。当前会话仍按启动时的连接与工具运行。`、`还没有配置服务器。点击「编辑配置」直接写 mcp.json。`
- Hooks：`Hooks`、`用户作用域`、`工作区级`、`信任`、`撤销信任`、`启用工作区级 hooks`、`打开 hooks.json`、`打开数据目录`、`打开工作区目录`。
- 技能引导语：`使用「技能名」技能：`。

---

## 9. 实现顺序建议

四个实现者可以并行，但接口按以下顺序对齐：

1. **外壳**：先写 `App.jsx` 的状态骨架 + 四个占位组件（只 import，不实现别人的文件），保证 `main.jsx` 能渲染；然后写 `SessionSidebar`、`WorkspaceBar`。
2. **消息流**：`ToolCards` 先完成（权限/提问/工具映射），再 `MessageTurn`，最后 `ChatPane`。
3. **输入与模拟**：`useMockAgent` 先完成 `send`/`stop`/`respondPermission`/`answerQuestion`/`retryLast` 的骨架，再 `Composer`、`StatusBar`。
4. **面板**：四个面板可以同时写；先 `SettingsPanel`（字段最多），再 `SkillsPanel`、`McpPanel`、`HooksPanel`。

集成时外壳实现者负责把 App 的 state/回调接到四个组件；其他三人不要改 App。

---

## 10. 每个实现者的自检清单

### 外壳实现者

- [ ] `App.jsx` 使用 `initialSessions` / `initialSettings` / `initialMemories` / `initialSkills` / `initialMcpServers` / `initialHooks`，没有硬编码会话或设置。
- [ ] `useMockAgent` 只调用一次，传 `sessions, settings, setSessions, setUsage`。
- [ ] `AppShell` 的 `sideNav` 是 `SessionSidebar`；主列是 `WorkspaceBar` + `ChatLayout` + `StatusBar`。
- [ ] `ChatLayout` 的 `composer` 是 `Composer`，children 是 `ChatPane`；`ChatPane` 只渲染 `ChatMessageList`。
- [ ] 新建/切换/重命名/删除/导出会话全部可用；删除最后一条后自动新建。
- [ ] 四个面板互斥打开；`onOpenPanel` 的四个值正确。
- [ ] 工作文件夹选择/清除/显示 toast。
- [ ] 运行时主题 `defineTheme` 使用 `settings.fontSize` / `settings.accent`。
- [ ] 没有 raw `<div>`/`<span>`、没有裸 hex/px、没有 CSS 文件。

### 消息流实现者

- [ ] `ChatPane` 只返回 `ChatMessageList`。
- [ ] 日期分隔正确；`MessageTurn` 的 `isLast`/`isStreaming` 正确。
- [ ] 四类工具状态（pending/running/complete/error）都能显示；`result` 用 `CodeBlock`。
- [ ] 权限卡三个按钮调 `onPermission(id, 'once'|'session'|'deny')`；解决后显示已放行/已拒绝。
- [ ] 待回答提问以居中 `QuestionDialog` 出现；支持 2–6 选项 + 自定义回答；提交调 `onQuestionAnswer(id, answer)`。
- [ ] 点「稍后回答」收起弹窗后，消息流只显示「模型提问 · 待回答」紧凑提示；点「继续回答」可重新打开弹窗。
- [ ] 图片卡用 `AspectRatio` + `<img>`；文件卡路径 chip 点击 toast。
- [ ] 复制/重试按钮只在正确条件显示；复制失败有 toast。
- [ ] 没有 raw `<div>`/`<span>`、没有裸 hex/px。

### 输入与模拟实现者

- [ ] `useMockAgent` 的 `send` 能追加用户 + 助手消息，流式文本，工具状态变化。
- [ ] `read`/`command`/`permission`/`question`/`default` 五个脚本至少各走通一次。
- [ ] `settings.permissionMode` 三档和 `planMode` 真正影响脚本。
- [ ] `respondPermission` 既能恢复运行中的等待，也能更新 `session-3` 的静态权限卡。
- [ ] `answerQuestion` 能更新 `session-4` 的静态提问卡。
- [ ] `retryLast` 截断最后一条助手并重播；`stop` 保留已生成内容并标记 stopped。
- [ ] `Composer` 的 `/` 只列启用技能，`@` 列 15 条路径；`onFiles` 接收图片。
- [ ] 附件抽屉最多 4 张；移除可用。
- [ ] `StatusBar` 在 >85% 时变危险色并显示警告。
- [ ] 没有 raw `<div>`/`<span>`、没有裸 hex/px。

### 面板实现者

- [ ] 四个面板都是右侧 `Drawer`，互斥由 App 控制；关闭按钮和 `onOpenChange` 都调 `onClose`。
- [ ] 设置：端点/模型/API Key/预算/字号/强调色/权限模式/Plan/roots/人设/记忆/储存全部可操作。
- [ ] 字号、强调色变更即时反映到整个应用（App runtime theme）。
- [ ] 长期记忆增/改/删/清空；清空有二次确认。
- [ ] 技能启用/停用；停用后 `/` 菜单同步（由 Composer 过滤）。
- [ ] MCP 信任切换、JSON 校验、保存 toast；安全提示可见。
- [ ] Hooks 信任/撤销、工作区 hooks 二次确认、三个打开按钮 toast。
- [ ] 没有 raw `<div>`/`<span>`、没有裸 hex/px。

---

## 11. 组件名陷阱（0.7.16 实际核对）

- **没有 `Select`**：用 `Selector`（单选下拉）或 `RadioList`（可见选项）。
- **没有 `Radio` / `Checkbox`**：用 `RadioList` + `RadioListItem`；布尔开关用 `Switch`。
- **没有 `Image`**：用 `AspectRatio` + 原生 `<img>`。
- **`IconButton` 不是 `Button` 的别名**：图标按钮用 `IconButton`，文本按钮用 `Button`；`Button` 的 `isIconOnly` 也可以，但本项目统一用 `IconButton`。
- **`ChatComposerInput` 的 trigger `onSelect` 返回字符串时是纯文本**，不是 token；本项目利用这一点插入 `使用「技能」技能：` 和 `@路径`。
- **`Drawer` 没有 `DrawerHeader`**：自己组合 `HStack` + `Heading` + `IconButton`。
- **`MoreMenu` 的 `items` 是 `DropdownMenuOption[]`**：`{label,onClick}`、`{type:'divider'}`、`{label,variant:'destructive'}`。
- **`Token` 的 `color` 取值**：`default|red|orange|yellow|green|teal|cyan|blue|purple|pink|gray`；状态/风险标签用它，不用 `Badge`。
- **`Badge` 只用于数量**：如附件数、未读数；不要用 Badge 表示“已连接/写入/只读”。

---

## 12. 已知问题与环境记录（必须知道）

1. **主题 PNG 检查失败**：
   - 命令：`npx astryx experimental theme editor new lfttdo-noir --like neutral --accent "#7c8cff" --neutral "#7b849c" --surfaces glass --corners large --spacing roomy --mode dark-only --vibrancy 65 --png`
   - 结果：主题创建成功，所有 dial 被接受；但 `--png` 没生成图片，输出：
     ```
     [warn] saved, not checked (ERR_THEME_NOT_CHECKED): Could not render the components with this project's React:
     The page threw while rendering: Unknown file extension ".css" for ...\themes\lfttdo-noir\lfttdo-noir.css
     ```
   - 去掉 `--png` 重跑 `new` 时 CLI 拒绝：`Theme "lfttdo-noir" already exists. Nothing was changed.`；主题已由第一次命令创建，后续用 `editor set` 调整。
   - `npx astryx experimental theme editor show lfttdo-noir --png` 也同样失败。
   - 因此本轮**没有 PNG 可供肉眼检查**；主题的深蓝黑/玻璃方向是通过生成 CSS token 验证的（`--color-background-body: #0a0d1c`、`--color-accent: #7c8cff`、app-shell radial gradient、card/dialog/drawer glass）。不要在实现里试图“修”主题；如果总控能在另一个 Node/Playwright 环境重跑 `show --png`，再补视觉检查。
2. **主题 flags 全部被接受**：`--like neutral`、`--accent`、`--neutral`、`--surfaces glass`、`--corners large`、`--spacing roomy`、`--mode dark-only`、`--vibrancy 65` 均写入 `themes/lfttdo-noir/lfttdo-noir.settings.json`；没有丢 flag。
3. **`theme add` / `theme use` 成功**：`astryx-themes.js` 默认 `lfttdo-noir`；`npx astryx doctor` 的 `theme-default` 检查为 ok。
4. **`astryx` 命令**：doctor 报 `cli-on-path` 警告——`node_modules\.bin\astryx.cmd` 不是 CLI。所有命令请用 `npx astryx ...`，不要裸 `astryx`。
5. **`theme-fonts` 警告**：主题继承了 Figtree 外部字体；不影响本次深色界面，不要因此改字体或引 CDN。
6. **Heroicons 已安装**：`@heroicons/react@^2.2.0`；用 `@heroicons/react/24/outline`。
7. **`src/App.jsx` 目前是旧的 Astryx 配置测试页**：外壳实现者整体替换；另外三人不要动。
8. **`dist/` 是旧构建物**：不要把它当作本轮结果；本轮不跑 build。
9. **Node v24.19.0 / npm 11.17.0**：`node --check` 通过；不要升级/降级 Node。

---

## 13. 交接备注

- 本契约的 props 名称、回调参数、Block 字段是硬约束；实现者可以补充内部辅助函数和局部 state，但不能改这些名字。
- 如果某个 Astryx 组件在实现时发现 props 与 §6.2 不一致，以 `npx astryx component <Name> --props` 的当前安装版本为准，并在自己的交付说明里记录差异；不要改本契约。
- 集成时先跑 `npm run dev` 手动点一遍；总控最后统一跑 `npm run build` 与 `npx astryx doctor --built dist`。
- 交付结果请写：改了哪些文件、每个组件对应哪些 props、手动验证了哪些交互、还有哪些没验证。不要声称跑过没跑过的命令。
