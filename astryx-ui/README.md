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
- 当前 P1 仍使用 mock agent；真实 IPC 适配在后续阶段接入。
- UI 数据契约见 `docs/agent-ui-contract.md`；整体设计见仓库根目录 `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md`。
