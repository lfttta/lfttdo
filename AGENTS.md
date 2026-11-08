# AGENTS.md — 在本仓库工作的 AI 代理契约

本文件给所有在本仓库干活的 AI 代理（ZCode 会话、子代理）读。人也可以读。内容都是实测过的约束，不是风格建议。

## 项目一句话

LFTTDO：Tauri v2 + React 19 + Rust 的 Windows 桌面 AI 助理。前端 `src/`，Rust 核心 `src-tauri/src/`。

## 架构红线（改代码前先读）

1. **只有 `src-tauri/src/ipc.rs` 允许 `use tauri`**（Plan A 约束，见 `agent/mod.rs` 头注释）。`agent/` 子树、`tools/`、`store/` 禁止依赖 Tauri——这是核心逻辑能用假模型单测的前提。
2. **持久化真相是 SQLite 的 `messages` 表**；前端看到的事件流是投影（`store/db.rs` 的 `events_from_messages` 重建 `AgentEvent`）。不要引入第二份真相。
3. **事件线上形状是契约**：`AgentEvent` 改变体/字段，必须同步 `src/state/reducer.ts`（编译期穷尽守卫会兜底）和 `src/types.ts`。`Decision` 的序列化是手写的（`agent/events.rs` 头注释有极性翻转事故记录），别动它。
4. **状态只有一个所有者**：跨 IPC 的会话状态在 Rust 侧，前端 reducer 只做事件折叠。不要在 React 里建平行会话状态。
5. 权限判定只在 `permission/gate.rs`（Rust），前端只渲染徽章，永远不自行判定风险。

## 规矩

- **spec 先行**：改行为前先在 `docs/superpowers/specs/` 放设计文档（日期命名）；做完把过程写进 `docs/superpowers/plans/` 或 `docs/findings/`。对照外部框架（ZCode/OpenCode）的演进计划在 `docs/superpowers/plans/2026-11-08-zcode-alignment.md`。
- **注释与提交信息用中文**。注释写「为什么」，不写「是什么」。修 bug 必须留注释：触发场景、根因、修复依据。
- **行为用测试钉住**：线上形状逐字断言（先看 `agent/summarize.rs` 的测试怎么写的）；新功能必须带测试；不许为了过测试改断言。
- **发现设计缺陷先停下来说**，不堆叠兜底分支。

## 验证（提交前必须全绿，如实报告）

```
cd src-tauri
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cd ..
npx vitest run
```

基线：Rust 966 lib + 3 集成、前端 328（2026-11-08，#70 合并后）。测试数会涨，不许跌。

## 并行会话纪律（发生过真实事故）

本仓库可能同时有**两个 ZCode 会话**在干活。规则：

1. 动共享大文件（`ipc.rs`、`App.tsx`、`settings.rs`、`styles.css`）前先 `git status` 看工作区有没有别人的未提交改动；有就先跟用户确认分工。
2. **禁止** `git checkout -- .` / `git clean` 清场——那是把别人的活删掉。本地清场放到工作流最末尾且用户确认后。
3. Edit 报 `modified since read` 就重读再改，不要硬写。
4. 长期状态（计划、进度）写进 `docs/`，不留在对话里——对话会关，文件不会。

## Git 与网络（本机约束）

- `git push` 走镜像会**挂起**：推送用 GitHub Git Data API（gh CLI 已登录）：blobs → tree(base_tree) → commit → PATCH `refs/heads/main`。
- clone/fetch 走 `https://ghfast.top/https://github.com/...` 镜像（origin 已指向它）。
- 合并方式是 **squash**；外部贡献来自 fork，不能直接推其分支。
- 提交信息：中文，说背景、设计意图、改前改后行为；bugfix 附触发场景/根因/回归验证。
