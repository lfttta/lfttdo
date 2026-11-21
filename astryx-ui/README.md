# LFTTDO 前端（astryx-ui）

LFTTDO 的默认 React + Astryx 前端（2026-11-21 转正，替代 `src/` 旧前端，旧前端保留未删）。Rust Agent 内核、Tauri 命令与 SQLite 仍来自仓库根目录的 `src-tauri/`。

## 开发

```bash
npm run tauri dev
```

只调前端（不启动 Tauri）时：

```bash
npm --prefix astryx-ui run dev
```

开发服务器固定 `http://localhost:1420`。Rust 的 `src-tauri/src/web_guard.rs` 只允许 1420 的开发导航，换成其他端口会得到空白窗口。

## 构建

```bash
npm run tauri build            # 默认安装包（本前端）
npm --prefix astryx-ui run build
```

## 回退与遗留入口

- **回退旧前端**：revert 转正提交（只动了 `src-tauri/tauri.conf.json` 三行）即可恢复；`src/` 旧前端与根 scripts 均保留原样，`npm run dev` / `npm run build` 仍指向旧前端。
- **遗留独立版入口** `npm run tauri:astryx`（overlay `src-tauri/tauri.astryx.conf.json`）仍可用：同一前端、独立数据目录 `%APPDATA%\com.qwa.do.astryx\`，适合隔离试验；窗口标题不再单独区分。

## 边界

- Tauri 模式已接真实内核：文本流与会话、设置持久化、技能/MCP/Hooks/记忆面板、图片、按会话工作文件夹、导出、文件定位与 @ 文件提及均已接入。
- 浏览器（非 Tauri）模式仍是纯 mock，仅供不启动 Tauri 调 UI。
- API Key 凭据（keyring 服务固定 `api_key.qwen-do`）是系统级、按用户存储，与数据目录无关：`src/` 旧前端、本前端、遗留独立版读写的都是同一个凭据。
- UI 数据契约见 `docs/agent-ui-contract.md`；整体设计见仓库根目录 `docs/superpowers/specs/2026-11-15-astryx-ui-integration-design.md`。
