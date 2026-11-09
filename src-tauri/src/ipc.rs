// Task 13：Tauri 装配与 IPC 层。
//
// 本文件是 Plan A 里唯一允许 `use tauri` 的文件（`lib.rs` 只剩模块声明与一行转发）。
// 合帧逻辑与整条一轮对话的路径都**不依赖 tauri**（R106：出口抽成 `Publish`，
// 一轮抽成 `run_turn`），因为 `AppHandle` 在测试里构造不出来 —— 不抽这两层，
// 这段每次发送都要跑几百次的代码在生产里就永远没有测试。
use crate::agent::context::{ContextBuilder, ContextConfig};
use crate::agent::events::{AgentEvent, Decision, ErrorWhere, EventSink, StopReason};
use crate::agent::memory::{
    MemoryCandidate, MemoryDecision, PROFILE_CHAR_CAP, consolidate_memory, extract_memories,
    merge_profile,
};
use crate::agent::session::{AgentProfile, AgentSession, SessionConfig, SessionDeps};
use crate::agent::summarize::{
    SUMMARY_OMITTED, collect_summary, collect_title, sanitize_title, summary_note, summary_request,
    title_request,
};
use crate::agent::types::{ContentBlock, Message, Role};
use crate::mcp::bridge::{McpBridge, McpTool, ServerStatus};
use crate::mcp::config::ServerDef;
use crate::mcp::editor;
use crate::model::embed::{EmbedEndpoint, EmbedProvider};
use crate::model::openai_compat::{OpenAiCompat, is_loopback_endpoint};
use crate::model::provider::ModelProvider;
use crate::permission::gate::{PermissionGate, Verdict};
use crate::settings::{Settings, SettingsStore, effective_roots};
use crate::store::db::{Db, SessionRow, events_from_messages};
use crate::store::memory::{MemoryRow, MemoryWrite, RecallCfg};
use crate::tools::edit_file::EditFileTool;
use crate::tools::experiment::ExperimentTool;
use crate::tools::fetch::FetchTool;
use crate::tools::glob::GlobTool;
use crate::tools::grep::GrepTool;
use crate::tools::memory_search::MemorySearchTool;
use crate::tools::question::QuestionTool;
use crate::tools::read_file::ReadFileTool;
use crate::tools::registry::ToolRegistry;
use crate::tools::render_html::RenderHtmlTool;
use crate::tools::shell::{
    ShellCatalog, ShellProfile, ShellTool, detected_presets, ensure_executable, path_search_dirs,
    validate_custom,
};
use crate::tools::task::TaskTool;
use crate::tools::todo::TodoTool;
use crate::tools::undo::UndoTool;
use crate::tools::use_skill::UseSkillTool;
use crate::tools::websearch::WebSearchTool;
use crate::tools::workflow::WorkflowTool;
use crate::tools::write_file::WriteFileTool;
use crate::window_state::{self, Ev, WindowGeom, should_persist};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::ipc::InvokeBody;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl,
    WebviewWindowBuilder, WindowEvent,
};
use tokio_util::sync::CancellationToken;

/// 事件通道的 payload（plan:2842 / spec:285 钉死 `{ sessionId, event }`）。
/// 只有这一个结构体用 camelCase：`AgentEvent` 内部的键仍是 snake_case，
/// 前端的 `AgentPayload` 镜像（plan:3354）读的就是 `sessionId` 这个键名。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPayload {
    pub session_id: String,
    pub event: AgentEvent,
}

/// 合帧的唯一出口（R106 要求的 seam）。生产实现持 `AppHandle`，测试实现记账。
pub trait Publish: Send + Sync {
    fn publish(&self, payload: AgentPayload);
}

/// `TextDelta` 的合帧缓冲。两个上限（30ms / 累计 512 字符）**都只在下一个 delta 到达时**
/// 由 `push` 检查一次 ⇒ 这里没有定时器，谁也别指望「时间到了自己会发一帧」。
/// 只负责「什么时候可以交出去」，交出之后发哪儿去归 `Publish`。
struct Coalescer {
    buf: Mutex<String>,
    last_flush: Mutex<Instant>,
}

impl Coalescer {
    /// 距上次 flush 的时间上限（30ms）。**它自己不会发帧**：`push` 在下一次被调用时拿
    /// `elapsed()` 与它比，比过了才当场交出整帧（实测由
    /// `the_30ms_threshold_releases_the_frame_on_the_next_delta` 钉住）。
    /// 所以「保证前端不觉得卡住」这句话的真相是：流还在动的时候最迟下一个 delta 就落地；
    /// 流的最后一帧没有下一个 delta，靠收尾的 `flush()` 兜底。
    const MAX_WAIT: Duration = Duration::from_millis(30);
    /// 累计字符上限：命中即发，不等 30ms。长段回复时这是 IPC 次数的主要来源。
    const MAX_CHARS: usize = 512;

    fn new() -> Self {
        Self {
            buf: Mutex::new(String::new()),
            // 构造即把 `last_flush` 拨到「现在」，计时从这一刻起算。诚实的说法：
            // 30ms 之内到达的第一个 delta **不会**因为时间命中而出帧（`elapsed() < MAX_WAIT`），
            // 它要么攒够 `MAX_CHARS`，要么等之后的某个 delta，要么由收尾的 `flush()` 带走。
            // 这里没有定时器，所以不存在「最多等 30ms 也能主动出去」那件事（F3/m3 改的就是这句）。
            last_flush: Mutex::new(Instant::now()),
        }
    }

    /// 时间到 / 字数到 → 交出整帧；否则留在缓冲里给 `None`。
    fn push(&self, text: &str) -> Option<String> {
        {
            let mut b = self.buf();
            b.push_str(text);
        }
        let due = self.last_flush().elapsed() >= Self::MAX_WAIT;
        let big = self.buf().chars().count() >= Self::MAX_CHARS;
        if due || big { self.take() } else { None }
    }

    /// 无条件取空缓冲。空缓冲给 `None`（不发空帧，前端不该为它渲染一次气泡更新）。
    fn take(&self) -> Option<String> {
        let t = {
            let mut b = self.buf();
            std::mem::take(&mut *b)
        };
        if t.is_empty() {
            return None;
        }
        *self.last_flush() = Instant::now();
        Some(t)
    }

    // R109：锁中毒不 panic 整条流。这三把锁只保护一个 String / 一个 Instant，
    // 中毒后继续用是安全的（没有跨锁不变量）。
    fn buf(&self) -> std::sync::MutexGuard<'_, String> {
        guard(&self.buf)
    }
    fn last_flush(&self) -> std::sync::MutexGuard<'_, Instant> {
        guard(&self.last_flush)
    }
}

/// 生产 `EventSink`：合帧 + 加 session_id + 交给 `Publish`。
pub struct TauriEmitter {
    out: Arc<dyn Publish>,
    session_id: String,
    coalescer: Coalescer,
    images_dir: Option<PathBuf>,
    /// G36：`needs_attention` 命中时调用的动作（生产 = 失焦时闪任务栏；测试 = None）。
    /// 判定在事件层（`agent::events::needs_attention`），动作在这支闭包 —— 注入闭包
    /// 而不是 AppHandle，TauriEmitter 才能在本模块单测里继续裸构造（AppHandle 造不出来）。
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl TauriEmitter {
    pub fn new(out: Arc<dyn Publish>, session_id: String) -> Self {
        Self {
            out,
            session_id,
            coalescer: Coalescer::new(),
            images_dir: None,
            notify: None,
        }
    }

    pub fn with_images_dir(mut self, dir: PathBuf) -> Self {
        self.images_dir = Some(dir);
        self
    }

    /// G36：装配「拉回用户注意力」的动作。只在 `needs_attention` 的事件上被调，
    /// 失焦与否的判定在闭包里（那里才拿得到窗口）。
    pub fn with_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.notify = Some(notify);
        self
    }

    fn send(&self, event: AgentEvent) {
        self.out.publish(AgentPayload {
            session_id: self.session_id.clone(),
            event,
        });
    }

    /// 落帧（会话收尾、以及 turn 结束前都要调一次，否则最后半句话留在缓冲里没人发）。
    pub fn flush(&self) {
        if let Some(text) = self.coalescer.take() {
            self.send(AgentEvent::TextDelta { text });
        }
    }
}

impl EventSink for TauriEmitter {
    fn emit(&self, e: AgentEvent) {
        match e {
            AgentEvent::TextDelta { text } => {
                if let Some(frame) = self.coalescer.push(&text) {
                    self.send(AgentEvent::TextDelta { text: frame });
                }
            }
            // 唯一的一条正确性约束（brief:76）：非 TextDelta 之前必先 flush，
            // 否则 `tool_call` 会跑到未落地的文本之前到达，前端时间线倒序。
            other => {
                self.flush();
                let mut event = other;
                if let Some(dir) = &self.images_dir {
                    crate::image_cache::enrich_event(&mut event, dir);
                }
                // G36：要人决策/收尾的事件交给 OS 注意力机制（失焦才闪，判定在闭包）。
                // 动作失败不反馈进事件流 —— 闪不了任务栏不该变成一条对话错误。
                if crate::agent::events::needs_attention(&event)
                    && let Some(notify) = &self.notify
                {
                    notify();
                }
                self.send(event);
            }
        }
    }
}

// ==== 装配：状态、命令、事件出口 ====

/// 事件通道名（plan:2842）：前端只 `listen("agent")` 这一条通道。
const AGENT_CHANNEL: &str = "agent";

/// 新会话的默认标题。**自动改名只在这个值仍然成立时才进行**（R104），
/// 所以 `session_create` 与 G37 的自动起名任务必须读同一个常量，不许各写一遍字面量。
pub const DEFAULT_TITLE: &str = "新对话";

// R109 的处置本身住在 `crate::lock`（与 `settings.rs`、`store/db.rs`、
// `permission/gate.rs`、`tools/registry.rs` 共用一份）。命令层的锁全是长命的，一次别处的
// panic 不许变成之后每条命令都 panic；这些锁保护的全是单点状态（一个 `String`、一个令牌、
// 一个缓冲），没有跨锁不变量，所以吞掉 poison 不会让人读到撕裂的形状。
use crate::lock::guard;

/// 生产出口：把 payload 发进 tauri 的 `agent` 通道。
/// 这是 `Publish` 的**唯一**生产实现 —— 合帧与路由逻辑全在 `TauriEmitter` 里，与 tauri 无关。
struct AppPublisher {
    app: AppHandle,
}

impl Publish for AppPublisher {
    fn publish(&self, payload: AgentPayload) {
        // 发不出去只可能是窗口已经没了。失败**不打 payload**：事件里带用户原文和工具输出，
        // 日志不该收（spec:473 同一条理由管着 API key）。
        if let Err(e) = self.app.emit(AGENT_CHANNEL, payload) {
            tracing::warn!("agent 事件发送失败：{e}");
        }
    }
}

/// Tauri 托管的应用状态。每个字段都是长命的：`Db`/`SettingsStore` 各只允许一个实例
/// （R101 的 `SettingsStore` 锁是**实例内**的，两个实例同目录并发 save 实测 48 次里
/// 20~30 次报 `Io(os error 2)`），所以一律 `Arc` + 一次 `manage`。
/// 按会话互斥的抽取锁(spec §12.5 缺口 1)。三条抽取路径(压缩 / 空闲 / 会话切换)
/// 共用这一把:同一会话同时只许一次抽取。
///
/// 为什么必须是**按会话**而不是一把全局锁:抽取内含一次模型调用(数秒),全局锁会让
/// 「A 会话在抽」挡住「B 会话在抽」—— 两个无关会话没有冲突的理由。
/// 为什么是「标记集」而不是阻塞锁:重复触发时的正确处置是**放弃**而不是排队 ——
/// 排队意味着用户切会话要等一次模型调用,而游标保证了被放弃的那次不会丢内容
/// (另一次抽取正在处理同一批消息)。
#[derive(Clone, Default)]
pub struct ExtractLocks(Arc<Mutex<std::collections::HashSet<String>>>);

impl ExtractLocks {
    /// 认领一个会话的抽取权;`None` = 它已经在抽了,本次调用应当**安静地放弃**。
    pub fn try_acquire(&self, session_id: &str) -> Option<ExtractGuard> {
        let mut set = crate::lock::guard(&self.0);
        if !set.insert(session_id.to_string()) {
            return None;
        }
        drop(set);
        Some(ExtractGuard {
            locks: self.clone(),
            session_id: session_id.to_string(),
        })
    }

    fn release(&self, session_id: &str) {
        // 中毒不可怕:这个集合只是个「谁在忙」的提示,拿走一条记录本身没有不变量。
        // 用 `into_inner` 而不是 unwrap —— panic 之后永久卡住一个会话的抽取,
        // 比恢复一个死锁集合糟得多。
        let mut set = self.0.lock().unwrap_or_else(|e| e.into_inner());
        set.remove(session_id);
    }
}

/// 抽取权的 RAII guard:析构即释放,`?` 的每条早退路径都不会把会话永久留在「正在抽取」。
pub struct ExtractGuard {
    locks: ExtractLocks,
    session_id: String,
}

impl Drop for ExtractGuard {
    fn drop(&mut self) {
        self.locks.release(&self.session_id);
    }
}

pub struct AppState {
    pub db: Arc<Db>,
    pub settings: Arc<SettingsStore>,
    pub registry: Arc<ToolRegistry>,
    pub gate: Arc<PermissionGate>,
    /// 当前这一轮的取消令牌（一次性，R63）。`run_turn` 装、`interrupt` 读、轮次结束清空。
    /// brief 的 Interfaces 写的是 `Mutex<..>`；外面多一层 `Arc` 是因为 `run_turn` 要在
    /// 不借 `&AppState`（那会把 tauri 类型拖进可测路径）的前提下拿到这个槽位。
    pub cancel: Arc<Mutex<Option<CancellationToken>>>,
    /// 正在运行的会话 id —— 单轮并发的唯一凭据。
    pub running: Arc<Mutex<Option<String>>>,
    /// 上一轮进入过的会话 id：切会话时用它清空「本会话都允许」的授权。
    pub last_session: Arc<Mutex<Option<String>>>,
    /// 日志文件路径（R107：GUI 没有控制台，路径看不见等于日志不存在）。
    /// 装配期才知道 app data 目录，所以是 `Option`；建不出日志文件时给 `None` 而不是失败。
    pub log_path: Option<PathBuf>,
    pub mcp: Arc<McpBridge>,
    pub mcp_dir: PathBuf,
    /// 技能包目录：固定为数据目录下的 `skills/`；装配期建出来，用户把
    /// `SKILL.md` 丢进子目录即可。
    pub skills_dir: PathBuf,
    pub data_dir: PathBuf,
    /// 按会话的抽取互斥(spec §12.5 缺口 1)。空闲 worker 与切换回补都经它,
    /// 避免同一会话被两条路径同时抽取、堆出重复记忆。
    pub extracting: ExtractLocks,
    /// Agent Shell（spec §2/§5.5）：这一轮可用的 shell 全集（预设 + 自定义 + 默认 id）。
    /// `ShellTool` 与 `AppState` 持**同一个** `Arc`（注册表是进程级单例，catalog 不能
    /// 留在某一轮的 `ToolCtx` 里）；`send_message` 每轮开头现刷（spec §2「生效时机」：
    /// 改设置下一条消息生效，无缓存可过期）。锁走 `crate::lock` 的中毒容忍入口。
    pub shells: Arc<RwLock<ShellCatalog>>,
}

/// 单轮并发闸门。guard 一析构就释放槽位，所以 `?` 的每条早退路径（读设置失败、
/// 没配 key、模型构造失败、落库失败）都不会把「正在运行」永久留在状态里。
/// brief 用手动清两遍的写法，`?` 之前那几行根本没执行 —— 表现是界面从此卡在上一轮。
pub struct RunningGuard {
    slot: Arc<Mutex<Option<String>>>,
}

/// 认领这一轮。**任何一轮在跑时都不许起第二轮，同一会话也不行**：
/// brief:113-121 允许同会话重入（先 cancel 上一轮），但上一轮的 `append_message` 还没落地时，
/// 新一轮的 `prior` 已经读完了库 —— 两轮同时落库正是 R103 要消灭的那个形状。
/// 想接着聊就先按停止，等这一轮收尾。
fn claim_running(slot: &Arc<Mutex<Option<String>>>, want: &str) -> Result<RunningGuard, String> {
    let mut current = guard(slot);
    if let Some(busy) = current.clone() {
        return Err(format!(
            "会话 {busy} 正在运行。同一时刻只支持一轮对话，请先中断它。"
        ));
    }
    *current = Some(want.to_string());
    drop(current);
    Ok(RunningGuard { slot: slot.clone() })
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        *guard(&self.slot) = None;
    }
}

/// 进入一轮之前对齐「会话作用域」：**真正开始另一条会话**时，把上一轮的
/// 「本会话都允许」全部作废。
///
/// 语义边界（如实记录）：授权只在发送时被 gate 读取，所以「只点开 B、不在 B 发送」
/// 不会触发清空 —— 那种情况下第一条会话并没有被另一轮对话取代。这避免了为一次纯浏览
/// 多发一条 IPC 命令，代价是 A→B→A 且在 B 不发送时授权会保留到下一次发送。
/// 安全上关键的那一半（server 改成 Block 必须压过旧授权）由 `check` 的优先级保证，
/// 不依赖这里。
fn enter_session_scope(last: &Mutex<Option<String>>, gate: &PermissionGate, session_id: &str) {
    let switched = {
        let mut slot = guard(last);
        if slot.as_deref() == Some(session_id) {
            false
        } else {
            *slot = Some(session_id.to_string());
            true
        }
    };
    if switched {
        gate.rules().clear_session_rules();
    }
}

/// 「这个会话正在跑一轮吗」——**只读一次、立刻释放锁**的判据。
///
/// 抽成函数而不是在调用点写 `guard(..).as_deref() == Some(..)`:`if` 条件里的临时
/// guard 虽然在该语句结束时就丢弃,但把它藏在一个布尔表达式里会让「锁有没有跨
/// await」变成读者要靠编译器版本去猜的事。函数边界把「拿锁 → 抄出一个 bool →
/// 放锁」钉死,后台 tick 的循环里因此不可能持锁跨 await(那会让一次网络调用
/// 把整条命令层卡住)。
fn a_turn_is_running(slot: &Arc<Mutex<Option<String>>>, session_id: &str) -> bool {
    guard(slot).as_deref() == Some(session_id)
}

/// 取消令牌的槽位生命周期也交给所有权：`run_turn` 无论从哪儿退出都清空，
/// 于是 `interrupt` 永远不会打到一个已经死掉的会话的令牌上（R63 的一次性合同）。
struct TurnToken {
    slot: Arc<Mutex<Option<CancellationToken>>>,
}

impl TurnToken {
    fn install(&self, token: CancellationToken) {
        *guard(&self.slot) = Some(token);
    }
}

impl Drop for TurnToken {
    fn drop(&mut self) {
        *guard(&self.slot) = None;
    }
}

/// 一轮对话的全部依赖，**一项 tauri 类型都没有**（R106 的 seam 从出口扩到整条路径）。
pub struct TurnDeps {
    pub db: Arc<Db>,
    pub registry: Arc<ToolRegistry>,
    pub gate: Arc<PermissionGate>,
    pub model: Arc<dyn ModelProvider>,
    pub sink: Arc<dyn EventSink>,
    /// 由 `Settings::to_context_config()` 给（R100）：钳位与预算只有一处定义。
    pub context: ContextConfig,
    /// 由 `Settings::to_model_config()` 给（T39）：`None` = 请求体不带 `temperature`，
    /// 端点自己定。和 `context` 同样理由 —— 会话不许自己去读 `Settings`。
    pub temperature: Option<f32>,
    /// G15：plan 模式开关（OpenCode 的「只读研究」agent）。由 `send_message` 从本轮
    /// 那份 `Settings` 现取（context/temperature/memory 同一条线：会话不自己读
    /// `Settings`），`run_turn` 经 `AgentSession::with_config` 注入 —— 改设置下一条
    /// 消息生效。`true` 时 Write/Exec 级工具调用在会话循环里被拒（session.rs 的
    /// `PLAN_MODE_NOTE`，不执行、不走权限门），Read 级照常。
    pub plan_mode: bool,
    /// G18：诊断命令（write_file/edit_file 成功落盘后每轮跑一次，输出作为
    /// 【自动诊断】合成 user 轮反馈给模型）。与 plan_mode 同一条线：`send_message`
    /// 从本轮 `Settings` 现取，改设置下一条消息生效；`None`/空串 = 关闭。
    pub diagnostics_command: Option<String>,
    /// G19：agent 工具白名单（激活档案的 `tools`）。与 plan_mode 同一条线现取；
    /// `None` = 不限制。会话里双层生效：声明表过滤（`tool_defs_for_turn` 的估算与
    /// `request()` 同参）+ 循环拦截。
    pub allowed_tools: Option<Vec<String>>,
    /// G19：激活档案的名字（拒绝理由点名用）。与 allowed_tools 同源同设。
    pub active_agent_name: Option<String>,
    /// G22：task 工具可点名的 agent 档案表。与 allowed_tools 同一条线现取
    /// （`send_message` 经 [`agent_profiles`] 从 `Settings.agents` 映射，改设置下一条
    /// 消息生效），`run_turn` 装进 `SessionConfig.agents`。
    pub agents: Vec<AgentProfile>,
    /// T5：记忆的四项开关（spec §7），由 `send_message` 从本轮那份 `Settings` 现取
    /// （context/temperature 同一条线：会话不自己读 `Settings`）。`enabled=false` 的
    /// 语义有两级：压缩段零抽取（记忆侧模型调用一个不发起）+ 请求头零备注（连召回
    /// 读库都不做）；记忆数据不清除，清空的入口只在设置页。
    pub memory: MemoryTurnCfg,
    /// T6：嵌入端点客户端。`None` = source 是 `"off"`（用户显式关掉）或构造失败
    /// （warn 后降级）—— 两种形态在检索侧**同一个**答案：纯 2-gram 关键词召回、
    /// 写入侧不建向量。嵌入失败永远降级、永远不阻塞对话/压缩/落库（裁定 4，
    /// spec §8 不变量 5），所以它不是 `Result` 而是 `Option`：失败在构造点就化掉了。
    pub embed: Option<Arc<dyn EmbedProvider>>,
    /// 按会话的抽取互斥(同 `AppState.extracting` 的同一份)。压缩段在抽之前认领;
    /// 拿不到说明另一个路径正在抽同一会话 —— 那段内容归它处理,本轮跳过抽取、
    /// 压缩照常(游标由它推进,内容不丢)。
    pub extract_locks: ExtractLocks,
    /// G44：hooks 配置与信任表所在的数据目录。`None` = 不跑 hooks（测试缺省）。
    /// 与 roots/skills_dir 同一条线：每轮由 `send_message` 从 `AppState` 现取。
    pub data_dir: Option<PathBuf>,
    /// T26（R276）：`read_file` 能读的那张目录表，**每轮从 `Settings` 现取**（`send_message`
    /// 头里那一次 `store.load()` 是它唯一的来源，不缓存 —— 用户改了设置，下一轮就该生效）。
    ///
    /// 为什么它是 `TurnDeps` 的一项、而不是 `SessionDeps` 上再加一个字段：`SessionDeps` 那条
    /// 路已经有 builder 方法 `AgentSession::with_roots`（`session.rs:86`），两处都能收 roots
    /// 就是「一个设置两个门」，两边给的值可以不一致而没人报错。R276 因此把它钉成一条线：
    /// `TurnDeps.roots` → `.with_roots(...)` → `ToolCtx.roots`，只此一处。
    /// T26 修复轮 1（R284）：这一项交的是**用户写的那份原表**（读写两侧不再裁盘），
    /// 「今天哪几条解得开」由 `run_turn` 在 `.with_roots(..)` 那一处经
    /// `settings::effective_roots` 现算 —— 使用点是文件系统依赖的唯一落点。
    /// 空表是**允许值**而且是默认值：那时 `read_file` 一律拒绝，那是设计（R90 取向），
    /// 不是这一层漏了什么。
    pub roots: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
    /// 技能包目录（`use_skill` + 系统提示里的技能索引）。与 roots 一样每轮交给会话，
    /// 会话再原样放进 `ToolCtx` —— 工具层不自己去猜路径。
    pub skills_dir: Option<PathBuf>,
    /// 用户停用的技能文件夹（`skills.json`，数据目录）。每轮从磁盘现读：面板一关掉，
    /// 下一条消息就按新选择走。
    pub skills_disabled: Vec<String>,
    pub cancel: Arc<Mutex<Option<CancellationToken>>>,
}

/// 同步 SQLite / 磁盘读写一律进 `spawn_blocking` —— 这是 `db.rs:50-53` 与 `settings.rs`
/// 结构体文档（N-3）写给 T13 的交接：占住 tokio worker 或占住主线程都是界面卡死。
/// `State` 不是 `'static`，所以调用方先把 `Arc` 克隆进闭包。
async fn blocking<T>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("后台任务失败：{e}"))?
}

/// 两次发送之间的摘要压缩（T41）。
///
/// 为什么它是一段**独立**的 `pub(crate)` 函数、而不是 `run_turn` 里的一串行：
/// `#[tauri::command]` 在单测里调不动（Plan A T13 实测），而 `run_turn` 要凑齐 `TurnDeps`
/// 那八项才谈得上一次调用。把压缩的真相收进一个不碰 `State` 的签名，Step 1–4 的测试直接调
/// 它（brief §5），`run_turn` 剩下的只有「按顺序装配」。
///
/// 三步，全部在「越了线」这个判定里面（`an_under_threshold_history_costs_no_model_call…`
/// 守的就是这条 —— 把 IO 挪到判定外，用户每次发消息都要白烧一次摘要调用）：
/// 1) 问构建器要不要压、压哪一段：纯决策，不调模型不做 IO（T40）；
/// 2) 用被丢掉的那一段换一句摘要。摘要调用报错就降级成 `SUMMARY_OMITTED` 并**照压**
///    （spec:365：降级只换正文，不换事务）；用户在摘要还挂着的时候按了停止是另一件事 ——
///    事务压根还没开始，一个字节都不写，原样交回；
/// 3) 一次事务删旧行 + 落一条 `Role::System` 备注（`db.rs` 的 `compact_session`），
///    然后把库**重读**回来。
///
/// 交回的是重读后的历史：调用方必须在这份历史之上才取 `prior`（brief §4 的第一个陷阱）。
/// 交回的事件只带两个数（`dropped` / `summary_chars`），摘要文本一个字都不在其中 ——
/// 它唯一的去处是库里那条 `Role::System` 备注，而那条备注在模型侧可见、用户侧不可见（R69）。
/// 抽取 →(按需)合并 → 画像归并,产出一次压缩事务里的记忆写入(spec §4)。
/// **每一段的失败都只降级自己**(warn 日志 + 该段零写入),压缩本体照常;
/// 用户按停止的裁定不在这里 —— 调用方(`compact_before_turn`)在事务前统一裁。
/// 读库(同键查重/现画像)发生在事务之前:只读不写,且单轮闸门保证此刻没有第二个写者。
///
/// T15:真正的合并逻辑搬到 [`plan_memory_writes_from_candidates`],本函数只保留
/// 「压缩路径的失败语义」—— 抽取失败 = 本轮零写入(warn),而不是冒泡。空闲/切换抽取
/// 需要相反的语义(失败必须冒泡,否则游标会被推过一段没抽成的对话),所以它直接调
/// 候选层,两条路径共用同一份合并/保护逻辑,不复制。
async fn plan_memory_writes(
    db: &Arc<Db>,
    session_id: &str,
    model: &dyn ModelProvider,
    source: &[Message],
    profile_char_cap: usize,
    cancel: &CancellationToken,
) -> Vec<MemoryWrite> {
    match plan_memory_writes_for_source(db, session_id, model, source, profile_char_cap, cancel)
        .await
    {
        Ok(writes) => writes,
        // 取消不是故障:用户按了停止,压缩段随后整段放弃(调用方还会再查一次令牌),
        // 这里不刷「抽取失败」的日志,免得把一次正常的中断说成端点问题。
        Err(_) if cancel.is_cancelled() => Vec::new(),
        Err(reason) => {
            tracing::warn!("会话 {session_id} 的记忆抽取/合并没有结论,本轮零记忆写入:{reason}");
            Vec::new()
        }
    }
}

/// [`plan_memory_writes`] 的「抽取 + 合并」形态,失败用 `Err` 冒泡。
/// 两个调用方的差别只在**怎么处理 Err**,不在做什么:
/// - 压缩路径(`plan_memory_writes`):Err → warn + 零写入,压缩照常;
/// - 空闲/切换路径([`extract_session_memories`]):Err → 冒泡,**游标不推进**,
///   下一个 tick 重试。抽取失败绝不能当成「这段没什么可记」——那会把记忆永久丢掉。
async fn plan_memory_writes_for_source(
    db: &Arc<Db>,
    session_id: &str,
    model: &dyn ModelProvider,
    source: &[Message],
    profile_char_cap: usize,
    cancel: &CancellationToken,
) -> Result<Vec<MemoryWrite>, String> {
    let candidates = extract_memories(model, source, cancel).await?;
    plan_memory_writes_from_candidates(db, session_id, model, candidates, profile_char_cap, cancel)
        .await
}

/// 候选 → 写入的**唯一**合并层(压缩/空闲/切换三条触发路径共用)。
///
/// T16(user_edited 保护):模型对某一行做的 UPDATE/INVALIDATE,如果那一行是
/// **用户亲手改过**的,就降级成 ADD(新旧并存),并记一行 info。判据在 `existing`
/// ——它就是本次同键对照集,保护集只从它里面取,和 Reinforce 的存在性核对同一个来源。
/// 为什么允许 REINFORCE 指向保护行:它不改正文,只加确认计数 —— 用户写的句子
/// 被「再说一遍」变强是完全正确的语义,没有需要保护的内容。
async fn plan_memory_writes_from_candidates(
    db: &Arc<Db>,
    session_id: &str,
    model: &dyn ModelProvider,
    candidates: Vec<MemoryCandidate>,
    profile_char_cap: usize,
    cancel: &CancellationToken,
) -> Result<Vec<MemoryWrite>, String> {
    let mut writes: Vec<MemoryWrite> = Vec::new();
    let mut profile_adds: Vec<MemoryCandidate> = Vec::new();
    // 不在计划层按 id 去重：同一批里「REINFORCE 之后又 INVALIDATE」必须让失效生效。
    // 重复命中由写入层的宽松版本兜底（0 行只 warn，不回滚整批）。
    for c in candidates {
        // 取消 = 「这一段没有结论」:返回 Err 而不是半截 writes。空闲路径据此
        // 不推进游标;压缩路径在调用方还会再查一次令牌并整段放弃。
        if cancel.is_cancelled() {
            return Err("记忆管线已取消".to_string());
        }
        if c.kind == "profile" {
            profile_adds.push(c);
            continue;
        }
        // 同键查重:查不了就**跳过该候选**,不按无冲突硬写 —— 重复记忆比漏记难清理
        // (漏记下段对话还能再抽,重复要靠用户手删)。
        let existing: Vec<MemoryRow> = match (c.subject.as_deref(), c.predicate.as_deref()) {
            (Some(s), Some(p)) => {
                let reader = db.clone();
                let (rs, rp) = (s.to_string(), p.to_string());
                match blocking(move || {
                    reader
                        .find_active_by_key(&rs, &rp)
                        .map_err(|e| e.to_string())
                })
                .await
                {
                    Ok(rows) => rows,
                    Err(reason) => {
                        tracing::warn!("会话 {session_id} 的同键查重读库失败,跳过该候选:{reason}");
                        continue;
                    }
                }
            }
            _ => Vec::new(),
        };
        // T16:保护集 = 本次同键对照里**用户亲手改过**的行的 id。
        // 每候选现算而不是全局算一次:existing 是按候选键取的,保护范围天然随之变化。
        let protected: HashSet<i64> = existing
            .iter()
            .filter(|r| r.user_edited)
            .map(|r| r.id)
            .collect();
        match consolidate_memory(model, &c, &existing, cancel).await {
            Ok(MemoryDecision::Add) => writes.push(MemoryWrite::Add(
                c.into_new_memory(Some(session_id.to_string())),
            )),
            Ok(MemoryDecision::UpdateBody { id, body }) => {
                // id 必须是本次同键对照集里的行。模型会编序号；放进去的后果是
                // 改写到无关键的记忆（跨键污染），或者 0 行命中让整个压缩事务回滚。
                // Reinforce 早就在查同一个判据，这里不能比它宽松。
                if !existing.iter().any(|r| r.id == id) {
                    tracing::warn!(
                        "会话 {session_id} 的记忆合并返回了不属于当前同键集的 id={id},跳过该条候选"
                    );
                    continue;
                }
                if protected.contains(&id) {
                    // 用户权威优先于系统自洽:改写被降级成「并存」,用户自己决定删哪条。
                    tracing::info!(
                        "会话 {session_id} 的记忆 id={id} 被用户编辑过,模型的 UPDATE 降级为 ADD 并存"
                    );
                    writes.push(MemoryWrite::Add(
                        c.into_new_memory(Some(session_id.to_string())),
                    ));
                } else {
                    writes.push(MemoryWrite::UpdateBody { id, body });
                }
            }
            Ok(MemoryDecision::Invalidate { id }) => {
                if !existing.iter().any(|r| r.id == id) {
                    tracing::warn!(
                        "会话 {session_id} 的记忆合并返回了不属于当前同键集的 id={id},跳过该条候选"
                    );
                    continue;
                }
                if protected.contains(&id) {
                    // 失效同样不动保护行:新候选入库,旧行保持有效。
                    tracing::info!(
                        "会话 {session_id} 的记忆 id={id} 被用户编辑过,模型的 INVALIDATE 降级为 ADD 并存"
                    );
                    writes.push(MemoryWrite::Add(
                        c.into_new_memory(Some(session_id.to_string())),
                    ));
                } else {
                    writes.push(MemoryWrite::Invalidate { id });
                }
            }
            // 强化只对「确实存在的那一行」有意义。模型偶尔会自己编序号:把一条指向
            // 不存在行的 Reinforce 交给事务,`reinforce_row` 的 affected=0 会报错,
            // 整个压缩事务随之回滚 —— 一条垃圾候选拖垮整次压缩,代价远大于漏记。
            // 所以先在 existing 里核对(它就是本次合并的对照集,同一份真相),
            // 查不到只 warn + 跳过该候选,压缩照常。
            // T16:指向保护行**允许** —— REINFORCE 不改正文,只加确认计数。
            Ok(MemoryDecision::Reinforce { id }) => {
                if !existing.iter().any(|r| r.id == id) {
                    tracing::warn!(
                        "会话 {session_id} 的记忆合并返回了不存在的 id={id},跳过该条候选"
                    );
                    continue;
                }
                writes.push(MemoryWrite::Reinforce { id });
            }
            Err(reason) => {
                tracing::warn!("会话 {session_id} 的记忆合并失败,跳过该条候选:{reason}");
            }
        }
    }
    if !profile_adds.is_empty() {
        let reader = db.clone();
        let current = blocking(move || reader.profile().map_err(|e| e.to_string()))
            .await
            .unwrap_or_else(|reason| {
                tracing::warn!("会话 {session_id} 的画像读取失败,按空画像归并:{reason}");
                String::new()
            });
        match merge_profile(model, &current, &profile_adds, profile_char_cap, cancel).await {
            Ok(body) => writes.push(MemoryWrite::SetProfile(body)),
            Err(reason) => {
                tracing::warn!("会话 {session_id} 的画像归并失败,画像保持现状:{reason}");
            }
        }
    }
    Ok(writes)
}

// 9 个参数是压缩轮次的真实签名，收拢成参数对象属于签名重构，超出本次机械清理范围。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn compact_before_turn(
    db: &Arc<Db>,
    session_id: &str,
    model: &dyn ModelProvider,
    builder: &ContextBuilder,
    history: Vec<Message>,
    memory: &MemoryTurnCfg,
    embed: Option<Arc<dyn EmbedProvider>>,
    locks: &ExtractLocks,
    cancel: &CancellationToken,
) -> Result<(Vec<Message>, Option<AgentEvent>, usize), String> {
    let Some(plan) = builder.compaction_plan(&history) else {
        return Ok((history, None, 0));
    };

    let body = match collect_summary(model, summary_request(&plan.source), cancel).await {
        Ok(text) => text,
        // 取消优先于报错。`collect_summary` 的 `Err` 在这一臂既可能是「用户按了停止」也可能
        // 是端点报错，所以判据看令牌、不看错误文本（错误文本里那句「已取消」是它的兜底说法，
        // 不是事实来源）。没压成，也就没什么可宣布的。
        Err(_) if cancel.is_cancelled() => return Ok((history, None, 0)),
        // 降级。原因只进日志、不进事件：`ContextCompacted` 的载荷是两个数，前端那句
        // 「已压缩 N 轮」不该被一条端点文案污染；日志才是它该去的地方（R107，GUI 没有控制台）。
        Err(reason) => {
            tracing::warn!("会话 {session_id} 的摘要调用失败，本轮改为省略占位：{reason}");
            SUMMARY_OMITTED.to_string()
        }
    };

    // 记忆抽取(spec §4.1):输入就是即将永久消失的那段原文 —— 压缩从纯损失变成一次蒸馏。
    // 取消语义与摘要侧同一条:事务还没开始,一个字节都不写;其余失败已在里面各自降级。
    // 总开关关掉 = 这一段**整段跳过**:零记忆写入、零记忆模型调用(不是「抽了不写」,
    // 抽取是一次真实端点调用,用户关掉的功能不许还有开销 ——
    // `memory_disabled_skips_extraction_entirely` 钉着);`profile_char_cap` 只在这条
    // 路径上约束画像归并的输出。
    // 认领抽取权:拿不到说明同一会话正被空闲 worker / 切换回补处理 —— 那批内容归它,
    // 本轮跳过抽取、压缩照常(它会把游标推过去,内容不会被重抽也不会丢)。
    // 这个窗口极小(压缩发生时本轮已占 `running` 槽,另外两条路径都会先看那个槽),
    // 但「极小」不是「不存在」。
    let writes = if memory.enabled {
        match locks.try_acquire(session_id) {
            Some(_guard) => {
                plan_memory_writes(
                    db,
                    session_id,
                    model,
                    &plan.source,
                    memory.profile_char_cap,
                    cancel,
                )
                .await
            }
            None => {
                tracing::info!("会话 {session_id} 正被另一条路径抽取记忆,本轮压缩段跳过抽取");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    if cancel.is_cancelled() {
        return Ok((history, None, 0));
    }
    // 本次记住了几条(画像另算:它不是一条条的记忆)。给 `MemorySaved` 用。
    let saved = writes
        .iter()
        .filter(|w| !matches!(w, MemoryWrite::SetProfile(_)))
        .count();

    let note = summary_note(&body);
    let dropped = plan.dropped;
    // 字数按字符算不是按字节：中文备注用 `len()` 会显示成 3 倍（brief §4 的第三条陷阱）。
    let summary_chars = note.chars().count();
    // 新增行的 body 在事务**前**收好(只对 Add —— Update/Invalidate 复用旧行、
    // SetProfile 是单行画像,都不产新行):事务返回的新增 id 按同一顺序对位,
    // zip 起来才是「这一行记忆 ↔ 这一条向量」。事务失败时这段清单跟着被丢,不会外泄。
    let add_bodies: Vec<String> = writes
        .iter()
        .filter_map(|w| match w {
            MemoryWrite::Add(m) => Some(m.body.clone()),
            _ => None,
        })
        .collect();
    let writer = db.clone();
    let write_sid = session_id.to_string();
    let new_ids = blocking(move || {
        writer
            .compact_session_with_memories(&write_sid, &plan, &note, &writes, None)
            .map_err(|e| e.to_string())
    })
    .await?;

    // 嵌入补写(T6,裁定 4):压缩事务已提交,新行先以「纯关键词可检索」的形态落地;
    // 向量随后补。抽成 [`backfill_memory_embeddings`] 是因为空闲/切换抽取要**同一套**
    // 语义与日志(顺序对位、失败不回滚),复制一份就会在两次升级之间分叉。
    backfill_memory_embeddings(db, new_ids, add_bodies, embed).await;

    // 重读，不在内存里自己拼那份「删了若干行 + 加了一条备注」的结果：
    // 库里是什么样就以库为准，`run_turn` 之后的 `restore`/`append_rows` 才有同一个基准。
    let reader = db.clone();
    let read_sid = session_id.to_string();
    let reread = blocking(move || reader.messages(&read_sid).map_err(|e| e.to_string())).await?;
    Ok((
        reread,
        Some(AgentEvent::ContextCompacted {
            dropped,
            summary_chars,
        }),
        saved,
    ))
}

/// 写入侧补向量的**唯一**一处(T6 裁定 4,压缩/空闲/切换三条路径共用):
/// 记忆行已随事务落地,向量随后补 —— 批量嵌入一次,逐行落 BLOB。
/// 任何失败只 warn,**不回滚、不阻塞**:行已在库里,降级的代价只是该行少一个召回通道。
/// `embed` 为 `None`(off/构造失败)时整段跳过 —— off 是用户显式选择,不是故障,不刷日志。
///
/// 顺序对位不变量:`new_ids` 与 `add_bodies` 都按 `writes` 里 `Add` 的顺序 ——
/// 调用方(两个事务入口)返回的 id 都遵守同一条约定,这里的 zip 才是「这一行 ↔ 这条向量」。
async fn backfill_memory_embeddings(
    db: &Arc<Db>,
    new_ids: Vec<i64>,
    add_bodies: Vec<String>,
    embed: Option<Arc<dyn EmbedProvider>>,
) {
    if new_ids.is_empty() {
        return;
    }
    let Some(provider) = embed else {
        return;
    };
    match provider.embed(&add_bodies).await {
        Ok(vecs) if vecs.len() == new_ids.len() => {
            let pairs: Vec<(i64, Vec<f32>)> = new_ids.into_iter().zip(vecs).collect();
            let blob_writer = db.clone();
            if let Err(reason) = blocking(move || {
                for (id, vec) in &pairs {
                    if let Err(e) = blob_writer.set_memory_embedding(*id, vec) {
                        // 单行失败(该行刚被用户清空是唯一现实来源):行已不在,
                        // 向量无处可写,其余行照常。
                        tracing::warn!("记忆 id={id} 的向量写入失败,该行保留为纯关键词可检索:{e}");
                    }
                }
                Ok(())
            })
            .await
            {
                tracing::warn!("记忆向量写入任务失败,新增行保留为纯关键词可检索:{reason}");
            }
        }
        Ok(vecs) => {
            tracing::warn!(
                "嵌入端点返回 {} 条向量与新增记忆 {} 条不符,新行保留为纯关键词可检索",
                vecs.len(),
                new_ids.len()
            );
        }
        Err(reason) => {
            tracing::warn!("记忆嵌入失败,新增行保留为纯关键词可检索:{reason}");
        }
    }
}

/// T15(P0):一次空闲抽取最多吃多少条消息。
///
/// 为什么是 40:空闲抽取的输入量要与**一次压缩段**同量级(spec §4.1 的抽取输入就是
/// 被压掉的那段原文,默认窗口 40 条)。无上限会让「一个聊了 500 轮的会话」在第一次
/// tick 里把整段历史塞进一次请求 —— 成本不可控,而且模型的注意力会被稀释。
/// 超出的部分不丢:游标只推进到第 40 条的 id,下一 tick 接着抽(每 60 秒一 tick)。
pub(crate) const MAX_IDLE_EXTRACT_MESSAGES: usize = 40;

/// 空闲/切换会话时的一次抽取:游标之后的新对话 → 记忆,并推进游标。
///
/// 与压缩路径的区别**只在失败语义**(见 [`plan_memory_writes_for_source`]):
/// 抽取/合并任何一步失败都返回 `Err` 且**不推进游标**,下一个 tick(或下一次切换)
/// 从头重试。这与压缩路径「抽取失败仍然压缩」的取向相反,因为这里的抽取**没有别的
/// 消费者**:压缩即使不抽记忆,原文也已经变成摘要(至少还剩一句话);空闲抽取若不重试,
/// 那段对话会在下一次压缩时被当摘要处理,细节永久消失。
///
/// 返回**变更条数**(`Add`/`UpdateBody`/`Invalidate`/`Reinforce` 的总数)。
/// `SetProfile` 不计入:画像是覆盖写,不是「一条新记忆」,把它算进来会让设置页的
/// 「这次抽到几条」在读起来时对不上账。
pub(crate) async fn extract_session_memories(
    db: &Arc<Db>,
    session_id: &str,
    model: &dyn ModelProvider,
    memory: &MemoryTurnCfg,
    embed: Option<Arc<dyn EmbedProvider>>,
    locks: &ExtractLocks,
    cancel: &CancellationToken,
) -> Result<usize, String> {
    // 按会话互斥(spec §12.5 缺口 1):另一条路径正在抽这个会话 ⇒ 放弃,不是排队。
    // 返回 Ok(0) 而不是 Err:被放弃不是失败,调用方(切换/空闲)都按「这次没抽到东西」处理。
    let _guard = match locks.try_acquire(session_id) {
        Some(g) => g,
        None => return Ok(0),
    };
    // 游标与待抽取行**同一次**读库:分两次读会夹进这一瞬间的新 append,让
    // 「读了游标、取了行」之间出现一段既不在行里也不在游标之前的空洞。
    let reader = db.clone();
    let read_sid = session_id.to_string();
    let (rows, max_id) = blocking(move || {
        let cursor = reader
            .memory_cursor(&read_sid)
            .map_err(|e| format!("读取抽取游标失败:{e}"))?;
        // 行集与 max(id) 必须同一把锁：分开读会让并发 append 落在两次读之间，
        // 「全部处理完」的分支把游标推到含新消息的 max(id)，那条文本就永久漏抽。
        reader
            .extraction_batch(&read_sid, cursor)
            .map_err(|e| format!("读取待抽取消息失败:{e}"))
    })
    .await?;
    if rows.is_empty() {
        // 尾部只剩工具结果 / system / 无文本行时，行集为空但 max(id) 仍然更大。
        // 不推游标，`sessions_needing_extraction` 会每个 60s tick 都返回这条会话，
        // 后台永远空转（还会每 tick 构造一次模型客户端去读 keyring）。
        if max_id > 0 {
            let writer = db.clone();
            let write_sid = session_id.to_string();
            blocking(move || {
                writer
                    .apply_memories_and_advance_cursor(&write_sid, &[], max_id)
                    .map_err(|e| format!("推进抽取游标失败:{e}"))
            })
            .await?;
        }
        return Ok(0);
    }
    // 上限之外的部分留给下一个 tick:游标只推进到**实际送进模型**的最后一条。
    let take = rows.len().min(MAX_IDLE_EXTRACT_MESSAGES);
    // 所有可抽取行都送进去了 ⇒ 游标推到 max(id)，把夹在中间的不可抽取行一起跨过；
    // 否则只推到实际送出的最后一条，下一 tick 继续。
    let upto_id = if take == rows.len() {
        max_id
    } else {
        rows[take - 1].0
    };
    let source: Vec<Message> = rows.into_iter().take(take).map(|(_, m)| m).collect();

    // 抽取失败 → `?` 冒泡,游标不动(见函数文档)。空 candidates 是**成功**的
    // 一次抽取(「这段没什么值得记」),游标照常推进 —— 否则同一段会被每 tick 重抽。
    let candidates = extract_memories(model, &source, cancel).await?;
    let writes = plan_memory_writes_from_candidates(
        db,
        session_id,
        model,
        candidates,
        memory.profile_char_cap,
        cancel,
    )
    .await?;
    let changed = writes
        .iter()
        .filter(|w| !matches!(w, MemoryWrite::SetProfile(_)))
        .count();
    let add_bodies: Vec<String> = writes
        .iter()
        .filter_map(|w| match w {
            MemoryWrite::Add(m) => Some(m.body.clone()),
            _ => None,
        })
        .collect();

    // 记忆写入与游标推进在**同一个事务**里(store 层的不变量):只写记忆不推游标
    // ⇒ 下次重抽;只推游标不写记忆 ⇒ 这段永久丢失。
    let writer = db.clone();
    let write_sid = session_id.to_string();
    let new_ids = blocking(move || {
        writer
            .apply_memories_and_advance_cursor(&write_sid, &writes, upto_id)
            .map_err(|e| format!("写入记忆并推进游标失败:{e}"))
    })
    .await?;
    backfill_memory_embeddings(db, new_ids, add_bodies, embed).await;
    Ok(changed)
}

/// 一期记忆注入的默认参数(spec §7 的表)。T5 起它们是**设置项的默认锚点**:
/// `MemoryTurnCfg::default` 引用这两个常量与 `agent::memory::PROFILE_CHAR_CAP`,
/// `Settings` 侧的默认值不许与此分叉
/// (`memory_turn_cfg_defaults_match_the_settings_defaults` 钉着)。
/// `MEMORY_RECALL_K` 是召回条数上限,`MEMORY_NOTE_TOKEN_CAP` 是两条备注(画像+召回)
/// 合计的 token 上限 —— 超了从相关度最低的召回行开始砍,画像永不因超限丢失。
pub(crate) const MEMORY_RECALL_K: usize = 5;
pub(crate) const MEMORY_NOTE_TOKEN_CAP: usize = 600;
/// T10:待办提醒的默认前瞻窗口(小时)。到期时刻落在「现在 + 这个窗口」内的待办
/// 会在本轮被提醒一次。24 小时 = 「今天该做的事,今天早上就告诉你」;窗口太短会让
/// 早上九点的待办在八点五十九分的会话里悄无声息地漏掉。
pub(crate) const MEMORY_PROSPECTIVE_WINDOW_HOURS: u32 = 24;
/// T12:事实记忆衰减半衰期的默认锚点(天),与 `settings::DEFAULT_MEMORY_DECAY_HALF_LIFE_DAYS`
/// 及 `store::memory::RecallCfg::default().fact_half_life_days`(60.0)逐字相同。
/// 偏好不另立常量:它按事实的 3 倍推导(`memory_notes_for_turn` 的注释说明这个比例)。
pub(crate) const MEMORY_FACT_HALF_LIFE_DAYS: f64 = 60.0;
/// T10:一次最多提醒几条待办。超出的**不提醒也不标记**,留给下一轮 ——
/// 一次塞十条会淹没真正的紧急项,而标记了却不注入等于把提醒吞掉。
pub(crate) const MEMORY_DUE_NOTE_MAX_ITEMS: usize = 5;
/// T10:编码特异性里「上一轮用户消息」保留的字符数。query 只是召回线索,
/// 不是要原样发出去的正文;200 字足够覆盖一句追问所依赖的实体。
pub(crate) const RECALL_QUERY_PRIOR_CHARS: usize = 200;
/// T6:`memory_embedding_model` 的默认锚点(裁定 3,与 settings.rs 的
/// `DEFAULT_MEMORY_EMBEDDING_MODEL` 同字面量 —— 分叉由
/// `memory_turn_cfg_defaults_match_the_settings_defaults` 盯着)。
pub(crate) const MEMORY_EMBEDDING_MODEL: &str = "text-embedding-v3";

/// 一轮记忆行为的全部开关(spec §7 的表)。住在 ipc.rs 而不是 settings.rs 是有意的:
/// settings.rs 的纪律是连 tauri 都不依赖的底层文件,给它加一个「to_memory_cfg」
/// 派生方法是让底层认识上层的形状 —— 组装由唯一的生产装配点 `send_message`
/// 从本轮那份 `Settings` 现做,与 context/temperature/roots 同一条线。
/// `Clone` 而不是 `Copy`(T6 加了 `embedding_model: String`):run_turn 需要的字段
/// 提前拆成局部量再进 `move` 闭包,不为保留 Copy 给 String 找托管类型。
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryTurnCfg {
    /// 总开关。`false` = 压缩段零抽取(记忆侧的模型调用一个不发起)+ 请求头零备注
    /// (连召回读库都不做);记忆数据**不**因此清除 —— 关闭是停用,不是抹掉。
    pub enabled: bool,
    /// 召回条数上限(spec §7:1..=20)。
    pub recall_k: usize,
    /// 画像+召回+概览几条备注合计的 token 上限(spec §7:200..=2000 且 ≤ budget/5)。
    /// 待办提醒**不在**这个预算里(见 `prospective_window_hours`):它是「必须说的事」,
    /// 与可选的召回不同。
    pub note_token_cap: usize,
    /// 画像正文的字符上限(spec §7:100..=2000),约束压缩路径上的画像归并输出。
    pub profile_char_cap: usize,
    /// T10/T12:待办提醒的前瞻窗口(小时,默认 24)。到期时刻落在这个窗口内的待办
    /// 会在本轮 head 里出现一条 `[待办提醒]`。**T12 起它由设置项
    /// `memory_prospective_window_hours` 驱动**(T10 时是硬编码常量);这一项没有
    /// 预算耦合 —— 待办提醒本来就不计入 note token cap(见 `note_token_cap`)。
    pub prospective_window_hours: u32,
    /// T12:事实记忆的衰减半衰期(天),由 `Settings::memory_decay_half_life_days` 转成
    /// `f64` 进来。召回排序里 `RecallCfg::fact_half_life_days` 的唯一来源 ——
    /// `memory_notes_for_turn` 不再读 `RecallCfg::default()`,否则设置页的旋钮就是装饰品。
    pub fact_half_life_days: f64,
    /// T6(裁定 2):嵌入来源是不是 `"endpoint"`。`false` = `"off"` —— 构造
    /// `EmbedEndpoint` 的判据就这一个布尔,让「第三态不存在」在类型上成立
    /// (normalize 已把一切不认识的值回落成 endpoint,见 settings.rs)。
    pub embedding_endpoint: bool,
    /// T6(裁定 3):嵌入请求体里的 model 名,原样透传给端点(支不支持只有端点知道)。
    pub embedding_model: String,
}

impl Default for MemoryTurnCfg {
    /// 逐字等于 `Settings::default()` 的记忆项。锚点引用常量而不是再抄字面量:
    /// 这些 `pub(crate)`/`pub` 常量从一期起就是这些默认值的唯一真相,谁改常量这里跟着走;
    /// 与 Settings 侧不许分叉,由 `memory_turn_cfg_defaults_match_the_settings_defaults` 盯着。
    fn default() -> Self {
        Self {
            enabled: true,
            recall_k: MEMORY_RECALL_K,
            note_token_cap: MEMORY_NOTE_TOKEN_CAP,
            profile_char_cap: PROFILE_CHAR_CAP,
            prospective_window_hours: MEMORY_PROSPECTIVE_WINDOW_HOURS,
            // T12:半衰期的默认锚点(60 天),与 Settings::default() 同值(check 由
            // `memory_turn_cfg_defaults_match_the_settings_defaults` 盯着)。
            fact_half_life_days: MEMORY_FACT_HALF_LIFE_DAYS,
            // 默认 source 是 "endpoint"(裁定 2)⇒ 布尔默认 true。
            embedding_endpoint: true,
            embedding_model: MEMORY_EMBEDDING_MODEL.to_string(),
        }
    }
}

/// T12:`Settings → MemoryTurnCfg` 的**唯一**转换点。原本这段内联在 `send_message` 里,
/// 但 `#[tauri::command]` 在单测里调不动(R103 段),于是「设置页改了一个旋钮,它到底有没有
/// 到达这一轮」这件事在测试里无法取证。抽成纯函数之后,`the_turn_cfg_takes_both_pacing_
/// settings_from_the_settings_table` 能直接钉住两项新设置的传递,而 `send_message` 只剩
/// 一行调用 —— 与 context/temperature/roots 的「装配层从本轮 Settings 现取」同一条线。
///
/// 放在 ipc.rs 而不是 `Settings` 的派生方法:settings.rs 是不依赖上层的底层文件,
/// 给它加 `to_memory_cfg` 就是让底层认识 ipc 的形状(与 `MemoryTurnCfg` 的文档同一理由)。
pub(crate) fn memory_turn_cfg(settings: &Settings) -> MemoryTurnCfg {
    MemoryTurnCfg {
        enabled: settings.memory_enabled,
        recall_k: settings.memory_recall_k as usize,
        note_token_cap: settings.memory_note_token_cap as usize,
        profile_char_cap: settings.memory_profile_char_cap as usize,
        // T12:T10 的「先固定 24 小时」到此为止 —— 两个节奏旋钮都从本轮设置现取。
        prospective_window_hours: settings.memory_prospective_window_hours,
        fact_half_life_days: settings.memory_decay_half_life_days as f64,
        // T6:source 归一后只有两种取值,== "endpoint" 即精确判定。
        embedding_endpoint: settings.memory_embedding_source == "endpoint",
        // normalize 已 trim,这里原样透传(端点支不支持只有端点知道,裁定 3)。
        embedding_model: settings.memory_embedding_model.clone(),
    }
}

/// G22:`Settings.agents → Vec<AgentProfile>` 的**唯一**转换点(与 `memory_turn_cfg`
/// 同一条纪律:`#[tauri::command]` 在单测里调不动,映射抽成纯函数才能单测钉住;
/// 放 ipc 不做成 `Settings` 的派生方法,理由同 `memory_turn_cfg` —— settings.rs 是
/// 不依赖上层的底层文件)。只映射 name/prompt/tools:`description` 是给人看的档案
/// 说明,模型与子会话都用不上 —— 丢在这里是有意的,不是漏搬。
pub(crate) fn agent_profiles(settings: &Settings) -> Vec<AgentProfile> {
    settings
        .agents
        .iter()
        .map(|p| AgentProfile {
            name: p.name.clone(),
            prompt: p.prompt.clone(),
            tools: p.tools.clone(),
        })
        .collect()
}

/// T10(机制一:编码特异性):召回 query 拼上**上一轮**用户消息的文本。
///
/// 人回忆时并不只用当前这句话当线索:追问(「那它呢」「还有别的吗」)本身没有实体,
/// 但紧邻的上一句有。把「上一轮用户说过什么」拼在当前消息之前,指代性追问才能命中
/// 上一轮提到的实体 —— 这是编码特异性(encoding specificity)在检索侧的工程对应:
/// 提取线索与编码时的上下文越接近,回忆越容易。
///
/// 取 `history` 里**最后一条** `role == User` 且**不含 ToolResult** 的消息:
/// - 工具结果消息的 role 也是 `User`(`Message::tool` 的协议妥协),它根本不是
///   用户说的话,不能当上下文线索。判据与 `agent::context` 里那个私有谓词
///   `has_tool_result` 同一个思路,但**不**把它提成跨模块公共 API(那会让「哪些
///   消息算用户轮」多一个定义点),就地按块种类判断。
/// - 上一轮文本截前 `RECALL_QUERY_PRIOR_CHARS` 字符:query 只是线索,不是要原样
///   发出去的正文;再长会把窗口外的旧话题一起拖进检索。
/// - `history` 是**压缩后**的历史(不含本轮),所以这里看到的「上一轮」就是模型
///   在上一轮真正见过的最后一句用户话。
/// - 没有这样的消息、或它的文本为空(纯图片轮)→ 只返回 `current`。
pub(crate) fn recall_query(history: &[Message], current: &str) -> String {
    let prior = history.iter().rev().find(|m| {
        m.role == Role::User
            && !m
                .blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            && !m.is_tool_image_followup()
    });
    let Some(m) = prior else {
        return current.to_string();
    };
    let text = m.text_of();
    let text = text.trim();
    if text.is_empty() {
        return current.to_string();
    }
    let clipped: String = text.chars().take(RECALL_QUERY_PRIOR_CHARS).collect();
    format!("{clipped}\n{current}")
}

/// T10(机制二:前瞻记忆):把「到期了、还没提醒过」的待办拼成一条备注。
///
/// 窗口是 `now + window_hours`:到期时刻落进这个窗口(含已过期)的行由
/// `Db::due_prospective` 捞出,按 `due_at` 升序。最多取
/// `MEMORY_DUE_NOTE_MAX_ITEMS` 条;**超出的行既不提醒也不标记**,留给下一轮 ——
/// 一次塞十条会淹没真正的紧急项,而标记了却不注入等于把提醒悄悄吞掉。
///
/// 交回 `(笔记文本, 被收进这条笔记的行 id)`:文本是空表或一个元素的 Vec(注入侧
/// 的统一形状),id 供调用方在**注入之后**标记 `notified_at`。标记不在这里做,
/// 是因为「注入」发生在调用方装配 head 的那一刻;把读与写拆开,调用方才能钉住
/// 「标记失败仍要注入」这个取向。
///
/// 日期按 **UTC** 格式化(与 T9 的 `parse_iso8601_ms` 同一口径:无时区字面值按
/// UTC 解释)。不引 chrono —— 一段纯整数换算就能把日期钉死,而新增依赖会拖
/// 整条构建链(见 `agent::memory` 里 `days_from_civil` 的同一条注释)。
pub(crate) fn due_notes(db: &Db, window_hours: u32) -> Result<(Vec<String>, Vec<i64>), String> {
    let until = Db::now_ms() + window_hours as i64 * 60 * 60 * 1_000;
    let rows = db.due_prospective(until).map_err(|e| e.to_string())?;
    let mut note = String::from("[待办提醒]");
    let mut ids = Vec::new();
    for m in rows.iter().take(MEMORY_DUE_NOTE_MAX_ITEMS) {
        // 查询本身保证 due_at 非空;`unwrap_or_default` 只是不让一条坏数据把
        // 整条提醒打空(真的出现时日期显示为空,正文仍然在)。
        let due = m.due_at.map(due_date_label).unwrap_or_default();
        note.push_str(&format!("\n- {}({due})", m.body));
        ids.push(m.id);
    }
    if ids.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    Ok((vec![note], ids))
}

/// T10(机制三:元记忆):记忆库概览。
///
/// 没有这条备注,模型不知道「我有一个可以检索的库」,`memory_search` 工具就形同
/// 虚设 —— 它不会凭空想到去调一个不知道有内容的东西。所以概览只说两件事:
/// 有多少条(按 kind 分)、该用哪个工具去查。
///
/// 没有任何有效记忆 → `None`(空库不注入,消费方据此完全跳过)。
/// 极短是有意的:它是「元」信息,不该挤占真正的画像与召回。
pub(crate) fn overview_note(db: &Db) -> Result<Option<String>, String> {
    let summary = db.memory_summary().map_err(|e| e.to_string())?;
    let total: i64 = summary.iter().map(|(_, n)| *n).sum();
    if total == 0 {
        return Ok(None);
    }
    let parts: Vec<String> = summary
        .iter()
        .map(|(kind, n)| format!("{} {n}", kind_label(kind)))
        .collect();
    Ok(Some(format!(
        "[记忆库]你有 {total} 条长期记忆({}),可用 memory_search 工具按需检索。",
        parts.join("、")
    )))
}

/// kind 的中文标签(只服务概览这一条备注)。不认识的 kind 原样显示 ——
/// 与其猜一个中文名把模型的胡话洗成正常分类,不如让用户/模型看到原始值。
fn kind_label(kind: &str) -> &str {
    match kind {
        "fact" => "事实",
        "preference" => "偏好",
        "prospective" => "待办",
        other => other,
    }
}

/// epoch 毫秒 → UTC 的 `YYYY-MM-DD`。负时间戳用 `div_euclid`(向负无穷取整),
/// 否则 1970 年之前的那半天会被算到前一天 —— 待办的期限不会落在 1970 年,
/// 但一个只在正数上正确的换算不值得写。
fn due_date_label(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// 距 1970-01-01 的天数 → 公历日期。Howard Hinnant 的 `civil_from_days`,与
/// `agent::memory::days_from_civil` 互为反函数(那边把日期换成毫秒,T9 的解析口径;
/// 这边把毫秒换回日期,给提醒里的「到期日」用)。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// 装配本轮的记忆备注(纯 db 读写,无模型调用)。顺序是硬性的(T10):
///
/// ```text
/// [待办提醒] → [用户画像] → [长期记忆·按相关度] → [记忆库]
/// ```
///
/// 也就是「最紧急的先被看到」:到期的事 → 用户是谁 → 这轮相关的旧事 → 库里还有什么。
/// 概览在最后,因为它是唯一一条「关于记忆的记忆」。
///
/// 预算口径分两类:
/// - **待办提醒不受 `cap` 约束**。它是「必须说的事」,漏掉一条到期的承诺比多花几十
///   token 糟得多;它有自己的独立上限(`MEMORY_DUE_NOTE_MAX_ITEMS` 条)。
/// - 画像/召回/概览合计受 `cap` 约束,超了从**相关度最低的召回行**开始砍,再砍概览;
///   画像永不因超限丢失(画像受 `PROFILE_CHAR_CAP` 独立约束)。
///
/// 待办在**注入之后**立刻标记 `notified_at`:注入在内存、标记在库,同一事务做不到。
/// 取向是**宁可重复,不可丢失** —— 标记失败只 warn,这一轮的提醒照常注入。反过来
/// (先标记、失败了就不注入)会让一条到期承诺彻底消失,而重复一次只是多看一眼。
pub(crate) fn memory_notes_for_turn(
    db: &Db,
    query: &str,
    query_vec: Option<Vec<f32>>,
    k: usize,
    cap: usize,
    window_hours: u32,
    fact_half_life_days: f64,
) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();

    // ① 待办提醒:先取、先标记。`due_notes` 只读,标记在这里做 ——
    // 标记失败不影响注入(见上面的取向说明)。
    let (due, due_ids) = due_notes(db, window_hours)?;
    if !due_ids.is_empty()
        && let Err(reason) = db.mark_notified(&due_ids, Db::now_ms())
    {
        tracing::warn!("待办提醒标记失败,本轮仍然注入(宁可重复一次,不可丢失):{reason}");
    }
    notes.extend(due);

    // ② 画像:常驻,不受超限影响。
    let mut used = 0usize;
    let profile = db.profile().map_err(|e| e.to_string())?;
    if !profile.is_empty() {
        let note = format!("[用户画像]{profile}");
        used += crate::agent::context::note_tokens(&note);
        notes.push(note);
    }

    // ③ 召回:按相关度装,装不下就从最低分那行开始放弃。
    //
    // T12:打分配置**只从参数取半衰期**,不再吃 `RecallCfg::default()` —— 那一份的 60 天
    // 会把设置页的旋钮变成装饰品。其余三项(strength_weight/decay_floor/now_ms)仍走默认:
    // 它们是「人味」的形状参数,没有一个用户会为它调滑块,收进设置页只会把一行挤掉。
    //
    // 偏好半衰期按 **事实的 3 倍** 推导,比例是**刻意的近似**,不是从哪条公式推出来的:
    // 「用户偏好简洁回答」这类自我描述会一直成立,而「用户住在北京」会变(搬家、换工作)。
    // 3 倍与 store 侧默认锚点(60/180)同比例,于是用户把事实调到 7 天时偏好是 21 天,
    // 两档的相对关系在所有设置下都保持不变 —— 一个旋钮改两档,不多开一个没人能解释
    // 「偏好应该是事实的几倍」的设置项。
    let recall_cfg = RecallCfg {
        fact_half_life_days,
        preference_half_life_days: fact_half_life_days * 3.0,
        ..Default::default()
    };
    let hits = db
        .search_memories(query, query_vec.as_deref(), k, &recall_cfg)
        .map_err(|e| e.to_string())?;
    if !hits.is_empty() {
        let header = "[长期记忆·按相关度]";
        let mut note = String::from(header);
        let mut included = 0usize;
        for m in &hits {
            let next = format!("{note}\n- {}", m.body);
            if crate::agent::context::note_tokens(&next) + used > cap {
                break;
            }
            note = next;
            included += 1;
        }
        if included > 0 {
            used += crate::agent::context::note_tokens(&note);
            notes.push(note);
        }
    }

    // ④ 概览:最后一条,也是最先被 cap 砍掉的一条(它最「元」、最不紧急)。
    if let Some(overview) = overview_note(db)?
        && used + crate::agent::context::note_tokens(&overview) <= cap
    {
        notes.push(overview);
    }

    Ok(notes)
}

/// 一轮对话的完整生产路径：读历史 →（越线就）摘要压缩 → `AgentSession::run` →
/// **只写新增的行** → 条件自动改名。
///
/// R103 的落库所有权就落在 `prior` 这一个下标上：`session.rs:110` 的 `run()` 自己会
/// `history.push(Message::user(blocks))`（一条用户轮次 = 一行），而 `restore()` 装回来的
/// 历史本来就在库里。所以发送前一行都不写、跑完只写 `history[prior..]`。
/// brief:152-160 的写法（按块补写 + 整段重放）会把一轮的两行写成四行。
///
/// T41 加的那一段就插在 `prior` 之前，顺序是硬性的（brief §4 的三条陷阱）：
/// ① `prior` 取在压缩段**重读之后**那份历史之上 —— 取在压缩前就是 `history()[40..]` 越界；
/// ② 构建器只造一次，压缩段借 `&builder`，同一个 `builder` 随后 move 进 `SessionDeps`
///    （造两次就是两份阈值、两份校准，压缩判定和装配判定从此可以分叉）；
/// ③ `ContextCompacted` 在写本轮新行**之前**emit（顺序在用户那边是「先说压缩、再答这句」），
///    也在 `sink` move 进 `SessionDeps` 之前用借用发。
pub async fn run_turn(
    deps: TurnDeps,
    session_id: &str,
    blocks: Vec<ContentBlock>,
) -> Result<StopReason, String> {
    let TurnDeps {
        db,
        registry,
        gate,
        model,
        sink,
        context,
        temperature,
        plan_mode,
        diagnostics_command,
        allowed_tools,
        active_agent_name,
        agents,
        memory,
        embed,
        mut roots,
        output_dir,
        skills_dir,
        skills_disabled,
        cancel,
        extract_locks,
        data_dir,
    } = deps;
    if let Some(dir) = &output_dir
        && !roots.contains(dir)
    {
        roots.push(dir.clone());
    }
    let sid = session_id.to_string();

    // G37 起名任务的提前捕获：cancel 在下面的 TurnToken 装配处、model 在会话装配处
    // 都会被 move，这里（读库前）是它们都还活着的最早一段。落库成功后收尾段
    // （Done 发射前）用这几份克隆起一个异步任务生成标题；fresh 那份在它创建处克隆。
    let namer_model = model.clone();
    // `guard().clone()` 解出 `Option<CancellationToken>`（`session_delete` 同一惯用法）。
    // None = 本轮没挂真实令牌（测试场景），届时换一把永不取消的默认令牌——语义正确。
    let namer_cancel: Option<CancellationToken> = guard(&cancel).clone();

    // 读库失败（含 R71 的「某一行解不开」）直接报错，不返回「看起来完整」的历史。
    // G44+：顺带取本会话的工作文件夹（`session_workspace`）—— 工作区级 hooks 住那里的
    // `.lfttdo/hooks.json`。合并进这一次 `spawn_blocking`：多一次往返只为一个 `Option`
    // 不值当，而两者都是「本轮开始前要读的持久化状态」。
    let reader = db.clone();
    let read_sid = sid.clone();
    let ws_sid = sid.clone();
    let (history, workspace) = blocking(move || {
        let msgs = reader.messages(&read_sid).map_err(|e| e.to_string())?;
        // 工作区读失败不该让整轮开不起来（它就是一条「额外来源」的可选路径）：
        // 降级成 `None` = 本轮只跑用户作用域的 hooks。
        let ws = reader.session_workspace(&ws_sid).unwrap_or_default();
        Ok((msgs, ws))
    })
    .await?;

    // 构建器只在这里造一次（②），压缩段与本轮用的是同一个。
    let mut builder = ContextBuilder::new(context);
    // 工具声明不进消息但进请求体：按**真正会发出去的那一批**估算（read_file 在
    // roots 为空时、use_skill 在无启用技能时会被 request() 隐藏），与 request()
    // 共用同一个过滤函数。否则估算虚高 → 校准系数偏低 → 预算越放越宽。
    let roots_for_estimate = effective_roots(&roots);
    // G19：白名单同参传入 —— 预算估算与 `request()` 的声明表共用同一份过滤
    // （算的是全量、发的是过滤后会让估算虚高、校准系数偏低，R286 同理）。
    let tool_defs = crate::agent::session::tool_defs_for_turn(
        &registry,
        &roots_for_estimate,
        skills_dir.as_deref(),
        &skills_disabled,
        allowed_tools.as_deref(),
    );
    builder.set_tool_tokens(crate::agent::context::tool_defs_tokens(&tool_defs));

    // 摘要调用挂在流上的那几秒也必须能被「停止」打到，所以先把**这一只**令牌装进槽位。
    // `AgentSession::new` 自己造令牌（`session.rs:74`）且不接受外来的，只能先装这只、
    // 装配完再换成会话那只，并把已经按下的停止传染过去。
    let compaction = CancellationToken::new();
    let token = TurnToken { slot: cancel };
    token.install(compaction.clone());

    let (history, compacted, saved) = compact_before_turn(
        &db,
        &sid,
        &*model,
        &builder,
        history,
        &memory,
        // 写路径的嵌入客户端与读路径共用同一只(同一来源同一凭据):Arc clone 一份
        // 随压缩段走,本体留给下面的查询嵌入。
        embed.clone(),
        &extract_locks,
        &compaction,
    )
    .await?;
    // ③：先宣布，再答这句。此刻本轮的一行都还没写。
    if let Some(event) = compacted {
        sink.emit(event);
    }
    // P1(spec §12)：「记住了多少」的可见证据。压缩是记忆的主要写入路径，所以这里是
    // 用户唯一能实时看见「它在记」的地方；`saved == 0` 不发（「已记住 0 条」只会让人
    // 以为坏了）。它排在 `ContextCompacted` 之后、本轮回答之前 —— 先说「忘了多少」，
    // 再说「记住了多少」，最后才回答这一句。
    if saved > 0 {
        sink.emit(AgentEvent::MemorySaved { count: saved });
    }

    // ④ 长期记忆注入（spec §5）：画像常驻 + 按本轮消息召回。备注是**易失**的 ——
    // 只进这一次请求的 head，不碰 `history`、不落库，`prior` 量到的仍是纯历史。
    // 检索口径每轮现算：上一轮的召回渗进这一轮等于召回结果跨轮存活。
    let current_text: String = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    // T10(机制一):query 拼上上一轮用户消息。`history` 是压缩后的历史(不含本轮),
    // 所以这里拿到的「上一轮」就是模型上一轮真正见过的最后一句用户话。追问
    // (「那它呢」)因此也能带着上一句的实体去召回;详情见 `recall_query`。
    let query = recall_query(&history, &current_text);
    let db_ro = db.clone();
    // 查询向量(T6):一轮最多嵌一次 —— 嵌的是**上面那个拼好的 query**(含上一轮
    // 上下文),让语义通道与关键词通道看同一份线索。三个 None 形态都在这里
    // 收敛:开关关(零记忆开销,嵌入也不该发)、embed 为 None(off/构造失败)、
    // 嵌入调用失败(warn 后降级)。**没有跨轮状态**,嵌入失败每一次发送都会重试
    // 一次 —— 这是接受的:无状态重试的代价是失败轮多一次失败调用,换来的是
    // 「端点恢复后的下一条消息立刻回到混合召回」,不需要任何缓存失效逻辑。
    // 空白 query(空历史 + 纯图片轮)也不嵌:空串没有可嵌的语义,发了也是端点的错题。
    let query_vec = if memory.enabled {
        match embed.as_ref() {
            Some(provider) if !query.trim().is_empty() => {
                match provider.embed(std::slice::from_ref(&query)).await {
                    Ok(mut v) if v.len() == 1 => Some(v.pop().unwrap_or_default()),
                    Ok(v) => {
                        tracing::warn!(
                            "嵌入端点对查询返回 {} 条向量,本轮退化为纯关键词召回",
                            v.len()
                        );
                        None
                    }
                    Err(reason) => {
                        tracing::warn!("本轮查询向量嵌入失败,退化为纯关键词召回:{reason}");
                        None
                    }
                }
            }
            _ => None,
        }
    } else {
        None
    };
    // 总开关关掉 = 请求头零备注,连召回读库都不做(R69 的另一面:用户关掉的功能不许
    // 还有开销);压缩段那边的 disabled cfg 让抽取同样整段跳过。开启时的三个口径
    // (k、cap、待办窗口)不再吃常量,从 `TurnDeps.memory` 现取 —— 与
    // context/temperature 同一条线。待办提醒也在这一段里:disabled 时既不提醒、
    // 也不标记 `notified_at`(「停用」不许偷偷消费掉本该在开回来时提醒的待办)。
    // (cfg 失去 Copy 后这里把闭包要的字段先拆成局部量,整份 cfg 不进闭包。)
    let notes = if memory.enabled {
        let (k, cap, window, half_life) = (
            memory.recall_k,
            memory.note_token_cap,
            memory.prospective_window_hours,
            memory.fact_half_life_days,
        );
        blocking(move || {
            memory_notes_for_turn(&db_ro, &query, query_vec, k, cap, window, half_life)
        })
        .await?
    } else {
        Vec::new()
    };
    builder.set_memory_notes(notes);

    // ①：这一行必须在压缩段之后，量的才是「restore 回来的那份历史」。
    let prior = history.len();

    // `Done` 的发送权在这一轮归这里：会话进入 defer 模式，等落库成功后再发。
    // 这样「界面显示完成、库里没有这一轮」的窗口不存在（messages 是唯一真相源）。
    let finish_sink = sink.clone();

    // R105：装配期没有 session_id，所以 gate 的出口每轮换一次内核（`gate.rs:77`）。
    gate.set_sink(sink.clone());

    let mut session = AgentSession::new(SessionDeps {
        model,
        registry,
        gate,
        sink,
        context: builder,
        temperature,
    })
    // T26（R276）：**全仓库唯一一处**调用 `AgentSession::with_roots` 的生产代码。
    // 在它之前这一行不存在，于是 `ToolCtx.roots` 恒空、T25 的 `read_file` 对任何路径都
    // fail-closed —— 缺陷不补，切片 3 交付的第二个工具就是死的。
    // T26 修复轮 1（R284）：这里拿的是**有效表**，不是 `TurnDeps.roots` 那份原表。
    // 文件系统依赖的裁剪（绝对性 + canonicalize）从读写两侧搬到了这个使用点，每轮现算一次
    // （R276 的「每轮从 `Settings` 现取、不缓存」因此更硬了：今天的答案只在今天有效）。
    // 过滤仍然**只此一处** —— `settings.rs::effective_roots` 是它的唯一实现，本行是它唯一的
    // 生产调用点；第二处过滤意味着两处答案可以不一致，那正是 R276 把它钉成一条线的原因。
    // 闸门用的 roots 仍然在压缩之后现算（R276 的「每轮现取」语义不变）；
    // 上面那份只是估算工具声明用的快照，不参与授权。
    .with_roots(effective_roots(&roots))
    .with_output_dir(output_dir)
    .with_skills_dir(skills_dir)
    .with_skills_disabled(skills_disabled)
    // G15：plan 模式每轮现取（改设置下一条消息生效）—— 会话里 Write/Exec 级调用被拒、
    // Read 照常，语义归 session.rs（`SessionConfig.plan_mode` / `PLAN_MODE_NOTE`）。
    // `max_turns` 保持默认 25：它是防自我循环的保险丝而不是旋钮（spec 4.6），不进 Settings。
    // G22：agents 档案表同线装配 —— 会话再经 LoopRunner 喂给 task 工具（session.rs）。
    .with_config(SessionConfig {
        plan_mode,
        diagnostics_command,
        allowed_tools,
        active_agent_name,
        agents,
        // G44/G44+：hooks 的用户作用域在数据目录，工作区作用域（`.lfttdo/hooks.json`）
        // 随本轮会话的工作文件夹走。这里只**登记**来源；工作区来源真正的启用闸在
        // 信任表里（`workspace_enabled`，默认关）—— 克隆一个仓库不该让它的 hooks
        // 因为文件里写着 enabled 就武装起来。
        hooks: data_dir.as_ref().map(|dir| {
            let rt = crate::hooks::HookRuntime::user_only(dir);
            match workspace.as_deref() {
                Some(ws) => rt.with_workspace(std::path::Path::new(ws)),
                None => rt,
            }
        }),
        ..Default::default()
    });
    session.defer_done();
    let session_cancel = session.cancel_handle();
    token.install(session_cancel.clone());
    if compaction.is_cancelled() {
        // 停止是按在摘要上的：会话带着同一个事实早退（`session.rs:114` 直接收尾成
        // `Interrupted`，连 user 行都不 push），而不是在这里另发明一套「按了停止还跑不跑」。
        // `fresh` 因此是空的，本轮一个字都不写 —— 库里那一刻还是压缩前的完整历史。
        session_cancel.cancel();
    }
    session.restore(history);

    let reason = session.run(blocks).await;

    let fresh = session.history()[prior..].to_vec();
    // G37：起名任务要的是「本轮新增的那段对话」，与落库的是同一份；落库闭包会
    // move 走 fresh，这里先克隆出起名任务的那份。
    let namer_history = fresh.clone();
    let writer = db.clone();
    let write_sid = sid.clone();
    match blocking(move || append_rows(&writer, &write_sid, &fresh).map_err(|e| e.to_string()))
        .await
    {
        Ok(()) => {}
        Err(reason) => {
            // 落库失败 = 这一轮没有真相。先把界面收口（fatal + Done），再把错误交给
            // 调用方：不能先发 Done 再报错，否则 UI 已经宣称完成而库里什么都没有。
            let message = format!("本轮写入数据库失败：{reason}");
            finish_sink.emit(AgentEvent::Error {
                at: ErrorWhere::Store,
                message: message.clone(),
                fatal: true,
            });
            finish_sink.emit(AgentEvent::Done {
                stop_reason: StopReason::Error,
            });
            return Err(message);
        }
    }

    // 自动改名是**附带**行为：它失败不能让一次已经落库的发送整体失败
    // （否则前端会回滚用户气泡，而这一轮其实已经存下来了）。
    //
    // G37：命名从 R104 的「截断首条消息」升级为「模型生成标题」（summarize::collect_title，
    // 与摘要同一套收流/清洗/无温度）。生成放在 Done 之后的独立任务里跑：本地端点一次
    // 调用要数秒，收尾不该等它。取消令牌沿用本轮的 cancel —— 用户按停，未完成的起名
    // 随之取消，不与「用户已不在场」的事实打架。模型失败或清洗为空时，降级回 R104 的
    // 首条消息截断路径（fallback_title）；降级也拿不到文本就保持「新对话」，只进日志。
    let namer_db = db.clone();
    let namer_sid = sid.clone();
    // namer_model / namer_cancel / namer_history 在本轮靠前处捕获（cancel/model 之后
    // 被 move、fresh 在落库闭包处被 move）。
    tokio::spawn(async move {
        let namer_cancel = namer_cancel.unwrap_or_default();
        // R104 语义原样保留：只对仍是默认「新对话」的会话起名，用户起的名字永不覆盖。
        // 判断走 list_sessions：Db 没有按 id 取标题的入口，而 Plan A 的会话数量级
        // 不值得为它加一条 SQL（一条只服务于这里的读法，就是给「谁拥有标题」
        // 再造一个答案）。
        let checker = namer_db.clone();
        let check_sid = namer_sid.clone();
        let still_default = blocking(move || {
            checker
                .list_sessions()
                .map(|ls| {
                    ls.iter()
                        .any(|s| s.id == check_sid && s.title == DEFAULT_TITLE)
                })
                .map_err(|e| e.to_string())
        })
        .await;
        let still_default = match still_default {
            Ok(v) => v,
            Err(reason) => {
                tracing::warn!("会话 {namer_sid} 起名前的标题检查失败，放弃本次起名：{reason}");
                return;
            }
        };
        if !still_default {
            return;
        }
        let generated =
            collect_title(&*namer_model, title_request(&namer_history), &namer_cancel).await;
        let title = match generated {
            Ok(raw) => {
                // 截断归调用方：上限复用 db::TITLE_CHARS，第二份字面量就是第二份真相。
                let cleaned = crate::text::truncate_display(
                    &sanitize_title(&raw),
                    crate::store::db::TITLE_CHARS,
                );
                if cleaned.is_empty() {
                    tracing::warn!("会话 {namer_sid} 模型标题清洗后为空，降级为首条消息截断");
                    fallback_title(&namer_db, &namer_sid).await
                } else {
                    Some(cleaned)
                }
            }
            // 取消优先于失败：用户按了停止，保持原名不算任何一种错误。
            Err(_) if namer_cancel.is_cancelled() => None,
            Err(reason) => {
                tracing::warn!("会话 {namer_sid} 模型起名失败，降级为首条消息截断：{reason}");
                fallback_title(&namer_db, &namer_sid).await
            }
        };
        if let Some(title) = title {
            let writer = namer_db.clone();
            let rename_sid = namer_sid.clone();
            if let Err(reason) = blocking(move || {
                writer
                    .rename_session(&rename_sid, &title)
                    .map_err(|e| e.to_string())
            })
            .await
            {
                tracing::warn!(
                    "会话 {namer_sid} 自动改名落库失败（本轮已落库，不影响发送结果）：{reason}"
                );
            }
        }
    });

    finish_sink.emit(AgentEvent::Done {
        stop_reason: reason,
    });
    Ok(reason)
}

/// R103 落库的唯一入口（一轮的新行按顺序追加）。同步 SQLite，调用方负责 `spawn_blocking`。
/// 事务在 `Db::append_messages` 里：中途失败不会留下半轮历史。
fn append_rows(db: &Db, session_id: &str, rows: &[Message]) -> Result<(), rusqlite::Error> {
    db.append_messages(session_id, rows)
}

/// G37 的降级路径：模型起名不可用时沿用 R104 的「截断首条消息」。
/// 与旧 `auto_title` 的差别只在不做 still_default 检查——调用方在起名任务开头查过，
/// 同一会话的下一轮被 `running` 槽挡着，两次检查之间没有别的写手。
/// 标题文本沿用 `latest_user_message`：它已经截到 `TITLE_CHARS`、并且跳过工具结果
/// 行（R75），在这里再截一遍就是第二份真相（R77 同一条理由）。
async fn fallback_title(db: &Arc<Db>, session_id: &str) -> Option<String> {
    let reader = db.clone();
    let sid = session_id.to_string();
    match blocking(move || reader.latest_user_message(&sid).map_err(|e| e.to_string())).await {
        Ok(Some(title)) if !title.is_empty() => Some(title),
        _ => None,
    }
}

/// 装配层的 key 闸门：`None` 时究竟是拒绝还是放行，判据只有一个 —— 端点是不是回环。
///
/// 为什么这道闸住在装配层而不是 provider 里：`OpenAiCompat::new` 早就接受空 `api_key`
/// （`new_rejects_blank_base_url_but_not_blank_api_key` 钉着），拦住本机模型的一直是
/// 这里那句 `ok_or_else`。反过来，provider 也不该拥有「要不要凭据」这个决定 —— 它只是
/// 把 key 塞进 header，配不配是设置与端点的事。
///
/// 判据**只能窄到回环**：放错的代价是把一个无凭据的推理端点交给任意网络（SSRF 同一条
/// 红线）。非回环分支保持与抽函数之前逐字一致，包括「`Some("")` 也照样放行」这一条 ——
/// 那是既有行为，收紧它是另一件事，不在本轮范围。
fn documents_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().document_dir().ok()
}

fn resolve_storage_root(
    app: &AppHandle,
    settings: &Settings,
    data_dir: &Path,
) -> Result<PathBuf, String> {
    crate::workspace::resolve_storage_root(
        &settings.storage_dir,
        data_dir,
        documents_dir(app).as_deref(),
    )
}

fn resolve_session_output(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    session_id: &str,
) -> Result<PathBuf, String> {
    if !state
        .db
        .session_exists(session_id)
        .map_err(|e| format!("读取会话失败：{e}"))?
    {
        return Err("会话不存在".to_string());
    }
    let workspace = state
        .db
        .session_workspace(session_id)
        .map_err(|e| format!("读取工作文件夹失败：{e}"))?;
    let storage = resolve_storage_root(app, settings, &state.data_dir)?;
    crate::workspace::session_output_dir(workspace.as_deref(), &storage, session_id)
}

fn resolve_api_key(api_key: Option<String>, base_url: &str) -> Result<String, String> {
    match api_key {
        Some(key) => Ok(key),
        None if is_loopback_endpoint(base_url) => Ok(String::new()),
        None => Err("尚未配置 API key，请在设置中填写".to_string()),
    }
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
    blocks: Vec<ContentBlock>,
) -> Result<(), String> {
    let _running = claim_running(&state.running, &session_id)?;

    let store = state.settings.clone();
    let settings = blocking(move || store.load().map_err(|e| e.to_string())).await?;
    // Agent Shell（spec §2「生效时机」）：每轮开头用**本轮现取**的设置刷 shell catalog，
    // 与下面的 roots / temperature 同一条线 —— 用户改完设置发下一条消息就生效，
    // 没有任何缓存可以过期（轮内不变：正在流式的那一轮不换后端）。
    // 锁走 `crate::lock` 的中毒容忍入口；`ShellTool` 持同一个 Arc，本轮 run_turn 里的
    // 每次执行看到的都是这份。这一行没法单测（`#[tauri::command]` 在单测里调不动），
    // 「值真的可解析」由 `shell_catalog_of` 的纯函数测试与 settings 侧 normalize 钉着。
    *crate::lock::write(&state.shells) = shell_catalog_of(&settings);
    // G13：工具级权限规则（`settings.permission_rules` → gate）。生效时机与上面的 shell
    // catalog、下面的 roots 同一条线 —— 每轮开头现读设置，**改完设置发下一条消息就生效**，
    // 不做实时热更（轮内不变：正在跑的那一轮不换规则表）。
    // `None`（没配过 / 已清空）时清空规则表，回到默认路径；形状非法时
    // `set_rules_from_json` 报 Err，这里降级为 warn + 清空 —— fail-safe：规则缺失只会
    // 回到「按默认问人」，绝不静默沿用上一轮的旧规则，也绝不让一条配错的规则拦停整条消息。
    // 这一段没法单测（`#[tauri::command]` 在单测里调不动，同上面 shell catalog 那行）：
    // 解析/替换/Err 语义由 `gate.rs` 的内联测试钉着，端到端行为由 `tests/gapcheck2.rs`
    // 的 G13 验收测试钉着。
    let mut rules_warning: Option<String> = None;
    match &settings.permission_rules {
        Some(v) => {
            if let Err(reason) = state.gate.set_rules_from_json(v) {
                tracing::warn!("permission_rules 形状非法，本轮忽略并清空规则：{reason}");
                state.gate.rules().clear_tool_rules();
                // 债④（2026-11-09）：只写日志用户永远不知道自己写的规则没生效。
                // 文案暂存，等 emitter 就绪后发**非致命** error —— 前端进通知条，
                // 「规则被忽略」在界面上可见（不拦停消息，fail-safe 语义不变）。
                rules_warning = Some(format!(
                    "permission_rules 形状非法，已忽略并清空（{reason}）；修正设置后下一条消息生效"
                ));
            }
        }
        None => state.gate.rules().clear_tool_rules(),
    }
    // G31：权限模式同规则注入的口径 —— 每轮现取 settings，轮内不变。
    state.gate.set_permission_mode(settings.permission_mode);
    // R90：`normalize_base_url("")` 返回空串，settings.rs 不替用户编一个默认端点，
    // 所以这道守卫必须住在装配层 —— 空 base_url 到这里就停。
    if settings.base_url.trim().is_empty() {
        return Err("尚未配置模型地址，请在设置中填写".into());
    }
    // 先查地址再读凭据：端点没配时压根不该去碰凭据管理器（少一次无关的失败）。
    let key_store = state.settings.clone();
    let api_key = resolve_api_key(
        blocking(move || key_store.api_key().map_err(|e| e.to_string())).await?,
        &settings.base_url,
    )?;

    // 明文 key 只在这一行进 `ModelConfig`：不打日志、不进事件、不回前端（spec:473）。
    // T39：温度从**同一个** `ModelConfig` 里取出来交给会话 —— 这里读一次 `Settings`，
    // `AgentSession` 只收现成的值，配置真相仍然只有一份。
    let model_cfg = settings.to_model_config(&api_key);
    let temperature = model_cfg.temperature;
    let model = Arc::new(OpenAiCompat::new(model_cfg).map_err(|e| e.to_string())?);
    // T6:嵌入客户端与聊天**同源**(同 base_url、同 api_key —— 隐私边界与聊天一致,
    // spec §3.1)。source=="endpoint" 才构造(normalize 白名单保证只有这两种取值);
    // base_url 的非空守卫在上面已经收过。构造失败 warn 后降 None:检索退化为纯关键词,
    // 发送照常(裁定 4)。明文 key 在 drop(api_key) 之前装进客户端,此后无人再持它。
    let embed: Option<Arc<dyn EmbedProvider>> = if settings.memory_embedding_source == "endpoint" {
        match EmbedEndpoint::new(
            &settings.base_url,
            &settings.memory_embedding_model,
            &api_key,
        ) {
            Ok(e) => Some(Arc::new(e)),
            Err(reason) => {
                tracing::warn!("嵌入端点构造失败,记忆检索退化为纯关键词召回:{reason}");
                None
            }
        }
    } else {
        None
    };
    drop(api_key);

    let output_dir = resolve_session_output(&app, &state, &settings, &session_id)?;
    let image_dir = output_dir.join(crate::image_cache::CACHE_SUBDIR);
    let emitter = Arc::new(
        TauriEmitter::new(
            Arc::new(AppPublisher { app: app.clone() }),
            session_id.clone(),
        )
        .with_images_dir(image_dir)
        // G36：失焦时闪一下任务栏（Informational 档）；聚焦中闪是骚扰，所以闸在闭包里。
        .with_notify({
            let app = app.clone();
            Arc::new(move || {
                if let Some(w) = app.get_webview_window("main")
                    && matches!(w.is_focused(), Ok(false))
                {
                    let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
                }
            })
        }),
    );
    // 会话作用域在**真正开跑**之前对齐：配置/凭据失败的那次发送没有开跑，
    // 不该顺手清掉上一轮用户答过的授权。
    enter_session_scope(&state.last_session, &state.gate, &session_id);
    // 债④：规则被忽略的通知在 emitter 就绪后补发（非致命 error → 前端通知条）。
    if let Some(warning) = rules_warning {
        emitter.emit(AgentEvent::Error {
            at: ErrorWhere::Store,
            message: warning,
            fatal: false,
        });
    }
    // G19：激活的 agent 档案。prompt 是**整段替换**（与用户人设同一语义）且
    // **优先于人设**——档案是「这个会话此刻是谁」的完整定义，人设只是它的缺省。
    // tools 白名单交给会话双层执行（声明表过滤 + 循环拦截，见 session.rs）。
    // 生效提示词的解析是纯函数（`Settings::effective_agent_prompt`，settings.rs
    // 单测钉住），这里只做一行装配 —— `#[tauri::command]` 在单测里调不动。
    let profile = settings.active_agent_profile();
    let mut context_settings = settings.clone();
    if let Some(prompt) = settings.effective_agent_prompt() {
        context_settings.system_prompt = prompt;
    }
    let out = run_turn(
        TurnDeps {
            output_dir: Some(output_dir),
            db: state.db.clone(),
            registry: state.registry.clone(),
            gate: state.gate.clone(),
            model,
            sink: emitter.clone(),
            // R100：钳位与预算只有一处定义，手搭 `ContextConfig{..}` 等于绕过 `normalize()`。
            // G19：context_settings = settings.clone() + 激活档案的 prompt 覆盖
            //（未激活时与 settings 相同，行为不变）。
            context: context_settings.to_context_config(),
            // T39：同上，温度的唯一来源是上面那个 `ModelConfig`（同一个 `normalize()` 出来的）。
            temperature,
            // G15：plan 模式与上面各项同一条线 —— 同一份 `store.load()` 现取，改完设置
            // 下一条消息生效（run_turn 经 `with_config` 注入会话；轮内不变）。
            plan_mode: settings.plan_mode,
            // G18：诊断命令同线现取（每轮一次、写在成功落盘之后，见 session.rs）。
            diagnostics_command: settings.diagnostics_command.clone(),
            // G19：agent 白名单与档案名同线现取（未激活 = 两个 None，会话行为不变）。
            allowed_tools: profile.as_ref().and_then(|p| p.tools.clone()),
            active_agent_name: profile.as_ref().map(|p| p.name.clone()),
            // G22：task 可点名的档案表 —— 同一份 `store.load()` 现取，经 `agent_profiles`
            // 映射（只搬 name/prompt/tools，见该函数注释）。空表 = 没有档案可点名，
            // task 的行为与从前一致。
            agents: agent_profiles(&settings),
            extract_locks: state.extracting.clone(),
            // G44：hooks 的配置与信任表在数据目录（会话侧每轮现读）。
            data_dir: Some(state.data_dir.clone()),
            // T26（R276）：roots 和上面两项同源 —— 同一个 `store.load()` 的每一轮现取，
            // 因此**用户改完设置发下一条消息就生效**，这里没有任何缓存可以过期。
            // R284：交的是设置文件里那份**原表**（不再是在读盘时被裁过的），裁剪在
            // `run_turn` 的 `.with_roots(..)` 那一处、每轮一次。
            // 这一行没法单测（`#[tauri::command]` 在单测里调不动，见本模块测试头 R103 段
            // 的说明）：它保证的是「来源是本轮那份 `Settings`」，而「值真的抵达 `ToolCtx`」
            // 由下面的 `run_turn_hands_the_effective_roots_not_the_persisted_list` 钉。
            roots: settings.roots.clone(),
            skills_dir: Some(state.skills_dir.clone()),
            skills_disabled: crate::skills::load_selection(&state.mcp_dir).disabled,
            // T5：记忆四项与上面各项同一条线 —— 同一份 `store.load()` 现取，改完下一条
            // 消息就生效。组装**在 ipc 侧**手写而不做成 `Settings` 的派生方法：settings.rs
            // 是不依赖上层的底层文件（「to_memory_cfg」是让底层认识 ipc 的形状）。
            // T6 两键同线:embedding_endpoint 由白名单归一后的 source 现算(只有两种取值,
            // == "endpoint" 即精确判定),embedding_model 原样透传(normalize 已 trim)。
            memory: memory_turn_cfg(&settings),
            // T6:上面的构造已把「构造失败」化成 None,这里只是搬运。
            embed,
            cancel: state.cancel.clone(),
        },
        &session_id,
        blocks,
    )
    .await;
    // 兜底冲一次。`run()` 的每个出口都发 `Done`，而 `Done` 走「非 delta 先 flush」那一臂
    // （`coalescing_preserves_the_exact_text` 钉着），所以正常路径上这里拿不到东西；
    // 将来谁加一条不发 Done 的出口，半句话也不会凭空消失。
    emitter.flush();
    // 收尾冲完缓冲再把 StopReason 丢掉：它已经由 `done` 事件交过一次，命令返回值里再给一份
    // 就是同一个事实的两个来源（前端该读的始终是事件流）。
    out.map(|_| ())
}

/// 打断当前这一轮。令牌一次性（R63）：取消过的会话算用完，下一轮 `run_turn` 会新建会话、
/// 换新令牌，`TurnToken` 负责把旧的那枚从槽里清掉。
#[tauri::command]
pub fn interrupt(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(token) = guard(&state.cancel).clone() {
        token.cancel();
    }
    Ok(())
}

/// 打开外部链接。前端不直接导航 WebView（那会让应用页面被远程页替换）；
/// 协议白名单与「不经过 shell」的落点都在 [`crate::external_link`]。
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    crate::external_link::open_external(&url)
}

/// 前端把权限决定递回来。`Decision` 的形状由 `events.rs` 里**手写**的 `impl Deserialize`
/// 把关（R98）：极性与携带字段矛盾（例如 `{"allow":false,"scope":"session"}`）时这条命令
/// 压根收不到参数，前端只会拿到 serde 的报错 —— 拒绝不会在入程上变成放行。
#[tauri::command]
pub fn respond(
    state: State<'_, AppState>,
    request_id: String,
    decision: Decision,
) -> Result<bool, String> {
    // 返回 false = pending 里没有这个 id（已经答过、或那一轮已经收场），前端据此收起弹窗。
    Ok(state.gate.resolve(&request_id, decision))
}

/// G20：前端把用户对 question 多选题的选择递回来。**一行薄壳**：真正的判定
/// （未知 id、通道关闭、一次性应答）在 [`QuestionTool::resolve`] 里且被单测直接
/// 钉住 —— `#[tauri::command]` 在单测里调不动（R103 同款），命令体只许做转发。
/// 返回 false = 没有这道挂起的题（已应答、已取消），前端据此收起提问面板。
/// `state` 按命令族惯例保留（Tauri 注入，前端 invoke 不传它）。
#[tauri::command]
pub fn question_respond(
    _state: State<'_, AppState>,
    id: String,
    choice: String,
) -> Result<bool, String> {
    Ok(QuestionTool::shared().resolve(&id, choice))
}

#[tauri::command]
pub async fn session_list(state: State<'_, AppState>) -> Result<Vec<SessionRow>, String> {
    let db = state.db.clone();
    blocking(move || db.list_sessions().map_err(|e| e.to_string())).await
}

// ───────────────────────── T35：MCP server 管理（四条命令） ─────────────────────────

/// 面向界面的 server 视图。字段**只有**这些：`env`/`headers` 一个字都不带出去
/// （命令返回值会进 DevTools 与日志，与 T29 R294 的脱敏是同一条红线）。
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerView {
    pub name: String,
    /// `connecting` | `ready` | `disconnected`
    pub status: String,
    pub tools: usize,
    /// `auto` | `ask` | `block`
    pub trust: String,
    /// 只有 `disconnected` 才可能有；已经脱敏、可直接上界面（T31 R308 的 200 char 上限）。
    pub reason: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigIssue {
    pub name: String,
    pub reason: String,
}

/// 面板常规加载的配置状态：**不含正文与密钥**。
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigStatusView {
    pub path: String,
    pub exists: bool,
    pub parse_error: Option<String>,
    pub skipped: Vec<McpConfigIssue>,
}

/// 编辑器打开时的快照：**只有这条命令**会把配置正文（含 env/headers）带到前端。
/// 故意不实现 `Debug`：避免正文误入日志。
#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigTextView {
    pub path: String,
    pub exists: bool,
    pub text: String,
    pub parse_error: Option<String>,
    pub skipped: Vec<McpConfigIssue>,
}

fn trust_word(verdict: Verdict) -> &'static str {
    verdict.as_str()
}

/// 线格式 → 信任档位。命令层自己映射（`Verdict` 没有 serde：它不进线格式）。
pub fn parse_trust(trust: &str) -> Result<Verdict, String> {
    Verdict::parse(trust).ok_or_else(|| format!("未知的信任档位 `{trust}`（只认 auto/ask/block）"))
}

/// 设置信任并**落盘**（R329：不落盘的话每次启动都要重新授权一遍）。
/// 写盘失败要把错误交回命令层——静默吞掉等于「用户以为记住了、下次又问」。
pub fn mcp_set_trust_of(
    gate: &PermissionGate,
    dir: &Path,
    name: &str,
    trust: &str,
) -> Result<(), String> {
    let verdict = parse_trust(trust)?;
    gate.rules().set_server_trust(name, verdict);
    // 信任档变了，这一台 server 的会话级旧授权不能继续作数（尤其是改成 Block 时）。
    // 只清这一台：改 MCP 信任不该把内置工具的授权也一起收回。
    gate.rules().clear_session_rules_for_server(name);
    crate::mcp::trust::save(dir, &gate.rules().trust_snapshot())
}

/// 可测的核心（`#[tauri::command]` 在单测里调不动，同 R103）。
pub fn mcp_servers_of(
    defs: &[ServerDef],
    bridge: &McpBridge,
    gate: &PermissionGate,
) -> Vec<McpServerView> {
    let rules = gate.rules();
    defs.iter()
        .map(|def| {
            let name = def.name();
            let (status, tools, reason) = match bridge.status_of(name) {
                Some(ServerStatus::Connecting) => ("connecting".to_string(), 0, None),
                Some(ServerStatus::Ready { tools }) => ("ready".to_string(), tools, None),
                Some(ServerStatus::Disconnected { reason }) => {
                    ("disconnected".to_string(), 0, Some(reason))
                }
                None => ("disconnected".to_string(), 0, None),
            };
            // 没登记过的 MCP server 在 gate 眼里就是 Ask（R321），界面必须照实显示，
            // 不能显示成「auto」让用户误以为已经信任。
            let trust = rules.server_trust(name).unwrap_or(Verdict::Ask);
            McpServerView {
                name: name.to_string(),
                status,
                tools,
                trust: trust_word(trust).to_string(),
                reason,
            }
        })
        .collect()
}

/// 按**精确名字**撤销一台 server 的全部工具（T30 的兜底命名可能截断 server 段，
/// 前缀法摘不到它们）。返回真正被摘掉的名字。
pub fn mcp_clear_tools(
    registry: &ToolRegistry,
    server: &str,
    decls: &[rmcp::model::Tool],
) -> Vec<String> {
    decls
        .iter()
        .filter_map(|decl| {
            let name = crate::mcp::naming::namespaced(server, &decl.name);
            registry.remove(&name).then_some(name)
        })
        .collect()
}

/// 把一台 server 声明的全部工具注册进生产表，并把「注册名 → 真名」喂给 gate 的
/// alias 表（T33）。alias 是 gate 查 per-server 信任档的唯一可靠通路：T30 兜底命名
/// 把注册名的 server 段截到 8 个 char，`scope_of` 从注册名还原不出真名，而信任表
/// 按真名存（`mcp_set_trust` / `mcp_trust.json` 都用真名）。与 [`mcp_clear_tools`]
/// 成对：一个摘（按精确名）、一个挂（挂的同时喂 alias）。
pub fn mcp_register_tools(
    registry: &ToolRegistry,
    gate: &PermissionGate,
    bridge: &Arc<McpBridge>,
    server: &str,
    decls: &[rmcp::model::Tool],
) {
    for decl in decls {
        let tool = McpTool::from_decl(server, decl, bridge.clone());
        gate.rules()
            .set_server_alias(tool.registered_name(), server);
        registry.register(Arc::new(tool));
    }
}

/// 重连一台 server 并把它声明的工具注册进生产表（T35 的 `mcp_reconnect` 的核心）。
/// **先连、后换**：连不上就保持原来那批工具不动（失败不会让可用工具表缩水）。
pub async fn mcp_reconnect_of(
    bridge: &Arc<McpBridge>,
    registry: &ToolRegistry,
    gate: &PermissionGate,
    def: ServerDef,
) -> Result<usize, String> {
    let name = def.name().to_string();
    let old = bridge.decls_of(&name);
    let out = bridge.connect_all(&[def]).await;
    let count = match out.into_iter().next() {
        Some((_, Ok(count))) => count,
        Some((_, Err(reason))) => return Err(reason),
        None => return Err("连接没有返回结果".to_string()),
    };
    mcp_clear_tools(registry, &name, &old);
    mcp_register_tools(registry, gate, bridge, &name, &bridge.decls_of(&name));
    Ok(count)
}

/// 启动时的 MCP 自举：按 `mcp.json` 逐台连接并注册工具。
///
/// 为什么要有它：没有这一步，用户每次打开应用都得先进面板点「重连」，否则模型看不到
/// 任何 MCP 工具——那不是一个成品该有的行为。逐台串行（复用 `mcp_reconnect_of`）而不是
/// `connect_all`：后者会并发起 N 个 npx，首启时同时下载/解压对磁盘和日志都不友好，
/// 而自举本来就在后台，慢几秒没人等它。
pub async fn mcp_bootstrap(
    bridge: &Arc<McpBridge>,
    registry: &ToolRegistry,
    gate: &PermissionGate,
    servers: Vec<ServerDef>,
) -> Vec<(String, Result<usize, String>)> {
    let mut out = Vec::with_capacity(servers.len());
    for def in servers {
        let name = def.name().to_string();
        let result = mcp_reconnect_of(bridge, registry, gate, def).await;
        out.push((name, result));
    }
    out
}

#[tauri::command]
pub fn mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerView>, String> {
    let cfg = crate::mcp::config::load(&state.mcp_dir);
    Ok(mcp_servers_of(&cfg.servers, &state.mcp, &state.gate))
}

#[tauri::command]
pub fn mcp_set_trust(
    state: State<'_, AppState>,
    name: String,
    trust: String,
) -> Result<(), String> {
    mcp_set_trust_of(&state.gate, &state.mcp_dir, &name, &trust)
}

#[tauri::command]
pub async fn mcp_reconnect(state: State<'_, AppState>, name: String) -> Result<usize, String> {
    let cfg = crate::mcp::config::load(&state.mcp_dir);
    let def = cfg
        .servers
        .into_iter()
        .find(|def| def.name() == name)
        .ok_or_else(|| format!("配置里没有 `{name}`"))?;
    mcp_reconnect_of(&state.mcp, &state.registry, &state.gate, def).await
}

#[tauri::command]
pub async fn mcp_disconnect(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let decls = state.mcp.decls_of(&name);
    state.mcp.disconnect(&name).await;
    mcp_clear_tools(&state.registry, &name, &decls);
    Ok(())
}

fn config_issues(skipped: Vec<crate::mcp::config::Skipped>) -> Vec<McpConfigIssue> {
    skipped
        .into_iter()
        .map(|s| McpConfigIssue {
            name: s.name,
            reason: s.reason.to_string(),
        })
        .collect()
}

/// 可测核心：配置状态（不含正文）。
pub fn mcp_config_status_of(dir: &Path) -> Result<McpConfigStatusView, String> {
    let st = editor::status(dir);
    Ok(McpConfigStatusView {
        path: st.path.display().to_string(),
        exists: st.exists,
        parse_error: st.parse_error,
        skipped: config_issues(st.skipped),
    })
}

/// 可测核心：编辑器快照（含正文）。只有显式打开编辑器时才会被调用。
pub fn mcp_config_read_of(dir: &Path) -> Result<McpConfigTextView, String> {
    let snap = editor::read(dir)?;
    Ok(McpConfigTextView {
        path: snap.path.display().to_string(),
        exists: snap.exists,
        text: snap.text,
        parse_error: snap.parse_error,
        skipped: config_issues(snap.skipped),
    })
}

/// 可测核心：严格校验后原子写盘。
pub fn mcp_config_save_of(dir: &Path, text: &str) -> Result<(), String> {
    editor::save(dir, text)
}

#[tauri::command]
pub fn mcp_config_status(state: State<'_, AppState>) -> Result<McpConfigStatusView, String> {
    mcp_config_status_of(&state.mcp_dir)
}

#[tauri::command]
pub fn mcp_config_read(state: State<'_, AppState>) -> Result<McpConfigTextView, String> {
    mcp_config_read_of(&state.mcp_dir)
}

#[tauri::command]
pub fn mcp_config_save(state: State<'_, AppState>, text: String) -> Result<(), String> {
    mcp_config_save_of(&state.mcp_dir, &text)
}

/// 技能面板的一行。只用来**看**：技能没有开关，模型按描述自动调用。
#[derive(Debug, Serialize)]
pub struct SkillView {
    pub name: String,
    pub description: String,
    pub folder: String,
    /// G41：参数提示（slash 菜单展示用），没写就是 `None`。
    pub argument_hint: Option<String>,
    /// 选择表里是否启用。停用的技能仍列出来（否则用户没法重新打开）。
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct SkillsView {
    /// 技能目录（数据目录下的 `skills/`），面板原样显示给用户。
    pub root: String,
    pub skills: Vec<SkillView>,
}

/// 面板的真相源就是磁盘：每次打开/刷新都重新扫，不在前端另存一份列表。
pub(crate) fn skills_view_of(dir: &Path, sel: &crate::skills::Selection) -> SkillsView {
    SkillsView {
        root: dir.display().to_string(),
        skills: crate::skills::discover(dir)
            .into_iter()
            .map(|e| SkillView {
                enabled: crate::skills::is_enabled(sel, &e.folder),
                name: e.name,
                description: e.description,
                folder: e.folder,
                argument_hint: e.argument_hint,
            })
            .collect(),
    }
}

#[tauri::command]
pub async fn skills_list(state: State<'_, AppState>) -> Result<SkillsView, String> {
    let dir = state.skills_dir.clone();
    let sel = crate::skills::load_selection(&state.mcp_dir);
    // 扫盘最多读 50 个文件头；放进阻塞池，别占住 IPC 的主线程。
    blocking(move || Ok::<SkillsView, String>(skills_view_of(&dir, &sel))).await
}

/// 启用/停用一个技能。文件夹必须真实存在（面板传回来的那一列），避免选择表里
/// 长出一条谁也对不上的幽灵记录。
pub(crate) fn set_skill_enabled_of(
    skills_dir: &Path,
    data_dir: &Path,
    folder: &str,
    enabled: bool,
) -> Result<(), String> {
    let exists = crate::skills::discover(skills_dir)
        .iter()
        .any(|e| e.folder == folder);
    if !exists {
        return Err(format!("没有这个技能：{folder}"));
    }
    let mut sel = crate::skills::load_selection(data_dir);
    crate::skills::set_enabled(&mut sel, folder, enabled);
    crate::skills::save_selection(data_dir, &sel)
}

#[tauri::command]
pub async fn skills_set_enabled(
    state: State<'_, AppState>,
    folder: String,
    enabled: bool,
) -> Result<(), String> {
    let skills_dir = state.skills_dir.clone();
    let data_dir = state.mcp_dir.clone();
    blocking(move || set_skill_enabled_of(&skills_dir, &data_dir, &folder, enabled)).await
}

/// 打开技能文件夹。路径只来自 `AppState`（装配期定死），不接收任何前端参数。
#[tauri::command]
pub fn skills_open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let dir = state.skills_dir.clone();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建技能目录失败：{e}"))?;
    #[cfg(windows)]
    {
        // 参数走数组、不经 cmd /C：这里没有用户输入，也不该为它开一条命令串通路。
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("打开技能文件夹失败：{e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // 与 Windows 分支同一纪律：参数走数组、不经 shell。`xdg-open` 是 freedesktop 的
        // 标准入口（PATH 解析不含当前目录，没有 CreateProcess 那条搜索序问题）。
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("打开技能文件夹失败：{e}"))?;
        Ok(())
    }
}

#[tauri::command]
pub fn reveal_path(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let settings = state.settings.load().map_err(|e| e.to_string())?;
    let roots = effective_roots(&settings.roots);
    let output_dir = resolve_session_output(&app, &state, &settings, &session_id)
        .unwrap_or_else(|_| PathBuf::new());
    let target = crate::path_reveal::validate_target(&path, &roots, &output_dir)?;
    crate::path_reveal::reveal(&target)
}

#[tauri::command]
pub async fn session_create(state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.clone();
    blocking(move || db.create_session(DEFAULT_TITLE).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn session_rename(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<(), String> {
    let db = state.db.clone();
    blocking(move || db.rename_session(&id, &title).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn session_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // 只取消**这个**会话的那一轮：删 B 不该把 A 正在跑的一轮打断（brief:197 是无条件 cancel）。
    // 刻意不把两个 guard() 写成一条 let-chain：那样会在持有 `running` 锁的去取 `cancel` 锁，
    // 今天没有反向路径所以不会死锁，但锁序不该靠运气，也不该给将来留一个周期。
    let owns_the_running_turn = guard(&state.running).as_deref() == Some(id.as_str());
    if owns_the_running_turn {
        let token = guard(&state.cancel).clone();
        if let Some(token) = token {
            token.cancel();
        }
    }
    let db = state.db.clone();
    blocking(move || db.delete_session(&id).map_err(|e| e.to_string())).await
}

/// G39：重试最后一轮 —— 删掉最后一轮的全部行（截断事务在
/// `Db::truncate_last_user_turn`），把用户原话（含图块）交回去重发。
/// 正在跑的那一轮不许重试（`session_delete` 同一 guard 惯用法，单道就够：
/// 截断与进行中的落库赛跑没有赢家，第二条并发请求会被同一道闸挡下）。
/// `Ok(None)` = 没有可重试的轮（空会话 / 只剩备注与辅助行），前端静默。
#[tauri::command]
pub async fn session_retry_last(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<Vec<ContentBlock>>, String> {
    let owns_the_running_turn = guard(&state.running).as_deref() == Some(id.as_str());
    if owns_the_running_turn {
        return Err("这一轮还在跑，停止后再重试".to_string());
    }
    let db = state.db.clone();
    blocking(move || db.truncate_last_user_turn(&id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn session_messages(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<AgentEvent>, String> {
    let store = state.settings.clone();
    let settings = blocking(move || store.load().map_err(|e| e.to_string())).await?;
    let images_dir = resolve_session_output(&app, &state, &settings, &id)
        .ok()
        .map(|dir| dir.join(crate::image_cache::CACHE_SUBDIR));
    let db = state.db.clone();
    let registry = state.registry.clone();
    blocking(move || {
        let msgs = db.messages(&id).map_err(|e| e.to_string())?;
        // R99：第二参必须是注册表的真查表。写死 `Risk::Read` 会让 Plan B 的写工具
        // 重开后显示「自动允许」，那是界面替用户撒谎（db.rs:329）。
        let mut events = events_from_messages(&msgs, &|name| registry.risk_of(name));
        if let Some(images_dir) = images_dir {
            for event in &mut events {
                crate::image_cache::enrich_event(event, &images_dir);
            }
        }
        Ok(events)
    })
    .await
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SessionOutputView {
    pub workspace: Option<String>,
    pub output_dir: String,
}

#[tauri::command]
pub async fn session_output_view(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionOutputView, String> {
    let store = state.settings.clone();
    let settings = blocking(move || store.load().map_err(|e| e.to_string())).await?;
    let db = state.db.clone();
    let sid = id.clone();
    let workspace = blocking(move || db.session_workspace(&sid).map_err(|e| e.to_string())).await?;
    let output_dir = resolve_session_output(&app, &state, &settings, &id)?;
    Ok(SessionOutputView {
        workspace,
        output_dir: crate::tools::fs_write::display_path(&output_dir),
    })
}

#[tauri::command]
pub async fn session_workspace_set(
    state: State<'_, AppState>,
    id: String,
    path: Option<String>,
) -> Result<(), String> {
    let stored = match path {
        Some(raw) => {
            let path = PathBuf::from(raw.trim());
            if !path.is_absolute() {
                return Err("工作文件夹必须是绝对路径".to_string());
            }
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("工作文件夹无法解析：{}: {e}", path.display()))?;
            if !canonical.is_dir() {
                return Err(format!("工作文件夹不是目录：{}", canonical.display()));
            }
            Some(crate::tools::fs_write::display_path(&canonical))
        }
        None => None,
    };
    let db = state.db.clone();
    blocking(move || {
        db.set_session_workspace(&id, stored.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
}

// ───────────────────────── G44：hooks 的设置面 ─────────────────────────

/// 面板一行：一条声明 + 它的信任状态。`digest` 是信任/撤销的键，前端原样回传。
#[derive(Debug, Serialize)]
pub struct HookView {
    pub event: String,
    pub matcher: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    /// **信任键**：已带作用域前缀（工作区来源是 `ws:<路径摘要>:<digest>`）。
    /// 前端把它原样递回 `hooks_set_trusted`，不需要理解前缀的含义。
    pub digest: String,
    pub trusted: bool,
    /// `"user"` / `"workspace"`。工作区来源是克隆仓库自带的，界面要显著区分。
    pub scope: String,
}

#[derive(Serialize)]
pub struct HooksView {
    /// 用户作用域的 hooks.json 路径（面板显示「改这个文件」）。
    pub config_path: String,
    pub enabled: bool,
    /// hooks.json 存在且解析成功。`false` 时面板要提示「文件有问题，当前不生效」。
    pub config_ok: bool,
    /// 工作区级 hooks 的总开关（信任表里的 `workspace_enabled`，默认 false）。
    /// 关着时 `hooks` 里**不会**出现工作区的条目 —— 未启用的来源连展示都不必，
    /// 免得用户以为勾选一下信任就够了（还要打开这个总开关）。
    pub workspace_enabled: bool,
    /// 本轮会话的工作区（`None` = 这个会话没有工作文件夹，谈不上工作区级 hooks）。
    pub workspace: Option<String>,
    pub hooks: Vec<HookView>,
}

/// 全部 hook 来源的视图。`workspace` 是**当前会话**的工作文件夹：
/// 面板永远为「正在说话的那个会话」展示，与运行时的来源表同一条线。
pub(crate) fn hooks_view_of(data_dir: &Path, workspace: Option<&Path>) -> HooksView {
    let path = data_dir.join(crate::hooks::HOOKS_FILE);
    let config_ok = !path.exists()
        || std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .is_some();
    let trust = crate::hooks::TrustStore::load(data_dir);
    let runtime = match workspace {
        Some(ws) => crate::hooks::HookRuntime::user_only(data_dir).with_workspace(ws),
        None => crate::hooks::HookRuntime::user_only(data_dir),
    };
    let mut hooks = Vec::new();
    for source in &runtime.sources {
        // 总开关关着时工作区来源**整段不展示**：让它在界面上不存在，
        // 比列出来再标「未启用」更不容易被误读成「已经检查过了」。
        if source.scope == crate::hooks::HookScope::Workspace && !trust.workspace_enabled {
            continue;
        }
        let cfg = crate::hooks::load_config(&source.config_dir);
        if !cfg.enabled {
            continue;
        }
        for (name, decls) in &cfg.events {
            // 事件名按配置里的原样展示（用户写的）；判定用解析后的枚举。
            let event = match name.trim().to_ascii_lowercase().as_str() {
                "sessionstart" => crate::hooks::HookEvent::SessionStart,
                "pretooluse" => crate::hooks::HookEvent::PreToolUse,
                "posttooluse" => crate::hooks::HookEvent::PostToolUse,
                "stop" => crate::hooks::HookEvent::Stop,
                _ => continue, // 未知事件名：面板不展示（也不执行 —— decls() 同样查不到）
            };
            for d in decls {
                let digest = d.digest(event);
                // 信任键带作用域前缀：同一个 hook 在两个来源里是两条独立的信任。
                let key = format!("{}{}", source.trust_prefix, digest);
                hooks.push(HookView {
                    event: event.as_str().to_string(),
                    matcher: d.matcher.clone(),
                    command: d.command.clone(),
                    args: d.args.clone(),
                    timeout_ms: d.timeout_ms.unwrap_or(crate::hooks::DEFAULT_TIMEOUT_MS),
                    trusted: trust.is_trusted(&key),
                    digest: key,
                    scope: source.scope.as_str().to_string(),
                });
            }
        }
    }
    HooksView {
        config_path: path.display().to_string(),
        enabled: crate::hooks::load_config(data_dir).enabled,
        config_ok,
        workspace_enabled: trust.workspace_enabled,
        workspace: workspace.map(|w| w.display().to_string()),
        hooks,
    }
}

#[tauri::command]
pub async fn hooks_list(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<HooksView, String> {
    let dir = state.data_dir.clone();
    let db = state.db.clone();
    blocking(move || {
        // 工作区跟着会话走：面板为「正在说话的那个会话」展示。会话 id 缺省/查不到
        // 就给 None（只展示用户作用域），不编一个默认工作区。
        let ws = session_id
            .as_deref()
            .and_then(|sid| db.session_workspace(sid).ok().flatten())
            .map(std::path::PathBuf::from);
        Ok(hooks_view_of(&dir, ws.as_deref()))
    })
    .await
}

/// 信任/撤销一条 hook。`trusted: true` = 记进信任表，`false` = 撤销。
/// digest 由前端从 `hooks_list` 原样拿回来（含作用域前缀）—— 它绑定的是**当前**
/// 声明内容；用户改了 hooks.json 之后旧 digest 自然不再匹配新声明（回到未信任）。
#[tauri::command]
pub async fn hooks_set_trusted(
    state: State<'_, AppState>,
    digest: String,
    trusted: bool,
) -> Result<(), String> {
    let dir = state.data_dir.clone();
    blocking(move || {
        let mut store = crate::hooks::TrustStore::load(&dir);
        if trusted {
            store.grant(&digest);
        } else {
            store.revoke(&digest);
        }
        store.save(&dir)
    })
    .await
}

/// 打开工作区级 hooks 的总开关（默认关）。
///
/// **为什么它是一条独立命令、而不是一条信任 checkbox**：这个开关放开的是一整个
/// 来源（克隆仓库自带的那份配置），风险量级与「信任某一条声明」不同 —— 单独一步
/// 动作、单独一句提示，用户按下去的时候知道自己在放开什么。
#[tauri::command]
pub async fn hooks_set_workspace_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let dir = state.data_dir.clone();
    blocking(move || {
        let mut store = crate::hooks::TrustStore::load(&dir);
        store.workspace_enabled = enabled;
        store.save(&dir)
    })
    .await
}

/// 在资源管理器里打开 hooks 所在目录（用户去改 hooks.json）。
/// 目标固定为**应用自己的数据目录**或**当前会话的工作区**，不是前端给来的任意路径
/// —— `workspace` 只在它是当前会话的工作文件夹时才被接受（由命令自己查库确认），
/// 不走 `reveal_path` 那套 roots 校验的理由与数据目录相同：路径不是模型给的。
#[tauri::command]
pub async fn hooks_open_folder(
    state: State<'_, AppState>,
    session_id: Option<String>,
    workspace: Option<bool>,
) -> Result<(), String> {
    let dir = state.data_dir.clone();
    let db = state.db.clone();
    blocking(move || {
        // `workspace: true` 时打开 `.lfttdo/`；目录不存在就先建出来 —— 用户点
        // 「打开」的语义是「我要去写那个文件」，让他面对一个不存在的目录没有意义。
        if workspace == Some(true) {
            let ws = session_id
                .as_deref()
                .and_then(|sid| db.session_workspace(sid).ok().flatten())
                .ok_or("当前会话没有工作文件夹，谈不上工作区级 hooks")?;
            let target = std::path::Path::new(&ws).join(crate::hooks::WORKSPACE_STATE_DIR);
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("创建 {} 失败：{e}", target.display()))?;
            return crate::path_reveal::reveal(&target);
        }
        crate::path_reveal::reveal(&dir)
    })
    .await
}

/// G43：@文件提及的浮层数据。搜索范围 = 全局 roots + 当前会话的输出目录
/// （`resolve_session_output` 的既有裁定：工作文件夹优先于默认储存位置）。
/// 文件名 / 相对路径子串匹配、浅层优先、上限 [`crate::tools::file_search::DEFAULT_LIMIT`]。
/// 搜索逻辑与剪枝规则在 `tools/file_search.rs`（纯函数、独立单测），这里只装配范围。
#[tauri::command]
pub async fn session_search_files(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    query: String,
) -> Result<Vec<String>, String> {
    let store = state.settings.clone();
    let settings = blocking(move || store.load().map_err(|e| e.to_string())).await?;
    let mut roots = settings.roots.clone();
    if let Ok(dir) = resolve_session_output(&app, &state, &settings, &session_id) {
        roots.push(dir);
    }
    blocking(move || {
        Ok(crate::tools::file_search::search(
            &roots,
            &query,
            crate::tools::file_search::DEFAULT_LIMIT,
        ))
    })
    .await
}

// ───────────────────────── G38：会话导出为 Markdown ─────────────────────────

/// 导出里一个块的一行（`None` = 这个块不值得导出）。文本块原样、图片占位、
/// 工具调用与结果折成一行引用 —— 导出是给人存档/转发的，完整输出留在库里。
/// 工具结果预览走 `ToolOutput::preview()`（R77：预览全仓库只此一份实现），
/// 导出再造一份截断就是第二份口径。
fn export_block(b: &ContentBlock) -> Option<String> {
    match b {
        ContentBlock::Text { text } if text.trim().is_empty() => None,
        ContentBlock::Text { text } => Some(text.clone()),
        ContentBlock::Image { mime, .. } => Some(format!(
            "[图片 {}]",
            match mime {
                crate::agent::types::ImageMime::Png => "png",
                crate::agent::types::ImageMime::Jpeg => "jpeg",
            }
        )),
        ContentBlock::ToolCall { name, input, .. } => {
            // 入参一行化（紧凑 JSON），失败就原样落 Debug —— 导出不因坏 JSON 而丢行。
            let flat = serde_json::to_string(input).unwrap_or_else(|_| format!("{input:?}"));
            Some(format!("> 🔧 调用 `{name}`：`{flat}`"))
        }
        ContentBlock::ToolResult {
            is_error, output, ..
        } => {
            let mark = if *is_error { "⚠" } else { "↳" };
            Some(format!("> {mark} {}", output.preview()))
        }
    }
}

/// G38：把一个会话转成 Markdown。标题 + 每条消息一节。
/// 角色标题自备（导出格式不是线上协议）；System 行保留为一节 —— 它是压缩摘要
/// 的落库形状（`summarize.rs`），跳过它导出的对话就缺了被压缩掉的那段背景。
pub fn messages_to_markdown(title: &str, msgs: &[Message]) -> String {
    let heading = |role: &Role| match role {
        Role::User => Some("## 🧑 用户"),
        Role::Assistant => Some("## 🤖 助手"),
        Role::System => Some("## 📋 摘要"),
    };
    let mut out = format!("# {title}\n\n> 导出自 LFTTDO\n");
    for m in msgs {
        let Some(head) = heading(&m.role) else {
            continue;
        };
        let body: Vec<String> = m.blocks.iter().filter_map(export_block).collect();
        if body.is_empty() {
            continue;
        }
        out.push_str(&format!("\n{head}\n\n"));
        for line in body {
            out.push_str(&line);
            out.push('\n');
            out.push('\n');
        }
    }
    out
}

/// 导出建议文件名：Windows 文件名里的非法字符（`\/:*?"<>|`）与结尾的空白/点
/// 一律换成 `_` —— 换而不是剔除，标题的可读性保得住，路径也不会非法。
pub fn sanitize_export_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_end_matches([' ', '.']);
    if trimmed.is_empty() {
        "导出会话".to_string()
    } else {
        trimmed.to_string()
    }
}

/// G38：导出一个会话为 `.md`。用户在保存对话框里取消 = `Ok(None)`（不是错误）。
#[tauri::command]
pub async fn session_export(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<String>, String> {
    let db = state.db.clone();
    let (title, md) = blocking(move || {
        let title = db
            .list_sessions()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|s| s.id == session_id)
            .map(|s| s.title)
            .ok_or_else(|| "会话不存在".to_string())?;
        let msgs = db.messages(&session_id).map_err(|e| e.to_string())?;
        if msgs.is_empty() {
            return Err("这个会话还没有可导出的内容".to_string());
        }
        // 先借 title 渲染、后把它移进返回元组：元组元素从左到右求值，写反就是
        // 「move 之后借」的编译错。
        let md = messages_to_markdown(&title, &msgs);
        Ok((title, md))
    })
    .await?;
    let picked = blocking(move || {
        Ok::<_, String>(
            rfd::FileDialog::new()
                .add_filter("Markdown", &["md"])
                .set_file_name(format!("{}.md", sanitize_export_filename(&title)))
                .save_file(),
        )
    })
    .await?;
    let Some(path) = picked else {
        return Ok(None);
    };
    std::fs::write(&path, md.as_bytes()).map_err(|e| format!("写入失败：{e}"))?;
    Ok(Some(crate::tools::fs_write::display_path(&path)))
}

#[tauri::command]
pub async fn pick_folder() -> Result<Option<String>, String> {
    let picked = blocking(|| Ok::<_, String>(rfd::FileDialog::new().pick_folder())).await?;
    Ok(picked.map(|path| crate::tools::fs_write::display_path(&path)))
}

// ───────────────────────── T5：长期记忆的审计与清空（设置页） ─────────────────────────

/// 设置页审计视图的一条：字段与 `MemoryRow` **同名同序** + 计算字段 `valid`。
/// 序列化走 snake_case（与 `SessionRow` 的线上惯例一致，前端镜像按这个键名读）。
/// `valid` 在这里算好而不是让前端拿 `invalid_at` 与当前时间比：判据就是
/// 「invalid_at 是否为空」（Graphiti 失效不删除），与当前时间无关。
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct MemoryView {
    pub id: i64,
    pub session_id: Option<String>,
    pub kind: String,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub object: Option<String>,
    pub body: String,
    pub confidence: f64,
    pub valid_from: i64,
    pub invalid_at: Option<i64>,
    pub updated_at: i64,
    pub valid: bool,
    /// T12:同一事实被再次确认的次数(设置页每行显示「确认 N 次」)。
    /// 原样来自 `MemoryRow.reinforcements` —— 检索排序里的「强度」就是它。
    pub reinforcements: i64,
    /// T12:待办的到期时刻(ms),非 `prospective` 或没解析出时间时是 `None`。
    /// 原样透传,前端只负责按本地时区格式化日期。
    pub due_at: Option<i64>,
    /// T12:`due_at` 非空且 `<= now`。**在 Rust 侧现算**而不是让前端拿 `due_at` 与
    /// `Date.now()` 比:比较口径只能有一处,前端复算第二遍就会在时钟/时区上分叉。
    /// 已失效的记忆同样按这个口径算 —— 设置页要能看见「这条待办已经过期且已失效」。
    pub is_due: bool,
    /// P0(spec §12):这一行是**用户亲手改过**的。设置页据此打「已编辑」标记,
    /// 也让用户知道「模型以后不会改写它」—— 这条标记是那句话的唯一证据。
    pub user_edited: bool,
}

/// 行 → 视图的唯一映射点（镜像 `memory_from_row` 的解码纪律：同名同序只此一处）。
impl From<MemoryRow> for MemoryView {
    fn from(r: MemoryRow) -> Self {
        // 先把判据算成局部量再搬字段:两个字段都要用 `r.due_at`,而结构体字面量里
        // 先把 `r` 的某个字段 move 走之后就不能再读它了(这里的 `Option<i64>` 是 Copy,
        // 但把「判据」与「搬运」分成两行,改起来不必再想 move 的顺序)。
        let is_due = r.due_at.is_some_and(|due| due <= Db::now_ms());
        Self {
            id: r.id,
            session_id: r.session_id,
            kind: r.kind,
            subject: r.subject,
            predicate: r.predicate,
            object: r.object,
            body: r.body,
            confidence: r.confidence,
            valid_from: r.valid_from,
            invalid_at: r.invalid_at,
            updated_at: r.updated_at,
            valid: r.invalid_at.is_none(),
            reinforcements: r.reinforcements,
            due_at: r.due_at,
            is_due,
            user_edited: r.user_edited,
        }
    }
}

/// 设置页「逐条查看」：走 `all_memories` 的**审计口径**（含已失效的）——
/// 用户要能看见「程序记过什么、又否定过什么」，只给有效集是替记忆系统隐恶。
#[tauri::command]
pub async fn memories_list(state: State<'_, AppState>) -> Result<Vec<MemoryView>, String> {
    let db = state.db.clone();
    blocking(move || {
        db.all_memories()
            .map(|rows| rows.into_iter().map(MemoryView::from).collect())
            .map_err(|e| e.to_string())
    })
    .await
}

/// 设置页「删除这一条」：物理删（`delete_memory` 的语义）。语义否定（失效）走
/// 记忆管线，不走这里 —— 两个出口、两种不可逆程度（store::memory 的既有裁定）。
#[tauri::command]
pub async fn memories_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let db = state.db.clone();
    blocking(move || db.delete_memory(id).map_err(|e| e.to_string())).await
}

/// 设置页「清空全部记忆」：物理删（含失效行）并把画像归空 —— spec §7 的唯一清空入口。
#[tauri::command]
pub async fn memories_clear(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.clone();
    blocking(move || db.clear_memories().map_err(|e| e.to_string())).await
}

/// 设置页的常驻画像正文（恒单行；从未写过 = 空串，前端据此显示「未生成」）。
#[tauri::command]
pub async fn memory_profile(state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.clone();
    blocking(move || db.profile().map_err(|e| e.to_string())).await
}

/// T16(P0):用户亲手改一条记忆的正文(设置页的编辑权)。
///
/// 走 `update_memory_body_by_user` 而不是模型侧那条:`user_edited=1 + confidence=1.0`
/// 是「用户权威」的存储层表达,模型此后的 UPDATE/INVALIDATE 会被合并管线降级成
/// 「并存」(见 [`plan_memory_writes_from_candidates`]),不再是改写。
#[tauri::command]
pub async fn memory_update_body(
    state: State<'_, AppState>,
    id: i64,
    body: String,
) -> Result<(), String> {
    let db = state.db.clone();
    blocking(move || {
        db.update_memory_body_by_user(id, &body)
            .map_err(|e| e.to_string())
    })
    .await
}

/// T16(P0):用户手动新增一条记忆。返回新行 id(前端可据此高亮这一条)。
///
/// `kind` 白名单:`fact`/`preference`/`prospective`,不认识的一律 **回落 `fact`**,
/// 而不是在这一层报错。理由:用户手写的东西比模型的分类意图更可信 —— 界面传下来的
/// 一个拼错的值不该让「我想记一件事」失败;退一步说,`kind` 只影响检索展示的标签,
/// 正文才是用户真正想留下的东西。
///
/// `due_at`(毫秒,spec §12.5 补的缺口):前端 `datetime-local` 的字面值**按 UTC**
/// 换算后传下来,与抽取侧 `parse_iso8601_ms` 的口径严格一致(少一处换算,同一个时间
/// 就不会在两处显示成不同的时刻)。`None` = 待办但不提醒。
/// 非待办带着值也不会有害:`insert_memory_row` 在插入时丢弃它(唯一判据)。
/// 前端参数键是 `dueAt`(Tauri v2 的 camelCase → snake_case 映射,与本文件
/// `memory_flush_session` 的 `sessionId` 同一惯例)。
#[tauri::command]
pub async fn memory_add(
    state: State<'_, AppState>,
    kind: String,
    body: String,
    due_at: Option<i64>,
) -> Result<i64, String> {
    let db = state.db.clone();
    let kind = normalize_user_memory_kind(&kind);
    blocking(move || {
        db.add_user_memory(kind, &body, due_at)
            .map_err(|e| e.to_string())
    })
    .await
}

/// `memory_add` 的 kind 白名单(唯一判定处)。回落目标是 `fact` —— 最中性的一种,
/// 不会被当成待办弹提醒,也不会污染偏好画像。
pub(crate) fn normalize_user_memory_kind(kind: &str) -> &'static str {
    match kind.trim() {
        "fact" => "fact",
        "preference" => "preference",
        "prospective" => "prospective",
        _ => "fact",
    }
}

/// 会话切换/关闭时的**第二次轻量抽取**(spec §4.1 的「会话收尾」)。
///
/// 与空闲 worker 的区别只有触发源:这里是用户的手势(切走/关掉),worker 是定时器。
/// 核心行为完全共用 [`extract_session_memories`](游标、上限、合并、保护、向量)。
///
/// **所有失败都返回 `Ok(0)`**:切换会话是界面的主路径,它不该因为「记忆抽取失败」
/// 而卡住或弹错 —— 用户在切换,不是在做记忆管理。失败只落日志,下一次空闲 tick
/// 会因为游标没推进而重试。这也是为什么这里连 `Err` 都不往上抛:
/// `#[tauri::command]` 的 `Err` 会变成前端一个可见的失败。
#[tauri::command]
pub async fn memory_flush_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<usize, String> {
    // 同一会话正在跑一轮时不做:**同一段对话被两条路径同时抽**既会烧调用,
    // 也会让两条写路径竞争游标(store 只保证「只前进」,不保证不重复)。
    // 与空闲 worker 的判据同源,共用 `a_turn_is_running`。
    if a_turn_is_running(&state.running, &session_id) {
        tracing::debug!("会话 {session_id} 正在运行,跳过切换时的记忆抽取");
        return Ok(0);
    }
    // 读设置失败 → 后台行为静默放弃(理由见函数文档)。
    let store = state.settings.clone();
    let settings = match blocking(move || store.load().map_err(|e| e.to_string())).await {
        Ok(s) => s,
        Err(reason) => {
            tracing::warn!("切换会话时读取设置失败,跳过记忆抽取:{reason}");
            return Ok(0);
        }
    };
    let memory = memory_turn_cfg(&settings);
    // 记忆总开关关掉 = 连模型都不构造(与压缩路径同一条纪律:关掉的功能不许有开销)。
    if !memory.enabled {
        return Ok(0);
    }
    // base_url 为空是**尚未配置**,不是错误:后台行为不该让切换失败。
    if settings.base_url.trim().is_empty() {
        return Ok(0);
    }
    // 以下装配与 `send_message` 逐段同线(同一份 Settings、同一个 resolve_api_key、
    // 同一个 EmbedEndpoint 构造);差别只在失败一律降级成 Ok(0)。
    let key_store = state.settings.clone();
    let stored_key = match blocking(move || key_store.api_key().map_err(|e| e.to_string())).await {
        Ok(k) => k,
        Err(reason) => {
            tracing::warn!("切换会话时读取 API key 失败,跳过记忆抽取:{reason}");
            return Ok(0);
        }
    };
    let api_key = match resolve_api_key(stored_key, &settings.base_url) {
        Ok(k) => k,
        Err(reason) => {
            tracing::debug!("切换会话时跳过记忆抽取:{reason}");
            return Ok(0);
        }
    };
    let model = match OpenAiCompat::new(settings.to_model_config(&api_key)) {
        Ok(m) => m,
        Err(reason) => {
            tracing::warn!("切换会话时构造模型失败,跳过记忆抽取:{reason}");
            return Ok(0);
        }
    };
    let embed: Option<Arc<dyn EmbedProvider>> = if settings.memory_embedding_source == "endpoint" {
        match EmbedEndpoint::new(
            &settings.base_url,
            &settings.memory_embedding_model,
            &api_key,
        ) {
            Ok(e) => Some(Arc::new(e)),
            Err(reason) => {
                tracing::warn!("切换会话时嵌入端点构造失败,本次抽取不建向量:{reason}");
                None
            }
        }
    } else {
        None
    };
    drop(api_key);

    // 独立令牌:这次抽取不挂在用户那一轮的中断令牌上(那是给前台 turn 的),
    // 否则用户按下停止会顺手取消一次已经提交给端点的后台抽取。
    let cancel = CancellationToken::new();
    match extract_session_memories(
        &state.db,
        &session_id,
        &model,
        &memory,
        embed,
        &state.extracting,
        &cancel,
    )
    .await
    {
        Ok(n) => {
            if n > 0 {
                tracing::info!("会话 {session_id} 切换时抽取了 {n} 条记忆变更");
            }
            Ok(n)
        }
        Err(reason) => {
            tracing::warn!("会话 {session_id} 切换时的记忆抽取失败,游标保持原位:{reason}");
            Ok(0)
        }
    }
}

/// 设置页要的一切。**这里没有任何一个字段承载明文 key**（R83 / spec:473）：
/// 凭据只以掩码形状出这道边界。
#[derive(Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    /// R99：`api_key_masked()` 返回 `Result<Option<String>, SettingsError>`，所以这里是
    /// `Option<String>` 而不是 brief 的 `String`。`None` = 未配置；「读不到」是另一回事，
    /// 由 `Err` 走 IPC 的错误通道（比在 view 里塞第二个字段诚实：前端不可能把两者读成一样）。
    pub api_key_masked: Option<String>,
    /// R102：`load()` 之后**紧接着**取走的「文件坏了、已备份为 X」通知。
    /// 这是 R93 造那个入口的唯一消费者 —— 不在这里取就等于没有。
    /// 不许在 `send_message` 里取：那会把提示从设置页抢走，那里才是该看到的地方。
    pub corrupt_backup: Option<PathBuf>,
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<SettingsView, String> {
    let store = state.settings.clone();
    blocking(move || {
        let settings = store.load().map_err(|e| e.to_string())?;
        // 顺序是 R102 定的：`take_corrupt_backup` 描述的就是「刚才那次 load 的结果」，
        // 中间不许插第二次 load。
        let corrupt_backup = store.take_corrupt_backup();
        let api_key_masked = store.api_key_masked().map_err(|e| e.to_string())?;
        Ok(SettingsView {
            settings,
            api_key_masked,
            corrupt_backup,
        })
    })
    .await
}

#[tauri::command]
pub async fn settings_set(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    if !settings.storage_dir.trim().is_empty() {
        crate::workspace::resolve_storage_root(&settings.storage_dir, &state.data_dir, None)?;
    }
    let store = state.settings.clone();
    let mode = settings.mode;
    blocking(move || store.save(&settings).map_err(|e| e.to_string())).await?;
    // 落盘生效的同一刻把**原生标题栏**一起翻面：网页那份由前端 `applySettingsTheme`
    // 负责（AppSettings 保存后应用），系统画的那一条归这里 —— 两个入口共用
    // `native_theme` 一份映射。取不到窗口 / set_theme 失败都按无害处理：这只是
    // 装饰性的一层，不该把一次已经落盘成功的保存报成失败。
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_theme(Some(native_theme(mode)));
    }
    Ok(())
}

/// key 进来之后只做一件事：交给凭据管理器。不打印、不写日志、不进返回值（spec:473）。
/// 空串按「清除」处理，那是 `set_api_key` 自己的合同（R88）。
#[tauri::command]
pub async fn settings_set_api_key(
    state: State<'_, AppState>,
    api_key: String,
) -> Result<(), String> {
    let store = state.settings.clone();
    blocking(move || store.set_api_key(&api_key).map_err(|e| e.to_string())).await
}

// ───────────────────── Agent Shell：预设表与 profile 校验（spec §5.5） ─────────────────────

/// 检测到的预设表（spec §5.5 `shellPresets()`；2026-11-01 修订：候选 + 检测）。
/// 真相住在 `tools/shell.rs`：候选表 + 检测（`detected_presets()`，spec §2 单一真相，
/// 前端经 IPC 拿只读副本、不抄第二份）。返回的是「本机检测到的那部分候选」——可能
/// 少于 8 条，也可能为空（一台机器可能一条候选都没有；此时 UI 渲染空列表，其余
/// shell 由用户以自定义路径添加，机制不变）。展示顺序仍是 spec §5.2 的冻结顺序
/// （检测只过滤、不重排）。
#[tauri::command]
pub fn shell_presets() -> Vec<ShellProfile> {
    detected_presets()
}

/// 校验的**可测核心**（`#[tauri::command]` 在单测里调不动，同本模块测试头 R103 段的
/// 既有口径，所以抽一层能直接测的纯函数）。顺序是 spec §5.4 的两步分工：
/// 1. `validate_custom` —— 词法（trim、空 id / 空 program / 空-含空白 args 项拒绝），
///    **不做存在性检查**；
/// 2. `ensure_executable` —— 存在 + 可执行判定；裸名按 `search_dirs` 逐目录解析，
///    生产传 `path_search_dirs()`（PATH，Windows 叠 PATHEXT），测试注入假表 ——
///    同 mcp R305 的「解析器可注入」口径。
///    错误是给人读的中文串，`Err` 原样上屏（spec §5.5）。
fn validate_shell_profile_with_dirs(
    profile: ShellProfile,
    search_dirs: &[PathBuf],
) -> Result<(), String> {
    let normalized = validate_custom(profile)?;
    ensure_executable(&normalized.program, search_dirs)?;
    Ok(())
}

/// UI「校验」按钮（spec §5.5 `shellProfileValidate`）：成功 `Ok(())`，失败
/// `Err(中文原因)`。**保存时不校验**（spec §2 决策清单）—— 这条命令只服务显式的校验动作。
#[tauri::command]
pub fn shell_profile_validate(profile: ShellProfile) -> Result<(), String> {
    validate_shell_profile_with_dirs(profile, &path_search_dirs())
}

/// R110：`settings_api_key_masked` 这条命令**不实现** —— `settings_get` 已经带掩码，
/// 再开一个只读掩码的命令没有消费者（R89：加接口先给消费者）。
///
/// 消费者是 T17 的人工验收（R107/R110 都要求版本与日志路径看得见），不是界面，
/// 所以维持 brief 的 `-> String`，不新造一个没人读的 struct。
#[tauri::command]
pub fn runtime_summary(state: State<'_, AppState>) -> String {
    let log = state
        .log_path
        .as_deref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "日志文件未能创建".into());
    format!(
        "LFTTDO {} · plan A · 日志：{log}",
        env!("CARGO_PKG_VERSION")
    )
}

/// raw body 取字节。字节只能克隆：`Request` 借的是 `InvokeMessage` 里的 `InvokeBody`，
/// 而 `blocking` 要 `'static`。T3 实测 512 KB 往返中位 7.5 ms（15/15 全 `Ok`），
/// 这次克隆在预算内，不是未量的开销。
fn take_raw_body(body: &InvokeBody) -> Result<Vec<u8>, String> {
    match body {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        // 不回显 value：这条字符串会进日志，而 body 内容大小不受控。
        InvokeBody::Json(_) => Err(
            "normalize_image 只接受 raw body，收到的是 json body（前端必须是 invoke(\"normalize_image\", bytes)）".into(),
        ),
    }
}

/// Task 18 Step 4：三入口拿到的字节 → `ContentBlock::Image`。
///
/// T3 的两条硬约束（brief:394，实测，非推测）是下面 1、2 两条 —— 它们都是「前端怎么写、
/// 命令体就怎么收，谁也察觉不到谁」的那种，所以只能写在 doc 里让 T19 读得到。
/// 第 3 条不是硬约束，是平台面。
/// 1. **raw 与 JSON 字段互斥** —— 这条命令一旦加第二个具名参数，前端就没法再把它整体当
///    raw body 发。所以 max-edge / mime 这类旋钮只能内置（`image_prep::MAX_EDGE_PX`），
///    或另开一条 JSON 命令。
/// 2. **永远不要把 `subarray` 切出来的那段视图的 `.buffer` 交出去**（brief:394 T3 第 2 条，
///    实测；T19 的前端就靠这一条活着）：若 `bytes` 是一个切过的视图，`bytes.buffer` 带回的是
///    底层**整个** ArrayBuffer 而不是那一段 ⇒ 字节照发、长度照报，命令体这边**察觉不到多收**
///    —— `take_raw_body` 只做「把收到的字节原样克隆出去」（`a_raw_body_is_handed_over_byte_for_byte`
///    钉的就是这个），它没有任何办法知道「前端其实只想发中间那 3KB」。
///    前端要传切片就得把它重建成独立缓冲（`view.slice(0)`，或 `new Uint8Array(view)`），
///    不许 `view.buffer`。
/// 3. `Request` 的文档注释写明 raw body 「on all platforms except Android」。本项目只发
///    Windows，但这是**已知降级面**：真要跨端，兜底是走 JSON 传 base64。
///
/// 归一化是 CPU 活（解码 + 缩放 + 降质循环），而且 R139 之后 `image` 的 `rayon` 是关的，
/// 单张 4000px 全在主线程串行 ⇒ 必须走 `blocking`，不能像 T3 探针那样写同步命令。
#[tauri::command]
pub async fn normalize_image(req: tauri::ipc::Request<'_>) -> Result<ContentBlock, String> {
    let bytes = take_raw_body(req.body())?;
    blocking(move || crate::image_prep::normalize_image(&bytes).map_err(|e| e.to_string())).await
}

/// 日志文件名。和 `settings.json`、`qwen-do.db` 同目录（R107）。
const LOG_FILE: &str = "qwen-do.log";

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join(LOG_FILE)
}

/// 共享一把 `File` 的写出端。
///
/// 为什么不直接 `with_writer(file)`：`fmt` 层的一条日志是**多笔**小 `write`，
/// 而 `MakeWriter` 每次拿到的都是一个新的写入口 —— 两个线程各带各的缓冲，两行日志会互相
/// 插进半行，搜日志的人读到的就是碎句。这里把「一条日志」收进一次 `Mutex` 持有，
/// `append` 模式下不同进程也不会互相盖。
#[derive(Clone)]
struct SharedFile(Arc<Mutex<std::fs::File>>);

impl std::io::Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        guard(&self.0).write(buf)
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        guard(&self.0).write_all(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        guard(&self.0).flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedFile {
    type Writer = SharedFile;

    fn make_writer(&'a self) -> SharedFile {
        self.clone()
    }
}

/// 把日志落到 app data 目录里的文件（R107）。返回路径给 `runtime_summary` 报出去。
///
/// 建不出来（目录权限、磁盘满）时让应用**照常启动**、只把路径报成 `None`：
/// 没有日志很糟，界面起不来更糟。这一支本身也 `warn` 不出去（还没有 subscriber），
/// 所以只能 `eprintln!` —— GUI 里看不见，但也不是异常：真需要用户知道的启动期失败
/// 另有出口，就是 `run()` 末尾那个 `expect`。
fn init_logging(dir: &Path) -> Option<PathBuf> {
    let path = log_path(dir);
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("无法创建日志文件 {}：{e}", path.display());
            return None;
        }
    };
    let writer = SharedFile(Arc::new(Mutex::new(file)));
    // `try_init` 而不是 `init`：重复装配（测试、或将来谁再调一次 `run()`）不该 panic。
    match tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(writer)
        .try_init()
    {
        Ok(()) => Some(path),
        Err(e) => {
            eprintln!("日志 subscriber 已存在，本次不接管：{e}");
            None
        }
    }
}

/// 把 `Monitor::work_area()` 摊平成 `window_state::{sane,pick}` 要的 `(x, y, width, height)`。
/// 之所以在 `ipc.rs` 而不是 `window_state.rs` 里展开：那个模块不许依赖 Tauri
/// （`lib.rs:4-6` 的边界规则），顺带换来它的 6 条测试不需要运行时也不需要接显示器。
fn work_area(monitor: &tauri::Monitor) -> (i32, i32, u32, u32) {
    // `tauri::Monitor` 的字段是 `pub(crate)`，只给访问器（`window/mod.rs:56-101`），
    // 而 `work_area()` 返回的是 `&PhysicalRect`，那个结构体的字段才是公开的。
    let area = monitor.work_area();
    (
        area.position.x,
        area.position.y,
        area.size.width,
        area.size.height,
    )
}

/// 设置里的明暗档（`settings::Mode`）→ 原生窗口主题。标题栏是**系统**画的：
/// tokens/presets/glass 三层 CSS 都管不到它，不点名它就永远停在系统默认的浅色上
/// —— 深色玻璃界面顶上横着一条白（2026-11-08 真机确认）。两个入口共用这一份映射：
/// 窗口创建（`setup`，首帧即对）与 `settings_set`（保存时翻面）；别处不许再写第二份。
fn native_theme(mode: crate::settings::Mode) -> tauri::Theme {
    match mode {
        crate::settings::Mode::Light => tauri::Theme::Light,
        crate::settings::Mode::Dark => tauri::Theme::Dark,
    }
}

/// 标题栏的**精确**着色（2026-11-08，用户的「跟界面一个颜色」）：`.theme()` 只能给
/// 系统的明暗两档灰，跟主题色永远差着一截（dusk 是暖褐，系统暗档是中性灰）。
/// DWMWA_CAPTION_COLOR / DWMWA_TEXT_COLOR（Windows 11 22000+）允许上任意色值。
/// 颜色的唯一真相仍是 `tokens.css`/`presets.css` —— 前端从**计算样式**现读现递，
/// Rust 这里不建第二份色值表。仅 Windows 使用（下游 DwmSetWindowAttribute 同 cfg），
/// Linux 下没有消费者 —— CI 的 Linux clippy 以 dead_code 抓过这一对。
#[cfg(windows)]
const DWMWA_CAPTION_COLOR: u32 = 35;
#[cfg(windows)]
const DWMWA_TEXT_COLOR: u32 = 36;

/// `#rrggbb` → COLORREF（0x00BBGGRR，DWM 的字节序跟 CSS 正好反过来）。
/// 只认六位十六进制：玻璃层的 `--bg-surface` 是带透明度的 `rgb()`，DWM 上不了色；
/// 拒收它，让调用方回落（前端只递解析得了的）。
#[cfg(windows)]
fn colorref_of(hex: &str) -> Result<u32, String> {
    let body = hex.strip_prefix('#').unwrap_or(hex);
    if body.len() != 6 || !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("不是 #rrggbb 颜色：{hex}"));
    }
    let value = u32::from_str_radix(body, 16).map_err(|e| e.to_string())?;
    let (r, g, b) = ((value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff);
    Ok(r | (g << 8) | (b << 16))
}

#[cfg(windows)]
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        attr: u32,
        value: *const core::ffi::c_void,
        size: u32,
    ) -> core::ffi::c_long;
}

/// 把标题栏染成界面自己的颜色（底色 `caption`、标题文字 `text`）。
/// 系统画的那条标题栏跟主题色对不上（见 `native_theme`），而 `.theme()` 只有两档；
/// 这条命令让**前端**把计算样式里现读的色值递进来，DWM 精确上色 —— 色值的
/// 唯一真相仍是 CSS 那两份，Rust 不抄色表。旧系统（Win10 < 22000）没有这两个
/// 属性，DWM 返回非零 HRESULT：原样报 Err，由前端决定吞不吞（它吞，理由在那边）。
#[tauri::command]
pub fn set_caption_color(
    window: tauri::WebviewWindow,
    caption: String,
    text: String,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        // tauri 的 `hwnd()` 把句柄包在 `HWND(...)` 里，Windows 版 windows crate
        // 的裸字段是 `*mut c_void`，DWM 的 ABI 要的是 isize —— 在这里折一下。
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        for (attr, color) in [
            (DWMWA_CAPTION_COLOR, caption.as_str()),
            (DWMWA_TEXT_COLOR, text.as_str()),
        ] {
            let colorref = colorref_of(color)?;
            // SAFETY：DWM 契约里 35/36 号属性各读一个 4 字节的 COLORREF，
            // `&colorref` 恰好提供这么大的一块。
            let hr = unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    attr,
                    &colorref as *const u32 as *const core::ffi::c_void,
                    4,
                )
            };
            if hr != 0 {
                return Err(format!("DwmSetWindowAttribute({attr}) 失败：0x{hr:08x}"));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (window, caption, text);
    Ok(())
}

/// T46b：按存档几何恢复窗口。**任何一步不成立就退回创建时的默认尺寸**，
/// 全程不许 panic —— 一个存坏的 `window.json` 不该有能力让应用打不开。
fn restore_window_geometry(handle: &AppHandle) {
    // 窗口在 `setup` 里由 `WebviewWindowBuilder` 创建（为了 `on_navigation`），
    // 位置就在本函数调用之前；`label` 仍是 "main"，与 capabilities 对得上。
    let Some(window) = handle.get_webview_window("main") else {
        return;
    };
    let Some(dir) = resolve_data_dir(handle) else {
        return;
    };
    let Some(saved) = window_state::load(&dir.join(window_state::STATE_FILE)) else {
        return;
    };
    // 枚举不到任何显示器就**不**恢复：无法证明「某块屏上看得见」的几何比默认尺寸更危险
    // —— 拔掉副屏后重启、窗口落在 x=3000 上就是这么来的。
    // 判定要对着**每一块**屏做，而不是只问 `current_monitor()`：这时窗口还是系统刚按默认位置
    // 摆出来的那块（通常主屏），摆在副屏上的存档会被误判成越界丢掉，症状是「每次重启都从
    // 副屏被拽回主屏」—— 等于这条需求对双屏用户白做。
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let works: Vec<(i32, i32, u32, u32)> = monitors.iter().map(work_area).collect();
    let Some(g) = window_state::pick(saved, &works) else {
        return;
    };
    let restored = window
        .set_size(PhysicalSize::new(g.width, g.height))
        .and_then(|_| window.set_position(PhysicalPosition::new(g.x, g.y)));
    if let Err(e) = restored {
        tracing::warn!("窗口几何未能恢复，留在默认尺寸：{e}");
    }
}

/// 关闭事件发生时把当前几何写回存档；取不到就什么也不写。
fn persist_window_geometry(window: &tauri::Window) {
    // 单窗口应用（spec §6.6）：别的 label 的窗口不该覆盖这份存档。
    if window.label() != "main" {
        return;
    }
    let Some(dir) = resolve_data_dir(window.app_handle()) else {
        return;
    };
    // `Destroyed` 这一臂很可能已经取不到几何了，那是「不写」而不是「写 (0,0,0,0)」：
    // 写一份 zeros 进存档会把一次正常的关闭变成下一次启动要靠 `sane` 兜底的脏数据。
    // 位置取 **outer**、尺寸取 **inner**，这个不对称是被真窗口逼出来的：恢复侧的
    // `set_position` 与 `set_size` 分别按 outer 位置、inner 尺寸生效（tao 的 `resize`
    // 改的是客户区）。两边都用 `outer_size` 时，存档的 1000×800 会被当成客户区尺寸重放，
    // 实测每次启动净增一圈边框 —— 在这台 2560×1600 的屏上是 +22 宽 / +56 高，连开三个
    // 窗口依次 1000×800 → 1022×856 → 1044×912，一路涨到被 `sane` 夹满工作区为止。
    let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    // 最小化状态下关窗，Windows 报的是 (-32000, -32000) 一类的哨兵位置。这里**不加特例**：
    // 照原样写进去，由下次启动的 `sane` 判成越界 ⇒ 回到默认尺寸。
    // 在此识别「现在是 minimized」要多引一层窗口状态查询，换来的只是同一次重启少丢一个位置。
    let geom = WindowGeom {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    if let Err(e) = window_state::save(&dir.join(window_state::STATE_FILE), geom) {
        tracing::warn!("窗口位置未能保存：{e}");
    }
}

/// 生产工具表的**唯一**装配点（Task 27）。
///
/// `skills_dir`）由会话每轮现构的 `ToolCtx` 给（R276 的单一入口）。一旦它开始收参数，「全进程装配一次」
/// 就退化成「每个调用点各装配一次」，而 `R101` 那条「一个目录只许一个 `SettingsStore`」
/// 的教训说的正是这种重复。
///
/// 名字序由 `ToolRegistry` 里的 `BTreeMap` 决定（字典序），**不是**这里的注册顺序；
///
/// Agent Shell（spec §2/§5.5）：`run_command` 在这里注册。catalog 经 `shells` 注入 ——
/// 调用方把**同一个** `Arc` 装进 `AppState`，`send_message` 每轮开头现刷它的内容；
/// 工具不自取设置（「装配层注入、工具不自取」的既有取向）。
pub fn build_production_registry(
    db: Arc<Db>,
    shells: Arc<RwLock<ShellCatalog>>,
) -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register(Arc::new(FetchTool::new()));
    registry.register(Arc::new(GlobTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(ReadFileTool));
    registry.register(Arc::new(UseSkillTool));
    registry.register(Arc::new(MemorySearchTool::new(db)));
    registry.register(Arc::new(WriteFileTool));
    registry.register(Arc::new(EditFileTool));
    // G32：render_html —— Gen UI 第一块砖。Risk::Write（写会话输出目录 rendered/），
    // 前端在工具卡里用 sandbox="" 的 iframe 内嵌渲染（禁脚本，交付物是静态 HTML）。
    registry.register(Arc::new(RenderHtmlTool));
    // G25：experiment 实验循环（autoresearch 精华移植）。Risk::Exec —— 跑任意命令
    // 且直接改 git 状态（keep 提交 / discard 回滚），每次调用都弹窗；
    // base 与 git 都从 ctx.roots/write_root 来，实例状态（last_run）在工具实例上。
    registry.register(Arc::new(ExperimentTool::new()));
    registry.register(Arc::new(ShellTool::new(shells)));
    // G16：task 子代理。runner 由会话循环装配进 ToolCtx（`session::run`）——
    // 注册表只注册工具壳，没有 runner 的执行上下文里它会明确报错（fail-closed）。
    registry.register(Arc::new(TaskTool));
    // G24：workflow 编排。Risk::Write —— 按定义连续派子代理执行任意组合动作
    // （task 的 N 连发），这一下必须弹窗；定义文件在 workspace 的
    // workflows/<name>.json（解析即验 + `{{步骤名}}` 结果传递，见 tools/workflow.rs）。
    registry.register(Arc::new(WorkflowTool));
    registry.register(Arc::new(TodoTool));
    registry.register(Arc::new(UndoTool));
    // G21：websearch。Risk::Read —— 唯一出站目标是写死的 DuckDuckGo 端点（fetch 同款
    // 只读取向）；provider 可注入，网络行为不进 CI（见 tools/websearch.rs 文件头）。
    registry.register(Arc::new(WebSearchTool::new()));
    // G20：question。Risk::Read —— 提问不是执行，出站只有一条事件。注册的是进程级
    // shared() 实例：挂起表必须跨装配共享，否则上一轮挂起的题在这一轮的注册表里
    // 永远等不到应答（见 tools/question.rs 文件头）。
    registry.register(QuestionTool::shared());
    registry
}

/// 这一轮的 shell catalog（spec §2「生效时机」的纯函数面）：从本轮现取的 `Settings`
/// 拼出「自定义表 + 默认 id」，预设侧在本构造点检测（2026-11-01 修订：候选 + 检测，
/// 每轮重测一次）。`send_message` 在 `store.load()` 之后调它并刷进
/// `AppState.shells` —— 与 roots / temperature 同一条「本轮现取、下一条消息生效」线，
/// 这里没有任何缓存可以过期。`default_shell` 的 trim 与空回落由 `from_parts`/`resolve`
/// 的既有语义承担（settings 侧 `normalize()` 已先行回落过一次）。
fn shell_catalog_of(settings: &Settings) -> ShellCatalog {
    ShellCatalog::from_parts(settings.shells.clone(), settings.default_shell.trim())
}

/// 数据目录的唯一解析点：`%APPDATA%\<identifier>`。
/// 配置、会话库、日志、MCP 信任表、窗口几何都跟着它走 —— 一处判定，处处一致。
fn resolve_data_dir(handle: &AppHandle) -> Option<PathBuf> {
    handle.path().app_data_dir().ok()
}

/// 一次空闲 tick 的**全部决策**。worker 只负责「每 60 秒调一次它」,因此这一层
/// 可以在测试里同步驱动:模型由 `model_factory` 注入,不碰 tauri、不碰真实端点。
///
/// 它做的事按顺序是:
/// 1. 记忆总开关关着 → 什么也不做(`0`);
/// 2. 间隔为 0(= 用户明确关闭空闲抽取) → 什么也不做(`0`);
/// 3. 取「最后活动早于 `now - 间隔`、且游标落后于最大消息 id」的会话(id 表,
///    store 侧已按最后活动升序排好);
/// 4. 逐个抽:正在跑一轮的会话跳过(绝不并发同一会话),其余调
///    [`extract_session_memories`],失败只 warn(游标不动,下 tick 重试)。
///
/// 返回**成功处理**的会话数(失败/跳过都不计)。模型惰性构造:一个待抽会话都没有、
/// 或者全被 running 跳过时,不构造客户端、不读 keyring。
// model_factory / built 的 `dyn Fn(&Settings) -> Option<(模型, 嵌入)>` 是刻意的注入形状，
// 收拢为 type alias 属于签名重构，超出本次机械清理范围。
#[allow(clippy::type_complexity)]
pub(crate) async fn idle_extract_tick(
    db: &Arc<Db>,
    settings: &Settings,
    running: &Arc<Mutex<Option<String>>>,
    model_factory: &(
         dyn Fn(&Settings) -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)>
             + Send
             + Sync
     ),
    locks: &ExtractLocks,
) -> usize {
    if !settings.memory_enabled || settings.memory_idle_extract_minutes == 0 {
        return 0;
    }
    let idle_before = Db::now_ms() - settings.memory_idle_extract_minutes as i64 * 60 * 1_000;
    let reader = db.clone();
    let due = match blocking(move || {
        reader
            .sessions_needing_extraction(idle_before)
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(v) => v,
        Err(reason) => {
            tracing::warn!("空闲记忆抽取:查询待抽取会话失败:{reason}");
            return 0;
        }
    };
    if due.is_empty() {
        return 0;
    }
    let memory = memory_turn_cfg(settings);
    let mut built: Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> = None;
    let mut processed = 0usize;
    for sid in due {
        // 「绝不并发同一会话」:同一段对话被前台一轮与后台 tick 同时抽,既烧调用,
        // 也会让两条写路径竞争游标。这是本 worker 存在的唯一硬约束。
        if a_turn_is_running(running, &sid) {
            tracing::debug!("会话 {sid} 正在运行,跳过空闲记忆抽取");
            continue;
        }
        if built.is_none() {
            match model_factory(settings) {
                Some(pair) => built = Some(pair),
                // 没有可用的模型(没配 key、回环端点之外没有 key、构造失败):
                // 这一 tick 的所有会话都不可能有结论,直接收工 —— 每个会话再试一次
                // 只会把同一条 warn 刷 N 遍。
                None => break,
            }
        }
        let (model, embed) = built.as_ref().expect("上一行刚保证它有值");
        // 每个会话一条独立令牌:后台抽取不该被用户按下的「停止」波及(那是前台 turn 的令牌)。
        let cancel = CancellationToken::new();
        match extract_session_memories(db, &sid, &**model, &memory, embed.clone(), locks, &cancel)
            .await
        {
            Ok(_) => processed += 1,
            Err(reason) => {
                tracing::warn!("会话 {sid} 的空闲记忆抽取失败,游标保持原位:{reason}");
            }
        }
    }
    processed
}

/// 后台 tick 的模型装配(与 `send_message`/`memory_flush_session` 同一套:
/// 同一份 Settings、同一个 `resolve_api_key`、同一个 EmbedEndpoint)。
///
/// 这里读 keyring 是**同步**调用,而且发生在 tokio 任务里 —— 这是有意的取舍:
/// 它每 60 秒最多一次、只在真的有待抽会话时发生(惰性构造),一次本地凭据读取
/// 的阻塞量级远小于它要发起的网络调用。为它套一层 `spawn_blocking` 只是把同一段
/// 阻塞挪到另一个线程,不会让 tick 更快。
// 返回的 `(模型, 嵌入)` 元组与 idle_extract_tick 的注入形状同一套，收拢为 type alias 属于
// 签名重构，超出本次机械清理范围。
#[allow(clippy::type_complexity)]
fn build_idle_model(
    settings: &Settings,
    store: &SettingsStore,
) -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
    if settings.base_url.trim().is_empty() {
        return None;
    }
    let stored_key = match store.api_key() {
        Ok(k) => k,
        Err(reason) => {
            tracing::warn!("空闲记忆抽取:读取 API key 失败,本次跳过:{reason}");
            return None;
        }
    };
    let api_key = match resolve_api_key(stored_key, &settings.base_url) {
        Ok(k) => k,
        Err(reason) => {
            tracing::debug!("空闲记忆抽取跳过:{reason}");
            return None;
        }
    };
    let model: Arc<dyn ModelProvider> = match OpenAiCompat::new(settings.to_model_config(&api_key))
    {
        Ok(m) => Arc::new(m),
        Err(reason) => {
            tracing::warn!("空闲记忆抽取:模型构造失败:{reason}");
            return None;
        }
    };
    let embed: Option<Arc<dyn EmbedProvider>> = if settings.memory_embedding_source == "endpoint" {
        match EmbedEndpoint::new(
            &settings.base_url,
            &settings.memory_embedding_model,
            &api_key,
        ) {
            Ok(e) => Some(Arc::new(e)),
            Err(reason) => {
                tracing::warn!("空闲记忆抽取:嵌入端点构造失败,本次不建向量:{reason}");
                None
            }
        }
    } else {
        None
    };
    Some((model, embed))
}

/// 空闲记忆抽取 worker(T15,P0)。
///
/// **这是全程序唯一一个没有用户手势触发的模型调用点**。正因为没有用户在场看着它,
/// 它必须同时满足三条,缺一条都不该存在:
/// (a) **可关闭**:`memory_idle_extract_minutes == 0`(或总开关关)= 一次 tick 都不发;
/// (b) **失败静默**:读设置/读 key/构造模型/抽取的任何失败都只 warn,不弹错、不重试风暴
///     —— 用户没发起这件事,不该被打断;失败的下一个 tick 会因游标没推进而自然重试;
/// (c) **绝不并发同一会话**:`running` 槽里是它就跳过 —— 与前台的写路径互斥。
///
/// 循环形态刻意最简:`sleep(60s)` → 现读设置(用户改了间隔,下一分钟就生效,
/// 不缓存)→ 一次 tick。不用 `tokio::select!`,因为没有任何东西需要与睡眠赛跑:
/// 取消这个 worker 的时机是进程退出,那时整个运行时一起消失。
pub fn spawn_memory_idle_worker(
    app: AppHandle,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    running: Arc<Mutex<Option<String>>>,
    locks: ExtractLocks,
) {
    tauri::async_runtime::spawn(async move {
        // 每 tick 复用的那把按会话锁(与 `AppState.extracting` 是同一份的 clone)。
        let locks = locks;
        // 用一句启动日志把 app 句柄用起来:它是观测入口(哪个应用、什么时候起的),
        // 也是这个函数签名保留 `AppHandle` 的理由 —— worker 本身不碰任何窗口。
        tracing::debug!("空闲记忆抽取 worker 已启动:{}", app.package_info().name);
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            // 每 tick 现读一次设置:间隔旋钮改完,下一次 tick 就该用新值。
            let store = settings.clone();
            let current = match blocking(move || store.load().map_err(|e| e.to_string())).await {
                Ok(s) => s,
                Err(reason) => {
                    tracing::warn!("空闲记忆抽取:读取设置失败,本次跳过:{reason}");
                    continue;
                }
            };
            // 闭包按引用借 `settings`(Arc),tick 结束即释放;不捕获 `current`,
            // 因为 tick 收到的 settings 已经是这一轮的快照。
            let factory = |s: &Settings| build_idle_model(s, &settings);
            let n = idle_extract_tick(&db, &current, &running, &factory, &locks).await;
            if n > 0 {
                tracing::info!("空闲记忆抽取:本次处理了 {n} 个会话");
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 没有任何外部插件：图片附件入口是原生 `<input type="file">`（R144），
        // 前端因此既不需要 dialog 也不需要 fs 权限。曾经的 `tauri_plugin_dialog`
        // 与 capabilities 里的 `dialog:allow-open` 一起在评审 #29 摘掉，理由写在
        // `Cargo.toml` 与 `capabilities/default.json` 里。
        .setup(|app| {
            let dir = resolve_data_dir(app.handle()).ok_or("无法确定数据目录")?;
            std::fs::create_dir_all(&dir)?;
            let skills_dir = dir.join("skills");
            // 建不出来不算失败：发现阶段对不存在的目录就是空表，`use_skill` 随之隐藏。
            let _ = std::fs::create_dir_all(&skills_dir);
            let log_path = init_logging(&dir);
            let db = Arc::new(Db::open(&dir.join("qwen-do.db"))?);
            // R101：一个目录只许存在**一个** `SettingsStore`。它的 `save()` 锁是实例内的
            // （settings.rs 结构体文档 N-3），两个实例同目录并发写那个固定名 temp 实测会失败
            // 20~30/48 次。所以这里 `manage` 一次，命令一律经 `State<AppState>` 取，
            // 绝不在命令里现构。
            let settings = Arc::new(SettingsStore::new(&dir));
            // R329：信任档位在建 gate 之后、manage 之前回灌（文件坏了就是空表 ⇒ 回到 Ask）。
            let gate = Arc::new(PermissionGate::new());
            for (server, verdict) in crate::mcp::trust::load(&dir) {
                gate.rules().set_server_trust(&server, verdict);
            }
            // R105：装配期还没有 session_id，所以 gate 先不带 sink，由 `run_turn` 每轮开头
            // `set_sink(emitter)`（`gate.rs:77` 的可换内核就是为这一刻准备的）。
            // 这里**不留** brief Step 3 的 `NoopSink`：空 sink 意味着 Plan B 的弹窗静默不发事件。
            let mcp = Arc::new(McpBridge::new());
            // 注册表要 clone 一份 db,而 `db` 字段本身会被 move 进 AppState ——
            // 先单独克隆一次避开 E0382(字面量里既 move 又借用不合法)。
            let db_for_registry = db.clone();
            // Agent Shell（spec §5.5）：shell catalog 的共享句柄在装配期建一次，
            // `ShellTool`（经 build_production_registry）与 `AppState.shells` 持同一个 Arc；
            // 装配期占位为空 catalog，第一条消息前 `send_message` 会现刷。
            let shells = Arc::new(RwLock::new(ShellCatalog::new()));
            let shells_for_registry = shells.clone();
            app.manage(AppState {
                db,
                settings,
                registry: build_production_registry(db_for_registry, shells_for_registry),
                gate,
                cancel: Arc::new(Mutex::new(None)),
                running: Arc::new(Mutex::new(None)),
                last_session: Arc::new(Mutex::new(None)),
                log_path,
                // T35：桥在装配期建好（此时不 spawn 任何进程，`connect_all` 才连）；
                // `mcp_dir` 就是 `mcp.json` 所在的 app data 目录。
                mcp,
                mcp_dir: dir.clone(),
                skills_dir,
                data_dir: dir.clone(),
                extracting: ExtractLocks::default(),
                shells,
            });
            // 后台自举：句柄从刚装好的 State 里取（保持装配行仍是唯一一条
            // `registry: build_production_registry()` —— 那条守卫测试盯着它）。
            let managed = app.state::<AppState>();
            let (bootstrap_bridge, bootstrap_registry, bootstrap_dir, bootstrap_gate) = (
                managed.mcp.clone(),
                managed.registry.clone(),
                managed.mcp_dir.clone(),
                managed.gate.clone(),
            );
            // 不挡窗口出现（冷启动 110ms 那条线不受影响），失败只落状态台账。
            tauri::async_runtime::spawn(async move {
                let cfg = crate::mcp::config::load(&bootstrap_dir);
                if cfg.servers.is_empty() {
                    return;
                }
                let _ = mcp_bootstrap(
                    &bootstrap_bridge,
                    &bootstrap_registry,
                    &bootstrap_gate,
                    cfg.servers,
                )
                .await;
            });
            // T15(P0)：空闲记忆抽取 worker。放在 `manage` 之后、几何恢复之前 ——
            // 它要的四个东西（app/db/settings/running）这时都已是托管实例。
            // 它是**唯一一个没有用户手势触发的模型调用点**，所以间隔为 0 时必须
            // 一次都不发（`idle_extract_tick` 的第一道闸），失败只 warn（第二道），
            // 正在跑的会话跳过（第三道）。三条都写在 `spawn_memory_idle_worker` 的文档里。
            spawn_memory_idle_worker(
                app.handle().clone(),
                managed.db.clone(),
                managed.settings.clone(),
                managed.running.clone(),
                managed.extracting.clone(),
            );
            // T46b：几何恢复放在 `manage` 之后、`Ok(())` 之前——`app_data_dir()` 与
            // `create_dir_all` 已经在前头跑过，存档目录这时一定存在。
            // 主窗口从 conf 搬到这里创建：只有 `WebviewWindowBuilder` 能挂
            // `on_navigation`。白名单只放行应用自身来源与开发服务器；Markdown
            // 链接、拖拽、右键菜单等任何想把顶层页面导航到外部站点的路径都会
            // 在这里被拒，页面留在应用里。
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("LFTTDO")
                .inner_size(800.0, 600.0)
                // 标题栏跟着设置里的明暗档走（`native_theme` 的说明）：首帧就是对的，
                // 不用等前端 settings_get 回来再改。load 失败回落 Dark —— 与
                // `Settings::default` 的 mode 同一档，不为一次读盘失败换一条白标题栏。
                .theme(Some(native_theme(
                    managed
                        .settings
                        .load()
                        .map(|s| s.mode)
                        .unwrap_or(crate::settings::Mode::Dark),
                )))
                .on_navigation(crate::web_guard::navigation_allowed)
                .build()?;
            restore_window_geometry(app.handle());
            Ok(())
        })
        // T46b：落盘时机（计划 Step 2 的裁定）。一次拖拽会发来几百个 `Resized`/`Moved`，
        // 全部写盘等于把「记住位置」做成「每移一次写一次磁盘」。**不为此引入定时器去抖**：
        // 那是给「关窗时定时器还在写」新造一类竞态。决定本身抽成纯函数 `should_persist`，
        // 于是这条取舍有测试而不是只有注释。代价如实记在这里：崩溃或强杀会丢窗口位置，
        // 症状是「下一次回到默认尺寸」，不是数据丢失。
        .on_window_event(|window, event| {
            let ev = match event {
                WindowEvent::Resized(_) => Ev::Resized,
                WindowEvent::Moved(_) => Ev::Moved,
                // `CloseRequested` 是带 `api` 字段的变体，而 `WindowEvent` 整个枚举
                // `#[non_exhaustive]`（`app.rs:111`）⇒ 结构体变体要写 `{ .. }`、且必须有兜底臂。
                WindowEvent::CloseRequested { .. } => Ev::CloseRequested,
                WindowEvent::Destroyed => Ev::Destroyed,
                _ => return,
            };
            if should_persist(ev) {
                persist_window_geometry(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            send_message,
            respond,
            question_respond,
            interrupt,
            open_url,
            reveal_path,
            pick_folder,
            session_list,
            session_create,
            session_rename,
            session_delete,
            session_messages,
            session_export,
            session_retry_last,
            session_search_files,
            hooks_list,
            hooks_set_trusted,
            hooks_set_workspace_enabled,
            hooks_open_folder,
            session_output_view,
            session_workspace_set,
            memories_list,
            memories_delete,
            memories_clear,
            memory_profile,
            memory_flush_session,
            memory_update_body,
            memory_add,
            settings_get,
            settings_set,
            settings_set_api_key,
            set_caption_color,
            shell_presets,
            shell_profile_validate,
            normalize_image,
            runtime_summary,
            mcp_servers,
            mcp_set_trust,
            mcp_reconnect,
            mcp_disconnect,
            mcp_config_status,
            mcp_config_read,
            mcp_config_save,
            skills_list,
            skills_open_folder,
            skills_set_enabled,
        ])
        // R108 / T18 m2 —— `capabilities/default.json` 的 `"windows": ["main"]` 按
        // **label** 指名窗口。主窗口现在在 `setup` 里由 `WebviewWindowBuilder` 创建
        // （为了挂 `on_navigation`），label 仍写死为 "main"；两处任一改名都会让权限表
        // 对不上。默认尺寸 800×600 也跟着搬到了那里 —— 没有存档时
        // `window_state::sane` 落到的就是这份默认值。
        // 为什么说明落在 Rust 里而不是 conf 里：conf 由 tauri-build 在编译期解析，
        // JSON 没有合法的注释位，任何注释/自由字段都会让 `cargo check` rc=101。
        .run(tauri::generate_context!())
        .expect("error while running qwen-do");
}

#[cfg(test)]
mod tests;
