# LFTTDO Astryx 前端

`LFTTDO Astryx` 独立版本的 React + Astryx 前端。Rust Agent 内核、Tauri 命令与 SQLite 仍来自仓库根目录的 `src-tauri/`。

## 开发

从仓库根目录启动独立版本：

```bash
npm run tauri:astryx
```

只调前端（不启动 Tauri）时：

```bash
npm --prefix astryx-ui run dev
```

开发服务器固定 `http://localhost:1420`。Rust 的 `src-tauri/src/web_guard.rs` 只允许 1420 的开发导航，换成其他端口会得到空白窗口。

## 构建

```bash
npm run tauri:astryx:build   # 独立版本安装包
npm --prefix astryx-ui run build
```

## 边界

- 默认 LFTTDO 与 Astryx 版共用 1420 开发端口，一次只能跑一个；构建产物、产品标识与数据目录彼此独立。
- Astryx 版使用独立数据目录 `%APPDATA%\com.qwa.do.astryx\`。
- Tauri 模式已接真实内核：文本流与会话、设置持久化、技能/MCP/Hooks/记忆面板、图片、按会话工作文件夹、导出、文件定位与 @ 文件提及均已接入。
- 浏览器（非 Tauri）模式仍是纯 mock，仅供不启动 Tauri 调 UI。
- API Key 凭据（qwen-do）与默认 LFTTDO 共用；在任一版本保存或清除都会同时影响两版。
- UI 数据契约见 `docs/agent-ui-contract.md`；整体设计见仓库根目录 `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md`。
