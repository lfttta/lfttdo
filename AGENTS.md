# AGENTS.md — 在本仓库工作的 AI 代理契约

本文件给所有在本仓库干活的 AI 代理（ZCode 会话、子代理）读。人也可以读。内容都是实测过的约束，不是风格建议。

## 项目一句话

LFTTDO：Tauri v2 + React 19 + Rust 的 Windows 桌面 AI 助理。前端 `src/`，Rust 核心 `src-tauri/src/`。

## 架构红线（改代码前先读）

1. **只有 `src-tauri/src/ipc.rs` 允许 `use tauri`**（Plan A 约束，见 `agent/mod.rs` 头注释）。`agent/` 子树、`tools/`、`store/`、`hooks.rs`、`headless.rs` 禁止依赖 Tauri——这是核心逻辑能用假模型单测的前提。
   - `headless.rs` 是**唯一例外说明**：它引用 `ipc.rs` 里的帮助函数（`build_production_registry` / `agent_profiles`），但那些函数的签名里一个 tauri 类型都没有。目的是「复用同一条装配语义」，不是绕过约束。
   - `ipc.rs` 现在是 **3985 行装配 + `ipc/tests.rs` 6518 行测试**（2026-11-09 拆分）。加命令写 `ipc.rs`，加测试写 `ipc/tests.rs`；子模块对父模块私有项天然可见，`use super::*` 不用改。
2. **持久化真相是 SQLite 的 `messages` 表**；前端看到的事件流是投影（`store/db.rs` 的 `events_from_messages` 重建 `AgentEvent`）。不要引入第二份真相。
3. **事件线上形状是契约**：`AgentEvent` 改变体/字段，必须同步 `src/state/reducer.ts`（编译期穷尽守卫会兜底）和 `src/types.ts`。`Decision` 的序列化是手写的（`agent/events.rs` 头注释有极性翻转事故记录），别动它。
4. **状态只有一个所有者**：跨 IPC 的会话状态在 Rust 侧，前端 reducer 只做事件折叠。不要在 React 里建平行会话状态。
5. 权限判定只在 `permission/gate.rs`（Rust），前端只渲染徽章，永远不自行判定风险。
6. **hook 是任意代码执行，信任绑定声明内容的 SHA-256**：`hooks.rs` 的 `TrustStore` 是唯一防线。动 hook 相关代码时不许放宽这几点——信任键必须含作用域前缀（工作区来源 `ws:<路径摘要>:`）、未信任一律不执行、信任表损坏按空表（fail closed）。
7. **`updated_input` 改写后，权限门必须按改写后的入参判定**（`session.rs` 重建 `ToolCallView` 的那一处）。拿原入参判定等于给 hook 一张绕过权限门的通行证——这是 hook 改写功能唯一不可让步的语义。

## 规矩

- **spec 先行**：改行为前先在 `docs/superpowers/specs/` 放设计文档（日期命名）；做完把过程写进 `docs/superpowers/plans/` 或 `docs/findings/`。对照外部框架（ZCode/OpenCode）的演进计划在 `docs/superpowers/plans/2026-11-08-zcode-alignment.md`。
- **注释与提交信息用中文**。注释写「为什么」，不写「是什么」。修 bug 必须留注释：触发场景、根因、修复依据。
- **行为用测试钉住**：线上形状逐字断言（先看 `agent/summarize.rs` 的测试怎么写的）；新功能必须带测试；不许为了过测试改断言。
- **发现设计缺陷先停下来说**，不堆叠兜底分支。
- **低风险任务不要拆碎问**：实现细节（命名、文件放哪、用哪个既有函数）自己取合理默认；只有架构、公共 API、数据模型、依赖、不可逆操作这几类才问。
- **不许留只为压警告而存在的空壳**：`#[allow(dead_code)] fn _marker()` 这种写法会把下一个人骗去看一个没有功能的函数。要么真用上，要么删掉。

## 会话级回归评测（`scripts/eval/`）

不进 CI（CI 无模型、无网络），是发版/大改时手动跑一遍的回归门。

```bash
node scripts/eval/run.mjs --dry        # 看将跑哪些用例（含端点可达性）
node scripts/eval/run.mjs              # 需要真实端点（默认本机 Ollama 127.0.0.1:11434）
node scripts/eval/run.mjs --boot-only  # 只验启动层，不需要模型
```

会话层用 `lfttdo --headless` 真跑一轮，对 stdout 的 NDJSON 事件流断言工具序列 / `stop_reason` / 产物 / 关键词。**端点不可达时会话层明确失败（exit 1），不静默跳过**——「全绿」如果其实一条会话断言都没跑，是最坏的假信号。加用例改 `scripts/eval/cases.json`（字段见 `scripts/eval/README.md`）。

改 `headless.rs` 时注意：**不带 `--headless` 的调用必须行为完全不变**（GUI 仍是默认），`wants_headless` 是唯一判定点。

## 验证（提交前必须全绿，如实报告）

```
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cd ..
npx vitest run
npx tsc --noEmit
npm run lint:css      # 注意它**排除** theme/tokens.css 与 theme/presets.css（那两处是设计令牌定义）
```

基线：**Rust 1016 lib + 3 集成、前端 354**（2026-11-09，`8a4df9e`）。测试数会涨，不许跌。

> **CI 的 clippy 在 ubuntu 上跑**（本机是 Windows）。`#[cfg(windows)]` 代码引用的**任何项**
> ——常量、测试替身结构体、它们的 `impl`——都必须自己挂 `#[cfg(windows)]`，
> 否则 Linux 侧报 `never constructed` / `never used`，`-D warnings` 直接失败。
> 这个坑踩过两次（`ipc.rs` 的 DWM 常量、`session.rs` 的 RecordingTool 系列）。
> 本机 `cargo clippy` 全绿**不代表** CI 会绿——提交前想一下「这段代码在 Linux 上还会被用到吗」。

> **`cargo test --all-targets` 会因 lfttdo.exe 在跑而失败**（`os error 5: failed to remove`）：
> 先 `taskkill /IM lfttdo.exe /F`。

## 并行会话纪律（发生过真实事故）

本仓库可能同时有**两个 ZCode 会话**在干活。规则：

1. 动共享大文件（`ipc.rs`、`App.tsx`、`settings.rs`、`styles.css`）前先 `git status` 看工作区有没有别人的未提交改动；有就先跟用户确认分工。
2. **禁止** `git checkout -- .` / `git clean` 清场——那是把别人的活删掉。本地清场放到工作流最末尾且用户确认后。
3. Edit 报 `modified since read` 就重读再改，不要硬写。
4. 长期状态（计划、进度）写进 `docs/`，不留在对话里——对话会关，文件不会。

## Git 与网络（本机约束）

- `git push` 走镜像会**挂起**：推送用 GitHub Git Data API（gh CLI 已登录）：blobs → tree(base_tree) → commit → PATCH `refs/heads/main`。
- **推送脚本必须用 git blob sha，不能读工作区字节**（本仓库 `core.autocrlf=true`：工作区 CRLF、仓库 LF）。
  正确做法：`git rev-parse HEAD:<path>` 取 sha，远端已有该 blob 就直接引用，没有才用
  `git cat-file` 取字节上传。曾经读工作区字节把 `main.rs` 与 eval 种子推成了 CRLF，
  同目录其它 `.rs` 是 LF——功能无差别，但一个目录混两种行尾是真实的卫生问题。

### 编码纪律（踩过一次最严重的事故：12 条 commit 消息被毁）

PowerShell 5.1 的 `$OutputEncoding` 默认是 **US-ASCII (cp20127)**。把含中文的字符串
**管道给原生 exe**（`$json | gh api --input -`）时，每个非 ASCII 字符都变成 `?`。
后果：本地 commit 消息完好，**推上去的全成了 `docs: AGENTS.md ? hooks ??????????`**。
此类损坏**不可逆** —— 远端对象里只剩 0x3f 字节，原文字节已不在。

所以：

1. **绝不把 JSON 管道给 `gh`**。写 UTF-8 文件，用 `--input <file>`：
   ```powershell
   [IO.File]::WriteAllText($f, $json, (New-Object Text.UTF8Encoding $false))
   gh api ... --method POST --input $f
   ```
2. **`ConvertTo-Json` 在 PS 5.1 下也会毁中文**（实测 `消息` → `?`）。要构造含中文的
   JSON，用手写 `JsonEscape`（见下表），或干脆用 `git log -1 --format=%B` 读原文再逐字符转义。
3. **`cmd /c "... > file"` 重定向会多出 / 吃掉行尾字节**：拿 blob 用
   `git cat-file -p <rev>` 重定向后用 `git cat-file -s <rev>` 校验长度。
4. **`.ps1` 脚本里不要写中文**。PowerShell 5.1 按 GBK 读脚本文件，UTF-8 中文注释会
   把字符串解析打断（表现为「表达式或语句中包含意外的标记」）。中文只放**数据文件**。
5. **推送后必查三件事**（缺一不可）：
   - `gh api repos/<r>/git/commits/<sha>` 的 `message` 与本地 `git log -1 --format=%B` 一致
   - 远端 blob sha == 本地 `git rev-parse HEAD:<path>`
   - 关键新文件在 `git/trees/<tree>` 里

> 修复这类损坏的代价很高（要重建整条链 + force push）。**宁可多写一个文件，也不要管道给 gh。**
- **推送后必须验证远端内容**，不能只看 PATCH 返回成功：
  - 关键新文件在不在：`gh api repos/<r>/contents/<path>` 逐条查（`?ref=main` 在 PowerShell 里
    会被引号吃掉，用 `--jq .sha` 或分变量拼 URL）
  - 行尾没被污染：比对远端 blob sha 与本地 `git rev-parse HEAD:<path>`
  - 曾出现「幻影提交」：commit 消息是旧的、功能文件缺失，PATCH 却返回成功
- clone/fetch 走 `https://ghfast.top/https://github.com/...` 镜像（origin 已指向它）。
- 合并方式是 **squash**；外部贡献来自 fork，不能直接推其分支。
- 提交信息：中文，说背景、设计意图、改前改后行为；bugfix 附触发场景/根因/回归验证。
- **`gh` 命令一律带 `-R lfttta/lfttdo`**：origin 是镜像 URL，gh 认不出它是 GitHub 仓库。
- 看真实改动规模用 `git diff --ignore-cr-at-eol --stat`（`core.autocrlf` 会让整文件看起来全变了）。
