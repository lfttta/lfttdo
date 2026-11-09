// 装配根：只做模块声明与一行转发。
//
// Plan A 的边界要求核心层（`agent/`、`model/`、`permission/`、`settings`、`store/`、
// `tools/`）一概不依赖 Tauri，而 T13 的硬约束是「`src/` 下只有 `ipc.rs` 允许 `use tauri`」。
// 所以 `tauri::Builder`、`State`、`Manager`、`generate_handler!` 全部住在 `ipc::run()` 里，
// 本文件一个 tauri 导入都没有 —— `grep -n "use tauri" src/` 只应命中 `ipc.rs`。
// 下面那行 `cfg_attr(mobile, …)` 是模板给的入口属性，桌面构建里 `cfg(mobile)` 为假、
// 整条被剥掉，不会解析 `tauri` 路径。

pub mod agent;
pub mod external_link;
/// G45+：headless 会话入口（`--headless`）。不开窗口跑一轮，事件流打成 NDJSON。
/// 与 Tauri 无关；评测（`scripts/eval`）靠它做真实的会话断言。
/// 例外说明：本模块引用 `crate::ipc` 里的**帮助函数**（`build_production_registry` /
/// `agent_profiles`），这是「复用同一条装配语义」而不是引入 tauri 类型 ——
/// 那几个函数的签名里一个 tauri 类型都没有。
pub mod headless;
/// G44：用户 hook（事件脚本 + digest 信任）。与 Tauri 无关，纯逻辑可单测。
pub mod hooks;
pub mod image_cache;
pub mod image_prep;
pub mod ipc;
pub mod lock;
pub mod mcp;
pub mod model;
pub mod path_reveal;
pub mod permission;
pub mod settings;
pub mod skills;
pub mod store;
pub mod text;
pub mod tools;
pub mod web_guard;
pub mod window_state;
pub mod workspace;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ipc::run()
}
