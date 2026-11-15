# Astryx UI 接入 LFTTDO 设计

- 日期：2026-11-15
- 分支：`feat/astryx-ui`
- 基线：`f412d7e`（`main`）
- 状态：方案待评审（本分支先提交设计与实施计划，不改 Rust、不改现有 `src/`）
- Astryx 原型：`D:\aido\Astryx-界面制作\Astryx测试1`

## 1. 背景

`Astryx测试1` 已经有一套可运行的高保真 Agent 界面：深色玻璃主题、会话侧栏、消息流、工具卡、权限卡、模型提问弹窗、输入坞、设置/技能/MCP/Hooks 面板。它目前由 `src/hooks/useMockAgent.js` 的定时器脚本驱动，**没有真实模型、工具、文件、MCP 与会话持久化**。

LFTTDO 是完整的 Tauri v2 + React + Rust 桌面 Agent：真实模型流、内置工具、权限门、MCP、SQLite 会话、长期记忆、Hooks 都已经实现。本方案要做的不是重写 Agent，而是**让 Astryx 界面成为 LFTTDO 的另一套前端**，直接复用 LFTTDO 的 Rust 内核和全部 IPC 能力。

## 2. 目标与非目标

### 目标

1. Astryx 界面运行在 LFTTDO 的 Tauri 窗口里，使用真实模型与工具。
2. 复用 LFTTDO 现有 Tauri 命令与 `agent` 事件通道，**不改 Rust Agent 核心**。
3. 保留现有 `src/` 前端与 354 条前端测试；分支可随时回退。
4. 先打通：文本流、工具卡、权限卡、模型提问、停止、重试、真实会话。
5. 再补齐：设置、图片、工作文件夹、导出、技能/MCP/Hooks、长期记忆。

### 非目标

- 不把 Rust Agent 改写成 HTTP/SSE 服务。
- 不把 `--headless` 当正式后端（它没有权限弹窗、没有 MCP、提问无人应答）。
- 不替换或删除现有 `src/` 前端；不做 `main` 分支合并。
- 不在本分支改 `src-tauri/` 的 Rust 逻辑；发现 IPC 缺口先停下补设计。

## 3. 现状与约束

### 3.1 LFTTDO 的真实边界

- 桌面壳：Tauri v2；前端 `src/`，Rust 核心 `src-tauri/src/`。
- 会话真相：SQLite `messages` 表；`session_messages` 把历史重建为 `AgentEvent[]` 投影。
- 实时事件：Tauri event 通道名 `agent`，信封：

```json
{ "sessionId": "...", "event": { "t": "text_delta", "text": "..." } }
```

- 权限判定只在 Rust `permission/gate.rs`；前端只渲染风险与决定。
- 只有 `src-tauri/src/ipc.rs` 允许 `use tauri`；本方案不碰它。

### 3.2 Astryx 原型现状

- 纯 Web：Vite 8 + React 19 + `@astryxdesign/core`，入口 `src/App.jsx`。
- `useMockAgent` 暴露 `send` / `stop` / `respondPermission` / `answerQuestion` / `retryLast` / `isStreaming` / `streamingSessionId`。
- `QuestionDialog` 已按“待回答提问居中弹窗 + 稍后回答紧凑回退”实现。
- 目前没有 `@tauri-apps/api`，也没有真实 IPC。

### 3.3 本机约束

- 仓库路径含中文时，Rust `aws-lc-sys` 的 NASM 构建可能失败。实施 worktree 使用 ASCII 路径：
  `D:\aido\lfttdo-astryx`（本分支已建立）。
- `git push` 走镜像可能挂起；按仓库 `AGENTS.md`，推送用 GitHub Git Data API。
- 旧前端与 Astryx 前端各自有依赖，不能共用同一份 `node_modules`。

## 4. 方案比较

| 方案 | 做法 | 结论 |
|---|---|---|
| A. Astryx 作为 LFTTDO 的替代前端 | 新目录 `astryx-ui/`，Tauri 配置指向它；Rust 不动 | **选定** |
| B. 直接替换 `src/` | 分支里删除旧前端 | 否决：会毁掉 354 条现有测试与旧 UI 对照 |
| C. 新增 Rust HTTP/SSE 服务 | 把 `AgentSession` 包成 Web 服务 | 否决：重复 IPC 装配，权限/取消/提问都要重做 |
| D. 包一层 `--headless` | 每次消息启动 `lfttdo --headless`，解析 NDJSON | 否决：Write/Exec 被拒、无 MCP、提问无人应答，只能做只读演示 |

方案 A 的代价是仓库里多一份前端依赖；收益是 Rust 侧零改动、旧 UI 与测试完整保留、分支边界清晰。

## 5. 选定架构

```text
LFTTDO Tauri 窗口
├─ astryx-ui/                 React 19 + Astryx（本方案新增）
│  ├─ src/App.jsx             现有界面，接线改为 useAgent
│  ├─ src/hooks/useAgent.js   根据 isTauri() 选 mock / 真实适配器
│  ├─ src/agent/lfttdo/
│  │  ├─ client.js            invoke / listen / 命令封装
│  │  └─ projection.js        AgentEvent -> 现有 Block[] / Message[]
│  └─ vite.config.js          dev server 5173，strictPort
└─ src-tauri/                 Rust 内核（不改）
   ├─ ipc.rs                  send_message / interrupt / respond / question_respond ...
   ├─ agent/                  AgentSession、事件、权限、上下文
   └─ store/                  SQLite 会话真相
```

关键原则：

1. **Rust 是唯一会话真相**；Astryx 的 `sessions` 只是事件流投影，不建立第二份持久化状态。
2. **浏览器里仍可跑 mock**：`isTauri()` 为 false 时继续用 `useMockAgent`，便于不启动 Tauri 调 UI。
3. **Tauri 窗口里走真实 IPC**：`isTauri()` 为 true 时用 `useLfttdoAgent`，所有写操作通过命令，界面变化由事件/命令返回值驱动。

## 6. 目录与构建

### 6.1 新增目录

```text
astryx-ui/
  index.html
  package.json
  package-lock.json
  vite.config.js
  astryx-themes.js
  themes/                  Astryx 主题构建物
  src/                     现有 Astryx 原型源码
    agent/lfttdo/          新增适配层
    hooks/useAgent.js      新增选择器
```

从 `Astryx测试1` 复制时排除：`node_modules/`、`dist/`、`artifacts/`、`.zcode/`、本地使用说明。

### 6.2 Tauri 配置

`src-tauri/tauri.conf.json` 在分支上改为：

```json
{
  "build": {
    "beforeDevCommand": "npm run dev:astryx",
    "devUrl": "http://localhost:5173",
    "beforeBuildCommand": "npm run build:astryx",
    "frontendDist": "../astryx-ui/dist"
  }
}
```

根 `package.json` 增加：

```json
{
  "scripts": {
    "dev:astryx": "npm --prefix astryx-ui run dev",
    "build:astryx": "npm --prefix astryx-ui run build",
    "test:astryx": "npm --prefix astryx-ui run test"
  }
}
```

`astryx-ui/vite.config.js` 使用 `server.port = 5173`、`strictPort = true`，避免 Tauri 等到错误端口。旧 `src/` 的 Vite 仍保留 1420，但 Tauri 不再启动它。

### 6.3 依赖

`astryx-ui/package.json` 增加：

- `@tauri-apps/api`（v2，与 Tauri 版本对齐）
- `vitest`（仅测投影与适配器）

不引入 `@tauri-apps/plugin-shell`、文件系统或 HTTP 插件；所有执行能力留在 Rust。

## 7. 启动、会话与数据流

### 7.1 启动

1. `isTauri()` 为 true 时，App 进入 `booting` 状态，不渲染 mock 初始数据。
2. 并行调用：
   - `settings_get` → 映射为 Astryx `settings`
   - `session_list` → 映射为 Astryx `sessions`
3. 没有会话时调用 `session_create`，拿到 id 后设为 active。
4. 选中会话时调用 `session_messages(id)`，把 `AgentEvent[]` 用 `projection.js` 投影成消息。
5. 完成后再挂载正常聊天界面；投影失败显示可重试错误，不静默吞。

浏览器（非 Tauri）保持现有 mock 启动路径，不调用任何 IPC。

### 7.2 实时事件

订阅一次 `agent` 通道，按 `payload.sessionId` 分发：

- active 会话：直接更新消息流。
- 非 active 会话：也用同一个 `setSessions` 更新对应会话，避免切回来历史断层。
- 未知 sessionId：触发一次 `session_list` 刷新，不丢事件、也不凭空造会话。

`isStreaming` / `streamingSessionId` 由事件驱动，不按“当前 active”推断。

### 7.3 会话切换

- 切到未加载会话：调用 `session_messages` 投影历史。
- 该会话正在流式：不重放历史，避免覆盖实时事件；只显示已投影内容。
- `hydrated` 用 ref/Map 记录；同一会话只加载一次，除非用户手动刷新。
- 重命名/删除/新建后刷新 `session_list`；删除 active 时选最近更新的一条。

### 7.4 发送与回合

- 发送时乐观追加用户消息；**不预先创建助手气泡**，等 `turn_start` 再建。
- `turn_start` 每次工具循环都会发一次，因此一轮用户输入可能产生多条助手气泡；这与 LFTTDO 原生 reducer 一致，Astryx 的 `ChatMessageList` 可以承载。
- 发送失败（IPC reject）：撤掉刚插入的用户消息，Toast/Banner 显示原文。
- 每次发送时捕获当前 `settings.model`，写入助手消息的 `turn.model`；流式中禁止切换模型。

## 8. Tauri IPC 契约

| 命令 | 参数 | 用途 |
|---|---|---|
| `send_message` | `{ sessionId, blocks }` | `blocks` 只允许 `{type:"text",text}` / `{type:"image",data,mime}` |
| `interrupt` | 无 | 打断当前轮 |
| `respond` | `{ requestId, decision }` | 权限决定；返回 bool 只表示“这一笔被闭合” |
| `question_respond` | `{ id, choice }` | 提问回答；返回 bool 是收起弹窗的依据 |
| `session_list` | 无 | 会话行 `{id,title,updated_at}` |
| `session_create` | 无 | 返回新会话 id |
| `session_rename` | `{ id, title }` | 重命名 |
| `session_delete` | `{ id }` | 删除 |
| `session_messages` | `{ id }` | 历史 `AgentEvent[]` |
| `session_retry_last` | `{ id }` | 返回被删那轮的用户 blocks 或 null |
| `session_export` | `{ sessionId }` | 原生保存对话框；返回路径或 null |
| `session_workspace_set` | `{ id, path }` | 设置会话工作文件夹 |
| `session_output_view` | `{ id }` | 当前 workspace / 输出目录 |
| `session_search_files` | `{ sessionId, query }` | `@` 文件提及 |
| `settings_get` | 无 | 扁平 `SettingsView` |
| `settings_set` | `{ settings }` | 写回完整 Settings，不能只发局部键 |
| `settings_set_api_key` | `{ apiKey }` | 明文 key 只走这一条 |
| `normalize_image` | 原始 `ArrayBuffer`/`Uint8Array` | 图片归一化，返回可发送的 image block |
| `pick_folder` | 无 | 选目录 |
| `reveal_path` | `{ sessionId, path }` | 资源管理器定位 |
| `open_url` | `{ url }` | 外部链接，Rust 侧白名单 |

事件订阅：`listen("agent", payload)`，payload 是 `{sessionId, event}`。

## 9. 事件到界面的映射

| `AgentEvent.t` | 关键字段 | Astryx 投影 |
|---|---|---|
| `turn_start` | `turn` | 新建一条助手消息；`isStreaming = true` |
| `text_delta` | `text` | 追加/合并当前助手消息末尾的 `text` 块 |
| `user_message` | `blocks` | 仅重放路径使用；追加用户消息 |
| `tool_call` | `call{id,name,risk,input}`, `risk` | 追加 `tool` 块：`id=call.id`、`status:"running"`、`target` 从 input 推导 |
| `tool_result` | `call_id`, `is_error`, `ms`, `preview`, `images` | 按 `call_id` 更新工具块：状态、耗时、结果、错误；图片追加为图片块 |
| `permission_request` | `id`, `call`, `risk` | 追加 `permission` 块：`id`、`tool=call.name`、`args=call.input`、`decision:null` |
| `permission_resolved` | `id`, `decision` | 按 id 把 `decision` 映射回 `once` / `session` / `deny` |
| `question` | `id`, `question`, `options` | 追加 `question` 块：`answer:null`；现有 `QuestionDialog` 自动打开 |
| `usage` | `input_tokens`, `output_tokens`, `budget` | 更新 `usage`；`contextTokens=input_tokens`，`contextBudget=budget` |
| `context_compacted` | `dropped`, `summary_chars` | 追加通知块/Toast |
| `memory_saved` | `count` | Toast |
| `workflow_step` / `experiment_run` | 进度字段 | 追加通知块/Toast |
| `error` | `where`, `message`, `fatal` | 非致命 → 通知；致命 → `Banner` 错误 |
| `done` | `stop_reason` | `isStreaming=false`；`interrupted` 标记当前助手消息 `stopped`；刷新会话列表 |

实现注意：

1. `turn_start` 会多次出现；不要假设“一轮只有一个助手气泡”。
2. 工具块需要能按 `call_id` 定位。Astryx 现有 `tool` block 没有 id，投影层增加可选的 `id`（`call.id`），UI 渲染不受影响。
3. `permission_resolved` 只能按 id 更新对应权限卡；不要写“最后一张卡”。
4. `question` 没有“已应答”事件；应答结果只以 `question_respond` 返回值和后续 `tool_result` 体现。
5. 历史重放与实时事件走同一个投影函数，区别只在于是否设置 `isStreaming`。

## 10. 交互细节

### 10.1 发送

1. 生成用户消息 id，追加文本/图片块。
2. 调用 `send_message(sessionId, blocks)`。
3. 失败：移除刚追加的用户消息，Toast 显示错误。
4. 成功：等待 `turn_start`；`text_delta` 驱动流式。

### 10.2 停止

- 调 `interrupt()`；不要立刻清空消息流。
- 收到 `done {stop_reason:"interrupted"}` 后设 `isStreaming=false`，当前助手消息标记 `stopped`。
- 若 IPC 本身失败，Toast 提示，保留当前内容。

### 10.3 权限

- 用户点“仅本次 / 本会话内都允许 / 拒绝” → 映射为 LFTTDO `Decision`：
  - `once` → `{allow:true, scope:"once"}`
  - `session` → `{allow:true, scope:"session"}`
  - `deny` → `{allow:false, reason:"用户拒绝"}`
- 调 `respond(requestId, decision)`；**不直接改卡片状态**。
- 只有 `permission_resolved` 事件能改卡片；命令返回 false 或 reject 时权限卡保持待决，Toast 说明。
- 这与 LFTTDO 前端现有约定一致，避免“界面以为放行了、Rust 还在等”。

### 10.4 提问

- `question` 事件 → 追加 `question` 块，`QuestionDialog` 自动打开。
- 提交调 `question_respond(id, choice)`：
  - 返回 true：本地把该块标记 `answer=choice`，关闭弹窗，追加一条本地用户回答记录（仅界面，不写库），后端随后用 `tool_result` 继续。
  - 返回 false：Toast “这道提问已失效”，关闭弹窗。
  - IPC reject：Toast，保留弹窗，允许重试。
- 重新打开 Tauri 时，未回答的提问不会从历史恢复（LFTTDO 事件流没有“未决提问”的持久化帧）；这是与原生前端一致的边界，需在 UI 文案/文档中说明。

### 10.5 重试

1. 调 `session_retry_last(sessionId)`。
2. 返回 `null`：Toast“没有可重试的轮次”。
3. 返回 blocks：本地移除最后一轮助手消息，调用发送逻辑重发这些 blocks。
4. 不自行发明“删除数据库”逻辑，截断由 Rust 完成。

### 10.6 会话、工作文件夹、导出

- 新建/重命名/删除都走命令，成功后刷新 `session_list`。
- 工作文件夹按会话：`session_workspace_set`；`session_output_view` 获取当前 workspace/output dir。
- 导出走 `session_export`，由 Rust 弹原生保存对话框；前端只显示结果路径，不再用 Blob 下载。
- 文件卡点击走 `reveal_path(sessionId, path)`；不要在 WebView 里拼 `file://`。

### 10.7 模型切换

- 下拉切换写入 settings；`settings_set` 必须提交完整 Settings 对象，不能只提交 `{model}`，否则 carry-through 字段会丢。
- 流式中禁用切换（现有 UI 已做）。
- LFTTDO 没有模型列表命令；真实模式下保留“当前模型 + 静态建议列表”，不伪造可用性。

### 10.8 图片

- 粘贴/拖拽图片先转成原始字节，调 `normalize_image`，拿返回的 `{type:"image",data,mime}` 再放进 `send_message`。
- 不在前端做缩放/转码第二套逻辑；Rust 是唯一图片闸门。

## 11. 数据映射

### 11.1 会话

| LFTTDO | Astryx |
|---|---|
| `SessionRow.id` | `Session.id` |
| `SessionRow.title` | `Session.title`（空 → “新对话”） |
| `SessionRow.updated_at`（毫秒） | `Session.updatedAt`（ISO 字符串，便于现有日期分组） |

### 11.2 设置

| LFTTDO `SettingsView` | Astryx `settings` |
|---|---|
| `base_url` | `endpoint` |
| `model` | `model` |
| `budget_tokens` | `contextBudget` |
| `font_size` | `fontSize` |
| `accent` | `accent` |
| `permission_mode` | `permissionMode`（`ask_all` ↔ `askAll`） |
| `plan_mode` | `planMode` |
| `roots` | `roots` |
| `system_prompt` | `persona` |
| `api_key_masked != null` | `hasApiKey` |
| 其他字段 | 保存在原始 Settings 对象里，保存时原样带回 |

`settings_set` 只接受完整 `Settings`，不接受 `SettingsView` 的 `api_key_masked` / `corrupt_backup`。实现时保留最近一次 `settings_get` 的原始对象，改字段时展开它，避免丢字段。

### 11.3 用量

- `usage.input_tokens` → `usage.inputTokens`
- `usage.output_tokens` → `usage.outputTokens`
- `usage.input_tokens` → `usage.contextTokens`（当前上下文近似值）
- `usage.budget` → 状态栏分母；`settings.contextBudget` 只在 mock / 初次加载时兜底。

### 11.4 工具与权限

- `Risk` 三档与 Astryx 完全一致：`read` / `write` / `exec`。
- `ToolStatus`：LFTTDO `running` / `ok` / `error` → Astryx `running` / `complete` / `error`。
- `target` 推导顺序：`input.path` → `input.command` → `input.pattern` → `input.query` → `input.url` → 工具名。
- `additions` / `deletions` 事件里没有；真实模式填 0，不伪造 diff。
- `tool_result.images` 追加为图片块；工具卡本身仍显示结果文本。

## 12. Astryx UI 改动清单

1. **入口选择**
   - 新增 `src/hooks/useAgent.js`，按 `isTauri()` 选择 `useMockAgent` 或 `useLfttdoAgent`。
   - `App.jsx` 只改 import 与少量启动/会话接线。

2. **真实适配器**
   - `src/agent/lfttdo/client.js`：命令封装、`listen("agent")`、错误归一化。
   - `src/agent/lfttdo/projection.js`：纯函数 `projectEvent(state, event, meta)`；可单测。
   - `src/hooks/useLfttdoAgent.js`：实现与 `useMockAgent` 相同的返回 API。

3. **启动与会话**
   - Tauri 模式加载真实 settings/sessions；增加 booting/空态/错误态。
   - 会话切换加载历史；事件按 sessionId 分发；避免重放覆盖流式。

4. **工具/权限/提问**
   - `tool` block 增加可选 `id`（`call_id`）。
   - 权限卡等待 `permission_resolved` 再变更。
   - 提问弹窗在 `question_respond` 成功后关闭；失败保留。

5. **输入坞**
   - 图片走 `normalize_image`。
   - 发送中禁用模型切换。
   - `@` 文件提及改用 `session_search_files`。

6. **状态栏**
   - 用真实 `usage.budget` 做上下文分母。

7. **设置/面板**
   - 设置字段映射；API Key 只走 `settings_set_api_key`。
   - 技能/MCP/Hooks/记忆接真实命令；未接入前不要展示 mock 数据冒充真实数据。

8. **工作区/导出**
   - 工作文件夹按会话；导出走原生保存对话框；文件卡走 `reveal_path`。

9. **主题**
   - 保留 `lfttdo-noir`；可选把 LFTTDO 的 `accent` / `font_size` 同步到 Astryx runtime theme。

## 13. 实施阶段

### Phase 0：方案（本分支当前内容）

- 新增本设计文档与实施计划。
- 不改代码，不推送 main。

### Phase 1：脚手架与 Tauri 外壳

- 复制 `Astryx测试1` 到 `astryx-ui/`（排除构建产物）。
- 增加根 scripts 与 Tauri 配置。
- `npm install`、`npm run build:astryx` 通过。
- `npm run tauri dev` 打开 Astryx UI；此时仍走 mock。
- 旧 `src/` 不修改；旧测试仍通过。

### Phase 2：真实文本与会话

- 实现 `client.js`、`projection.js`、`useLfttdoAgent` 骨架。
- 接入 `settings_get`、`session_list`、`session_messages`、`send_message`。
- 真实模型文本流、会话切换、启动加载、发送失败回滚。
- 浏览器 mock 仍可用。

### Phase 3：工具、权限、提问、停止、重试

- 工具卡、权限卡、提问弹窗接通事件。
- `respond` / `question_respond` / `interrupt` / `session_retry_last`。
- 多轮 `turn_start`、助手多气泡、`done` 收尾。
- 权限决定只由事件落地；提问返回值语义按 §10.4。

### Phase 4：设置、图片、工作区、面板

- 设置读写、API Key、模型切换、accent/font。
- 图片归一化与 `@` 文件提及。
- 工作文件夹、导出、文件定位。
- 技能/MCP/Hooks/记忆面板接真实命令。
- 未支持的 LFTTDO 字段保留 carry-through，不丢。

### Phase 5：验收与打包

- 单测、构建、旧前端回归、Rust 回归。
- Tauri 真机跑一轮完整流程。
- `npm run tauri build` 打包验证（可选，但建议至少跑一次）。

## 14. 测试与验收

### 14.1 自动化

- `astryx-ui` 新增 Vitest：
  - 投影：`turn_start` + `text_delta` + `tool_call` + `tool_result` + `done` 生成正确块。
  - 权限：`permission_request` → 卡；`permission_resolved` → 状态；未收到事件不改变。
  - 提问：`question` → 待答；`question_respond` true/false 分支；无 resolved 事件。
  - 重放：`user_message` + `turn_start` + `text_delta` 不重复、不串会话。
  - usage / error / context_compacted。
- 适配器单测：用 fake `invoke` / `listen`，验证命令参数逐字正确（camelCase 键名）。
- 构建检查：
  - `npm --prefix astryx-ui run build`
  - `npx astryx doctor --built astryx-ui/dist`

### 14.2 回归

```bash
# 旧前端不受影响
npx vitest run
npx tsc --noEmit
npm run lint:css

# Rust 无改动，但跑一次确认
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### 14.3 真机验收

在 Tauri 窗口里用真实模型端点（默认本机 Ollama `/v1`）逐项操作：

1. 发送一句话，看到流式文本。
2. 触发 `read_file`，工具卡 running → complete。
3. 触发写入/命令，权限卡出现；分别测试“仅本次”“本会话内都允许”“拒绝”，确认卡片只在 `permission_resolved` 后变化。
4. 触发 `question`，居中弹窗出现；选择/自定义回答后弹窗关闭，工具结果继续。
5. 停止：点停止，消息保留并标记已停止。
6. 重试：最后一轮被截断并重发。
7. 新建/切换/重命名/删除会话；切换后历史正确。
8. 设置改模型/预算/权限模式；下一条消息生效。
9. 图片粘贴 → 归一化 → 模型收到。
10. 技能/MCP/Hooks/记忆面板读真实数据（Phase 4）。

## 15. 风险与回退

| 风险 | 处理 |
|---|---|
| 非 ASCII 仓库路径导致 Rust 构建失败 | 使用 ASCII worktree `D:\aido\lfttdo-astryx`；必要时 `AWS_LC_SYS_PREBUILT_NASM=1` |
| Tauri 配置改动影响旧前端开发 | 只在分支改；旧 `src/` 保留；回退配置即可恢复 |
| 设置只提交局部字段导致丢配置 | 保留 `settings_get` 原始对象，展开后提交完整 Settings；加 round-trip 测试 |
| `turn_start` 多次导致多助手气泡 | 按 LFTTDO 语义处理；不要假设单气泡；重试/停止以最后一轮为准 |
| 非 active 会话事件丢失 | 按 `payload.sessionId` 分发到所有会话；未知会话刷新列表 |
| 历史重放覆盖实时流 | 流式中的会话不重放；`hydrated` 与 `streaming` 双重守卫 |
| 提问未回答不持久 | 明确边界；重开后不恢复未决提问；与原生前端一致 |
| 权限卡提前变绿 | 只由 `permission_resolved` 改状态；`respond` 返回值不作为状态依据 |
| 设置/技能/MCP 面板展示 mock 冒充真实 | Phase 4 前显示“未接入”或只读真实数据；不混用 |
| Astryx CSS 在 WebView2 下表现差异 | 在 Tauri 窗口实测；必要时只修 Astryx 侧 token |
| 两份 node_modules 体积 | 仅分支；构建产物不入库；可接受 |

## 16. 待确认

1. 验收用的模型端点：本机 Ollama `http://127.0.0.1:11434/v1`，还是指定端点？
2. Phase 4 是否必须一次做完全部面板，还是先设置 + 图片 + 工作区，技能/MCP/Hooks/记忆随后？
3. 旧 UI 是否需要保留一个可运行入口（例如 `tauri.legacy.conf.json`），还是分支只保证可回退？
