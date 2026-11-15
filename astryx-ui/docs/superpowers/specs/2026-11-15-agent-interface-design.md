# lfttdo Agent 界面设计

日期：2026-11-15
状态：待用户批准
项目：`D:\aido\Astryx-界面制作\Astryx测试1`

## 1. 背景与意图

用户要以 [lfttta/lfttdo](https://github.com/lfttta/lfttdo) 为参照，在现有 Astryx 练习项目中做一个 Agent 界面。

lfttdo 是一个 Windows 桌面 AI 助理：Tauri v2 + Rust + React/TypeScript，深色玻璃拟态界面。它不只是一个聊天窗口，而是一个完整的 Agent 工作台：会话侧栏、工作文件夹、流式对话、工具调用卡、权限确认、技能、MCP、Hooks、长期记忆、上下文压缩提示、设置面板。

本次要做的是**这个界面的高保真可交互前端原型**。目的是：

1. 在 Astryx 设计系统上验证这类复杂 Agent 界面能否做得好看、好用。
2. 得到一个可以真实点击演示的壳，供评审和后续迭代。
3. 为将来接真实模型/工具后端留出清晰的前端边界。

## 2. 目标 / 非目标 / 反目标

### 目标

- 复刻 lfttdo 的信息架构和主要界面：侧栏、工作文件夹栏、消息流、输入区、状态栏、四类面板。
- 深色玻璃视觉：深蓝黑底、靛蓝强调色、半透明层次、柔和光晕、大圆角。
- 全套交互可点：切会话、发消息、模拟流式、工具调用展开、权限允许/拒绝、`/` 技能、`@` 文件、设置改主题、面板开关、导出会话。
- 桌面优先（1280×800 及以上），在 900px 以下侧栏自动收起，768px 以下走 AppShell 移动抽屉。
- 严格用 Astryx 组件与 token 表达布局和样式，不写 raw `<div>`/`<span>` 布局，不写裸 hex/px。

### 非目标

- 不接真实模型 API，不产生真实推理。
- 不做 Rust/Tauri 后端、不做工具真实执行、不读写本地文件。
- 不做会话持久化；刷新页面回到预置演示数据。
- 不做 MCP/Hooks 的真实运行，只做配置界面和状态演示。
- 不逐像素复刻 lfttdo 的 CSS（尤其是完整的 backdrop-filter 玻璃层）。

### 反目标（即使技术可用也算失败）

- 只能看不能点的静态截图式页面。
- 为了像 lfttdo 而绕开 Astryx：组件里堆 raw div/span、内联 hex、自造一套布局。
- 丢掉 Agent 特征，退化成普通聊天页：没有工具卡、权限卡、工作文件夹、上下文占用条、面板。
- 演示时必须输真实 API Key 才能看到内容。

## 3. 约束

- 现有项目：React 19.3 + Vite 8.3，入口 `src/main.jsx`、页面 `src/App.jsx`，纯 JavaScript（无 TS 编译配置）。
- Astryx `@astryxdesign/core` / `cli` 0.7.16，当前 neutral 主题。
- Astryx 规则：`AppShell` 作为页面根；布局用 VStack/HStack/Grid 等组件；样式先用组件 props，再用 `var(--color-*)` / `var(--spacing-*)` token；不使用 StyleX/Tailwind 编译器。
- 主题通过 Astryx 主题系统创建/修改，禁止 `:root` 覆盖。
- 模板 `ai-chat` 依赖 `@heroicons/react@^2.2.0`，需要安装。
- 现有 `astryx-themes.js` 是 CLI 生成文件，不手改；主题通过 `astryx theme add` 接入。
- 交付前必须通过 `npx astryx doctor --built <dir>`。

## 4. 方案选择

### 方案 A：高保真可交互原型（用户已选）

纯前端 + mock 数据；流式输出、工具调用、权限门、面板全部本地模拟。

- 优点：能完整演示信息架构和交互；一天内可交付；后续可把 mock 层替换成真实 IPC。
- 代价：不产生真实模型输出，工具不落地执行。

### 方案 B：静态视觉稿（未选）

只做一屏聊天界面，交互限于悬停和展开。

- 优点：最快。
- 代价：无法演示 Agent 的流程与状态，面板和权限门看不到。

### 方案 C：接入真实模型（未选，建议二期）

需要后端代理（API Key 不能放前端）、SSE 流式、会话持久化、工具执行与权限门。

- 优点：真能用。
- 代价：从 UI 项目变成全栈项目，工作量数倍，且涉及安全边界。

用户于 2026-11-15 选择：**方案 A + 全套面板 + 深色玻璃风**。

## 5. 设计

### 5.1 总体结构

```
┌────────────────────────────────────────────────────────────┐
│ AppShell                                                   │
│ ┌──────────────┬─────────────────────────────────────────┐ │
│ │ SideNav      │ 主列                                     │ │
│ │ 240px / 56px │ ┌─────────────────────────────────────┐ │ │
│ │              │ │ 工作文件夹栏                        │ │ │
│ │ 品牌/折叠    │ ├─────────────────────────────────────┤ │ │
│ │ 新建对话     │ │ 消息流（独立滚动、自动贴底）        │ │ │
│ │ 会话列表     │ │  - 通知条 / 致命错误                │ │ │
│ │  ...         │ │  - 用户/助手气泡、工具卡、权限卡    │ │ │
│ │              │ │  - 提问卡、重试按钮                 │ │ │
│ │ 设置 技能    │ ├─────────────────────────────────────┤ │ │
│ │ MCP Hooks    │ │ ChatComposer（毛玻璃底栏）          │ │ │
│ │              │ ├─────────────────────────────────────┤ │ │
│ │              │ │ 状态栏：token 数 + 上下文占用条      │ │ │
│ └──────────────┴─────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

- 根：`<Theme theme={lfttdoNoir} mode="dark">` → `AppShell`。
- 侧栏：`SideNav`，`collapsible` + `resizable`，展开 240px，收起 56px。
- 主列：`Layout`（工作文件夹栏 + ChatLayout + 状态栏）。
- 消息区：`ChatLayout` + `ChatMessageList`；`ChatLayout` 自带毛玻璃底栏、自动滚动和回到底部按钮。
- 消息列宽度：桌面最大 860px 居中；`ChatLayout density="spacious"`。
- 响应式：<900px 侧栏默认收起；<768px 由 AppShell 自动转移动抽屉。

### 5.2 主题与视觉

新建 Astryx 自定义主题 `lfttdo-noir`：

- 基色：深蓝黑 `#0a0d1c` 级别的页面底；抬升面用蓝灰；输入面更亮一档。
- 强调色：靛蓝 `#7c8cff` 级别；悬停更亮；强调色上的文字用深色。
- 语义色：危险红、成功绿、警告琥珀，均从主题生成，不在组件里写死。
- 圆角：大档（约 16–20px），小元素用小档。
- 字体：系统 UI + `Microsoft YaHei`；代码 `Cascadia Mono` / `Consolas`。
- 明暗：本次固定深色，只提供 dark 值；不提供明暗切换控件（避免出现半成品 light 主题）。

玻璃质感分三层实现：

1. `ChatLayout` 自带的毛玻璃底栏（frosted glass dock）——输入区的主要玻璃感来源。
2. 主题层为 `app-shell` / `card` / `drawer` 设置半透明表面与边框；半透明和渐变只写在主题 CSS/override 里，不散落在组件。
3. 页面背景用主题层的深色渐变 + 两团低饱和光晕（靛蓝/青），对应 lfttdo 的背景氛围。

限制：Astryx token 不提供任意 `backdrop-filter` 组件级入口。若主题层也无法覆盖某处模糊，接受「半透明 + 边框 + 阴影」的近似，不为此破坏设计系统。

### 5.3 组件映射

| lfttdo 元素 | Astryx |
|---|---|
| 应用根/侧栏 | `AppShell` + `SideNav` |
| 会话行 | `SideNav` 自定义 children + `Button`/`TextInput`/`HStack` |
| 消息流 | `ChatMessageList` |
| 消息分组 | `ChatMessage` |
| 气泡 | `ChatMessageBubble` + `ChatMessageMetadata` |
| 工具调用卡 | `ChatToolCalls`（运行/完成/错误、结果详情、diff 统计） |
| 权限确认卡 | `Card` + `HStack` + `Button` + `Badge` |
| 模型提问 | 居中 `Dialog` + `DialogHeader` + `Layout` 内容/页脚（选项 + 自由输入） |
| Markdown/代码 | `Markdown` / `CodeBlock` |
| 思考过程 | `Collapsible` |
| 输入区 | `ChatComposer` + `ChatComposerInput` |
| 附件缩略图 | `ChatComposer` 的 `drawer` + `Token`/缩略图卡片 |
| `/` 技能菜单、`@` 文件菜单 | `ChatComposerInput` 的 `triggers` |
| 上下文占用条 | `ProgressBar`（`headerContext`） |
| 面板 | `Drawer`（右侧全高）+ `Layout`/`DrawerHeader` |
| 设置分区导航 | `settings-sidebar` 模板结构 |
| 通知/错误 | `Banner` / 自定义消息条 |
| 轻提示 | `useToast` / `ToastViewport` |
| 图标 | `@heroicons/react/24/outline` |

### 5.4 各区域细节

#### 侧栏

- 顶部：产品名「LFTTDO」+ 折叠按钮。
- 新建对话：主按钮，占满宽度；收起态为图标按钮。
- 会话列表：按 `updatedAt` 倒序；当前会话高亮。
  - 行点击切换会话。
  - 行尾 `MoreMenu`：重命名、导出、删除。
  - 重命名：行内 `TextInput` + 确认/取消。
  - 删除：行内两段式确认（“删除会话？” + 确认删除 / 取消），不弹全屏对话框。
- 底部：设置、技能、MCP、Hooks 四个入口；收起态只剩图标 + 可访问名称。
- 空态：不出现。启动和删除最后一个会话后都自动保留至少一条「新对话」。

#### 工作文件夹栏

- 左侧标签「工作文件夹」/「默认位置」。
- 中间路径 chip：点击 Toast 提示「已在资源管理器中显示」（mock）。
- 右侧「选择文件夹」「清除」两个次要按钮；点击更新 mock 路径并 Toast。

#### 消息流

- 顶部日期分隔（`ChatSystemMessage variant="divider"`）。
- 用户消息：右侧气泡，`bg-input` 视觉；附件用 `Token` 列表；时间元数据。
- 助手消息：左侧，头像 + ghost 气泡；可含：
  - Markdown 正文。
  - `CodeBlock`。
  - 思考过程 `Collapsible`（默认收起，标题「思考过程 · Ns」）。
  - `ChatToolCalls`：每个调用有 name / target / status / duration；多调用折叠成组；结果用 `resultDetail` 的代码块。
  - 权限卡：标题「需要你的许可」、工具名与参数、`允许一次` / `允许本会话` / `拒绝`；选择后卡片变为已解决状态。
  - 执行卡：运行中 spinner → 完成输出；失败显示错误色。
  - 文件产物卡：文件名 + 路径 + 打开位置按钮。
  - 图片结果：缩略图网格（最多 4 张）。
- 通知条：普通通知为低对比条；致命错误用 `Banner` 语义色；连续重复合并 `×n`，超过 5 组折叠为「展开（另有 N 条）」。
- 最后一条助手消息下方：「重试这一轮」按钮；点击后截断该轮并重新播放 mock 回复。
- 提问：待回答时以居中 `Dialog`（宽 520）出现，2–6 个选项 + 自由输入；提交后作为用户回答插入，消息流保留「已回答」记录。点「稍后回答」或关闭按钮收起弹窗，消息流只留一条「模型提问 · 待回答」紧凑提示，点「继续回答」可重新打开弹窗。
- 会话过长时顶部显示「展开更早的 N 条」；默认只渲染最近 40 条（mock 数据远小于此，但逻辑保留）。

#### 输入区

- `ChatComposer`，`drawer` 显示待发送图片缩略图（可删除），`headerActions` 放「图片」「@ 文件」「/ 技能」按钮，`footerActions` 放可点击的模型下拉（切换后新消息记录所选模型），`headerContext` 放上下文占用条与百分比。
- `ChatComposerInput`：
  - Enter 发送，Shift+Enter 换行，IME 组合中不发送。
  - `/` 触发技能菜单：显示 `/技能名`、描述、参数提示；选中后插入「使用「技能」技能：」引导语。
  - `@` 触发文件菜单：相对路径子串匹配，最多 20 条；选中插入文件 token/路径。
  - `onFiles` 接收粘贴/拖拽图片，生成缩略图加入 drawer。
  - `hasHistory` 开启，ArrowUp 召回历史。
- 发送中：发送按钮变停止按钮；停止后消息以「已停止」收尾。
- 空输入不可发送。
- 状态栏：本轮输入/输出 token 数 + `ProgressBar` 上下文占用；>85% 时变危险色并显示「上下文较满：下次发送将自动压缩较早的对话」。

#### 面板

四类面板统一用右侧全高 `Drawer`，互斥打开；关闭后焦点回到触发按钮。

1. **设置**（宽 720–840px）
   - 左侧分区导航（模型、外观、权限、人设、长期记忆、文件访问），右侧内容。
   - 模型：接口地址、模型名、API Key（密码框 + 保存/清除）、上下文预算。
   - 外观：当前主题（只读 `lfttdo-noir`）、字号、强调色；后两项即时生效并 Toast。
   - 权限：Rules / AskAll / Trusted 三档；Plan 模式开关；说明文字。
   - 人设：多行文本、导入 `.txt`、导出、恢复内置。
   - 长期记忆：列表、编辑、删除、新增（kind/body/due）、清空（二次确认）。
   - 文件访问：只读目录列表 + 添加/移除（mock）。

以上控件全部可操作（mock 级）：保存、清除、导入、导出、增删改、清空都会更新界面状态并给出 Toast；不写磁盘、不持久化。
2. **技能**
   - 技能卡片：名称、描述、启用开关、「用这个技能」。
   - 刷新、打开技能文件夹（Toast）。
3. **MCP**
   - 服务器列表：名称、transport（stdio / streamable-http）、状态（已连接/停止）、信任档（全部允许/每次询问/禁止）。
   - 内嵌 `mcp.json` 编辑区（TextArea）+ 校验 + 保存并重启（mock）。
   - 安全提示：配置文件可能含 env/headers，不要在共享屏幕时打开。
4. **Hooks**
   - Hook 列表：事件（PreToolUse/PostToolUse/SessionStart/Stop）、命令、信任状态（声明 SHA-256）、信任/撤销。
   - 工作区 Hooks 总开关（默认关，打开需二次确认）。
   - 打开 hooks.json（Toast）。

#### 轻提示与确认

- 所有成功/失败反馈走 `useToast`，不打断对话。
- 破坏性操作（清空记忆、撤销 hook 信任、打开工作区 hooks）二次确认。
- 通知区域使用 `aria-live="polite"`。

### 5.5 交互与 mock 行为

- `useMockAgent` 是唯一产生助手回复的地方：
  - 根据用户输入关键词选择预置脚本（读文件、改文件、执行命令、抓网页、默认回答）。
  - 文本按 20–40ms 的节奏分块推送，用 `useStreamingText` 平滑显示。
  - 在脚本节点插入工具调用：`running` → 800–1500ms 后 `complete`，带 duration 与结果。
  - 需要权限的脚本先发权限卡，用户允许后继续；拒绝则助手改口并结束。
  - 结束时更新 token 计数与上下文占用。
- 会话：
  - 预置 4 条会话，标题与内容各不相同，覆盖纯文本、工具、权限、图片、代码。
  - 新建会话后首条消息自动生成标题（截取前 16 字）。
  - 切换会话保留各自消息与草稿。
  - 删除当前会话后自动切到最近一条；删掉最后一条时自动新建「新对话」，界面永不为空。
  - 导出生成 Markdown Blob 并触发下载，文件名清洗非法字符。
- 设置：主题固定；字号/强调色真实作用到 Astryx 主题层；其余设置保存在 React state（刷新丢失），并且**必须影响 mock 行为**：
  - 权限档：Rules = Read 自动放行、Write/Exec 弹权限卡；AskAll = Write/Exec 每次都弹；Trusted = 全部自动放行。
  - Plan 模式：Write/Exec 工具卡直接变拒绝，并显示「请先退出 Plan 模式」。
  - 技能启用：停用的技能不出现在 `/` 菜单。
  - 上下文预算：改变占用条分母与 85% 告警时机。
  - 模型名：输入区的下拉切换与之后新消息的元数据都跟随它；接口地址只影响设置回显。
- 技能/文件菜单：数据来自 `src/data/mock.js`，不访问文件系统。

### 5.6 数据模型（前端 mock）

```js
Session = { id, title, updatedAt, messages: Message[] }
Message = {
  id, role: 'user' | 'assistant',
  blocks: Block[],
  createdAt,
  isStreaming?,
  stopped?,
}
Block =
  | { type: 'text', text }
  | { type: 'code', language, title?, code }
  | { type: 'tool', call: ToolCall }
  | { type: 'permission', request: PermissionRequest }
  | { type: 'question', request: QuestionRequest }
  | { type: 'image', src, alt }
  | { type: 'file', path, name }
ToolCall = {
  id, name, target, status: 'pending'|'running'|'complete'|'error',
  duration?, additions?, deletions?, result?, errorMessage?, node?
}
PermissionRequest = {
  id, tool, args, risk: 'read'|'write'|'exec',
  decision: null | 'once' | 'session' | 'deny',
}
Skill = { name, description, enabled, argumentHint? }
McpServer = { name, transport, status, trust }
Hook = { id, event, command, trusted, source: 'user'|'workspace' }
Settings = {
  endpoint, model, hasApiKey, contextBudget,
  theme, mode, fontSize, accent,
  permissionMode, planMode,
  persona, memories, roots,
}
```

### 5.7 文件结构

```
src/
  App.jsx                       应用外壳与状态编排
  main.jsx                      Theme provider（dark）
  data/mockData.js              会话、消息、技能、MCP、Hooks、设置初值
  hooks/useMockAgent.js         模拟流式、工具调用、权限流程
  components/
    SessionSidebar.jsx
    WorkspaceBar.jsx
    ChatPane.jsx
    MessageTurn.jsx
    ToolCards.jsx               工具/权限/执行/文件/图片卡
    QuestionDialog.jsx          模型提问的居中弹窗（内联卡作为回退）
    Composer.jsx
    StatusBar.jsx
    panels/
      SettingsPanel.jsx
      SkillsPanel.jsx
      McpPanel.jsx
      HooksPanel.jsx
  theme/                        主题生成物与切换逻辑（如需要）
docs/superpowers/specs/2026-11-15-agent-interface-design.md
```

实现时先执行 `npx astryx template ai-chat --type page <临时目录>` 读取模板结构，再把内容搬进上面的组件；模板是 TSX，需要转成 JSX 并去掉未用的示例内容。

### 5.8 实现顺序

1. **主题与外壳**：创建 `lfttdo-noir` 主题；接入 `main.jsx`；搭 AppShell + SideNav + 主列骨架；验证折叠与响应式。
2. **消息流**：ChatLayout/ChatMessageList；用户/助手气泡；Markdown/CodeBlock；思考折叠；工具卡；通知条；上下文条。
3. **输入区**：ChatComposer；`triggers` 的 `/` 与 `@`；图片粘贴/拖拽；发送/停止；模拟流式。
4. **会话管理**：预置会话、切换、新建、重命名、删除两段式、导出、轻提示。
5. **四类面板**：设置、技能、MCP、Hooks；统一 Drawer 与分区导航。
6. **打磨与验收**：空态、错误态、键盘、屏幕阅读器标签、窄屏；构建与 doctor；浏览器逐项操作。

### 5.9 验收标准

- `npm run dev` 启动，浏览器打开后主界面完整，无控制台报错。
- 可完成以下操作且界面正确响应：
  - 切换/新建/重命名/删除/导出会话。
  - 发送消息，看到模拟流式回复、工具卡从运行中变为完成、权限卡允许/拒绝后的不同结果。
  - 模型提问以居中弹窗出现，选择/自定义回答可提交；「稍后回答」后只留紧凑提示，点「继续回答」可重新打开。
  - `/` 技能菜单、`@` 文件菜单、粘贴图片、停止生成。
  - 打开四个面板：设置里改字号/强调色即时生效；长期记忆可增删改清空；技能可启用/停用且 `/` 菜单同步；MCP JSON 可校验并给出结果；Hook 信任可切换。
  - 切换权限档/Plan 模式后，下一条消息的工具卡行为随之变化。
  - 上下文占用条随对话增长，超过 85% 出现红色提示。
- `npm run build` 成功；`npx astryx doctor --built dist` 通过。
- 组件代码中无 raw `<div>`/`<span>` 布局、无裸 hex/px（主题文件除外）。
- 在 1280×800 与 900×700 两个尺寸下布局不破。

### 5.10 风险与回退

| 风险 | 回退 |
|---|---|
| 主题编辑器不支持渐变/半透明背景 | 用主题 `components` override 写 CSS 属性；再不行退回纯深色 + 阴影层次 |
| `ChatComposerInput` triggers 的搜索源接口与预期不符 | 读 `useChatComposerContext` 文档，改用自定义输入并接同一契约；最差用 `Popover` + `TextInput` 模拟菜单 |
| `ChatToolCalls` 不适合承载自定义权限卡 | 工具调用仍用 ChatToolCalls；权限/提问用 `Card` 单独渲染，不硬塞进 calls |
| ai-chat 模板是 TSX，项目是 JSX | 手工转 JSX，保留布局与组件用法，删除示例数据 |
| Drawer 内设置分区导航在大屏过宽 | 设置面板改用 `Dialog variant="fullscreen"` + 左侧分区导航 |
| 玻璃效果导致可读性下降 | 提高表面不透明度、降低光晕强度，保证正文对比度 |

## 6. 留给实现者的决定

以下由实现者按实际情况决定，不需要再问用户：

- 主题具体色值、圆角与间距的精确取值（在既定方向内微调）。
- mock 文案、会话标题、工具调用的具体命令与输出。
- 每个组件的确切 props（实现前用 `npx astryx component <Name>` 核对）。
- 面板内部的分区顺序与控件密度。
- 是否需要把模板的 StyleX 样式改写成组件 props（优先 props）。
- 窄屏断点的具体像素（在 768/900 附近微调）。
