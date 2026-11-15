# LFTTDO Astryx 独立版本实施计划

- 日期：2026-11-15
- 分支：`feat/astryx-ui`
- 基线：`f412d7e`
- 版本形态：独立应用 `LFTTDO Astryx`，默认 LFTTDO 保持原样
- 设计文档：`docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md`
- 实施 worktree：`D:\aido\lfttdo-astryx`（ASCII 路径，避免 Rust NASM 构建受中文路径影响）

## 0. 当前状态

- 已克隆 LFTTDO 源码，`main` 干净，无 `node_modules` / `src-tauri/target`。
- 已创建分支 `feat/astryx-ui`，只包含本计划与设计文档。
- Astryx 原型在 `D:\aido\Astryx-界面制作\Astryx测试1`，当前为 mock。
- 本阶段不修改 `src/`、`src-tauri/`、`package.json`。

## 1. 目标拆解

按“先能跑、再能聊、再接工具、最后接面板”的顺序做，每一步都能单独验收：

1. **P1 外壳**：独立 `LFTTDO Astryx` 版本加载 Astryx UI，仍走 mock；默认 LFTTDO 不受影响。
2. **P2 真实文本**：真实模型流式、会话列表、历史加载。
3. **P3 真实工具**：工具卡、权限卡、提问弹窗、停止、重试。
4. **P4 设置与产物**：设置、图片、工作区、导出、文件定位。
5. **P5 面板与交付**：技能/MCP/Hooks/记忆、测试、打包。

每一步完成后单独提交，不做“大爆炸合并”。

## 2. P1：脚手架与独立 Tauri 版本

### 任务

1. 复制 Astryx 原型到 `astryx-ui/`：
   - 保留 `src/`、`index.html`、`vite.config.js`、`package.json`、`package-lock.json`、`astryx-themes.js`、`themes/`、`src/themes/`。
   - 排除 `node_modules/`、`dist/`、`artifacts/`、`.zcode/`、`使用说明.md`。
2. 修改 `astryx-ui/package.json`：
   - `name` 改为 `lfttdo-astryx-ui`。
   - 增加 `@tauri-apps/api`（v2）。
   - 增加 `vitest` 与 `test` script（P2 开始使用）。
3. 修改 `astryx-ui/vite.config.js`：
   - `server.port = 5173`，`strictPort = true`。
   - 保留 `@vitejs/plugin-react`。
4. 根 `package.json` 增加：
   ```json
   "dev:astryx": "npm --prefix astryx-ui run dev",
   "build:astryx": "npm --prefix astryx-ui run build",
   "test:astryx": "npm --prefix astryx-ui run test",
   "tauri:astryx": "tauri dev --config src-tauri/tauri.astryx.conf.json",
   "tauri:astryx:build": "tauri build --config src-tauri/tauri.astryx.conf.json"
   ```
5. **不改** `src-tauri/tauri.conf.json`；新增 `src-tauri/tauri.astryx.conf.json`：
   ```json
   {
     "productName": "LFTTDO Astryx",
     "identifier": "com.qwa.do.astryx",
     "build": {
       "beforeDevCommand": "npm run dev:astryx",
       "devUrl": "http://localhost:5173",
       "beforeBuildCommand": "npm run build:astryx",
       "frontendDist": "../astryx-ui/dist"
     },
     "app": {
       "windows": [
         { "title": "LFTTDO Astryx" }
       ]
     }
   }
   ```
6. 检查根 `.gitignore` 覆盖 `astryx-ui/node_modules` 与 `astryx-ui/dist`；没有就补。

### 验收

```bash
cd astryx-ui && npm install && npm run build
cd ..
npm run tauri:astryx        # 打开 LFTTDO Astryx
npm run tauri dev            # 打开原 LFTTDO
```

- `LFTTDO Astryx` 窗口显示 Astryx UI；默认 LFTTDO 窗口仍显示原 UI。
- Astryx 版仍走 mock 流式，不调用真实 agent。
- 旧 `src/` 与 `tauri.conf.json` 未修改。
- 普通浏览器 `http://localhost:5173/` 仍能打开。

### 风险

- Tauri `--config` 的合并顺序要在 P1 实测；若 `tauri dev --config` 不接受，改成 `tauri --config src-tauri/tauri.astryx.conf.json dev`。
- 独立 productName / identifier 是否能让两个安装器并存，P5 实测；必要时设置独立 `mainBinaryName`。
- Tauri `beforeDevCommand` 在 Windows 下对 `npm --prefix` 的解析要实测；若失败，改成 `node scripts/astryx-dev.mjs` 启动子进程。
- 端口 5173 被占用时 strictPort 会直接失败，先关掉旧的 Vite。

## 3. P2：真实文本与会话

### 新增文件

```text
astryx-ui/src/agent/lfttdo/client.js
astryx-ui/src/agent/lfttdo/projection.js
astryx-ui/src/hooks/useLfttdoAgent.js
astryx-ui/src/hooks/useAgent.js
```

### 实现

1. `client.js`
   - `invoke` / `listen` 的薄封装。
   - `subscribeAgent(onPayload)` 订阅 `agent` 通道，返回取消函数。
   - `sendMessage(sessionId, blocks)`、`interrupt()`、`respond()`、`questionRespond()`、`sessionList()`、`sessionMessages()`、`settingsGet()` 等命令函数。
   - 参数键严格用 Tauri 文档的 camelCase：`sessionId`、`requestId`、`sessionId`、`apiKey`、`dueAt`。
2. `projection.js`
   - 纯函数，输入 `{ sessions, activeId, ... }` 与 `AgentEvent`，输出新的消息投影。
   - 实现设计文档 §9 的全部已支持事件。
   - `tool` 块带可选 `id = call.id`，用于 `tool_result` 归位。
   - `turn_start` 新建助手消息；`text_delta` 合并到该消息最后一个 text 块；`tool_call` 追加工具块；`tool_result` 按 id 更新；`permission_*` 按 id 更新；`question` 追加待答块；`usage` 更新用量；`error`/`done` 收尾。
   - 同一函数同时服务实时事件与 `session_messages` 重放；重放时 `isStreaming` 只在最后一个 `turn_start` 到 `done` 之间为真。
3. `useLfttdoAgent`
   - 与 `useMockAgent` 相同的返回 API。
   - 启动时 `settings_get` + `session_list`；空库 `session_create`；选中会话 `session_messages`。
   - 发送：乐观追加用户消息 → `sendMessage`；失败回滚；`turn_start` 创建助手消息。
   - 事件按 `payload.sessionId` 分发到对应会话；active 与非 active 都更新。
   - `isStreaming` / `streamingSessionId` 由事件驱动。
   - 订阅只建立一次，卸载时取消。
4. `useAgent`
   - 模块级判断 `isTauri()`：Tauri 用 `useLfttdoAgent`，浏览器用 `useMockAgent`。
   - 不在渲染中条件调用 Hook。

### App 接线

- `App.jsx` 的 `useMockAgent` import 改为 `useAgent`。
- Tauri 模式下：
  - 初始 `sessions` / `settings` 改为空/默认；增加 `booting` 状态。
  - `useEffect` 启动加载；加载完成后渲染正常界面。
  - 加载失败显示错误 + 重试。
- 浏览器 mock 路径保持现有初始化，不改行为。

### 验收

- Tauri 窗口发送“你好”，真实模型文本流式出现。
- 历史会话列表来自 SQLite；切换会话能加载历史。
- 新建会话后能发消息；标题在首轮后刷新。
- 发送失败时用户消息回滚，Toast 显示原因。
- 浏览器 5173 仍是 mock，不报错。

## 4. P3：工具、权限、提问、停止、重试

### 实现

1. 工具卡
   - `tool_call` → `tool` 块，状态 running。
   - `tool_result` → 按 `call.id` 更新为 complete/error；填 `duration=ms`、`result=preview`、`errorMessage=preview`（is_error）。
   - `images` 追加为图片块。
2. 权限卡
   - `permission_request` → `permission` 块，`decision:null`。
   - `respondPermission(id, decision)` 只调 `respond`；不本地改卡。
   - `permission_resolved` 事件更新卡片；返回 false/reject 时保留待决并 Toast。
3. 提问
   - `question` → `question` 块；`QuestionDialog` 自动打开。
   - `answerQuestion(id, answer)` 调 `question_respond`；true 时标记 answer、追加本地用户回答、关闭弹窗；false 时 Toast 并关闭；reject 保留。
4. 停止
   - `stop()` 调 `interrupt()`；收到 `done{stop_reason:"interrupted"}` 后标记 stopped。
5. 重试
   - `retryLast(sessionId)` 调 `session_retry_last`；null Toast；有 blocks 则移除最后一轮助手并重发。
6. 多气泡
   - 每次 `turn_start` 新建助手消息；`isLast`、重试按钮、停止标记都以最后一条助手消息为准。

### 验收

- 只读工具：卡从 running 到 complete。
- 写入/命令：权限卡出现；三种决定都能改变卡片；拒绝后模型收到拒绝并继续/收尾。
- 提问：弹窗出现；选择后关闭，工具结果继续。
- 停止：内容保留并标记“已停止”。
- 重试：最后一轮被截断并重新流式。
- 权限卡不会在事件到达前变绿。

## 5. P4：设置、图片、工作区、导出

### 设置

- 映射字段见设计文档 §11.2。
- 保留 `settings_get` 原始对象；保存时展开并覆盖变更字段，提交完整 `Settings`。
- API Key 只调 `settings_set_api_key`；显示 `api_key_masked`。
- 模型切换写 `model`；流式中禁用。
- 可选：把 `accent` / `font_size` 同步到 Astryx runtime theme。

### 图片

- 粘贴/拖拽 → `File.arrayBuffer()` → `normalize_image` → image block → `send_message`。
- 不在前端做第二套大小/格式判断。

### 工作区与产物

- 工作文件夹按会话：`session_workspace_set`、`session_output_view`。
- 文件卡点击 `reveal_path(sessionId, path)`。
- 导出 `session_export(sessionId)`，显示返回路径。
- `@` 文件提及改 `session_search_files(sessionId, query)`。

### 验收

- 设置改模型/预算/权限模式/字号/强调色；下一条消息生效。
- API Key 保存/清除；掩码显示正确。
- 粘贴图片能发送并被模型收到。
- 文件卡能定位；导出弹原生保存框。
- 工作文件夹按会话切换，不串会话。

## 6. P5：面板、测试与交付

### 面板

按 LFTTDO 命令接真实数据：

- 技能：`skills_list`、`skills_set_enabled`、`skills_open_folder`。
- MCP：`mcp_servers`、`mcp_set_trust`、`mcp_reconnect`、`mcp_disconnect`、`mcp_config_status/read/save`。
- Hooks：`hooks_list`、`hooks_set_trusted`、`hooks_set_workspace_enabled`、`hooks_open_folder`。
- 记忆：`memories_list`、`memories_add`、`memories_delete`、`memories_clear`、`memory_update_body`、`memory_profile`。

未接入的面板不要继续展示 mock 数据；显示“当前分支尚未接入”或只读真实数据。

### 测试

```bash
# Astryx UI
cd astryx-ui
npm run build
npm test

# Astryx 规则检查
cd ..
npx astryx doctor --built astryx-ui/dist

# 旧前端回归
npx vitest run
npx tsc --noEmit
npm run lint:css

# Rust 回归（本方案不改 Rust，仍要确认）
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### 真机验收

按设计文档 §14.3 的 10 条逐项执行，记录实际结果；失败项不隐藏。

### 打包

```bash
npm run tauri:astryx:build
```

确认能产出独立的 `LFTTDO Astryx` NSIS 安装包；默认 LFTTDO 的 `npm run tauri build` 仍应可用。若因环境失败，记录原因。

## 7. 提交与分支纪律

- 每个 Phase 一个提交，提交信息中文，说明背景/行为/验证。
- 不改 `main`，不合并；分支保留可回退。
- 推送按 `AGENTS.md` 使用 GitHub Git Data API，不用直接 `git push`。
- 推送后验证：
  - 远端 commit message 与本地一致。
  - 新文件在 tree 中。
  - blob sha 与本地 `git rev-parse HEAD:<path>` 一致。
- 分支：`feat/astryx-ui`。

## 8. 回退

- P1 后要移除 Astryx 版：删除 `astryx-ui/`、`src-tauri/tauri.astryx.conf.json` 与根 scripts；默认 `tauri.conf.json` 从未修改，原版不受影响。
- P2+ 出问题：保留 `astryx-ui/`，把 `useAgent` 强制切回 mock，Astryx 外壳仍可打开 UI；默认 LFTTDO 始终可用。
- Rust 未改动，无需数据库迁移；独立数据目录在 `%APPDATA%\com.qwa.do.astryx\`，删除它即可清理 Astryx 版数据。

## 9. 阻塞条件

出现以下情况先停，不硬改：

- 需要新增/修改 Tauri 命令才能完成当前 Phase。
- `settings_set` 的全量写回会丢字段且无法用现有命令避免。
- 非 active 会话事件无法按 sessionId 分发。
- 权限/提问语义与 LFTTDO 现有前端冲突。
- Rust 构建因环境失败且短期无法解决。

## 10. 第一批可执行命令（P1）

```bash
cd /d D:\aido\lfttdo-astryx
# 复制 Astryx 原型（示例，实际排除 node_modules/dist/artifacts）
xcopy "D:\aido\Astryx-界面制作\Astryx测试1" "astryx-ui" /E /I /EXCLUDE:exclude.txt

cd astryx-ui
npm install
npm run build

cd ..
npm run tauri:astryx
# 可选：确认默认版本仍能启动
npm run tauri dev
```

（`exclude.txt` 在实施时生成，列出 `node_modules`、`dist`、`artifacts`、`.zcode`。）
