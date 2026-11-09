// ipc 层的测试。**从 `ipc.rs` 移出**（2026-11-09）。
//
// 动机是纯可读性：移出前 `ipc.rs` 有 10512 行，其中 6526 行是测试 —— 要读装配代码
// 的人得先翻过一整座测试山。生产代码与测试分开之后，「这个文件有多大」有了两个
// 可以分别回答的答案（3983 行装配 + 本文件测试），谁也不用再为另一方的规模买单。
//
// 为什么可以零成本搬：Rust 的子模块对父模块私有项天然可见，所以下面的
// `use super::*` 一个字都不用改。`ipc.rs` 末尾的 `#[cfg(test)] mod tests;`
// 保证本文件只在测试构建里参与编译（不进 release 产物）。

use super::*;
use crate::agent::types::{Risk, ToolCallView};
use serde_json::json;

// G38：导出格式逐字钉住 —— 用户拿去存档的就是这串字节，格式漂移等于历史变味。
#[test]
fn export_renders_messages_into_a_stable_markdown_shape() {
    let msgs = vec![
        Message::system_note("早期对话摘要：用户在调权限门"),
        Message::user(vec![
            ContentBlock::Text {
                text: "看看这个目录".into(),
            },
            ContentBlock::Image {
                data: "QUJD".into(),
                mime: crate::agent::types::ImageMime::Png,
            },
        ]),
        Message {
            role: Role::Assistant,
            blocks: vec![
                ContentBlock::ToolCall {
                    id: "c1".into(),
                    name: "glob".into(),
                    input: json!({"pattern": "*.rs"}),
                },
                ContentBlock::ToolResult {
                    call_id: "c1".into(),
                    is_error: false,
                    output: crate::agent::types::ToolOutput::text("main.rs"),
                },
                ContentBlock::Text {
                    text: "目录里有 main.rs。".into(),
                },
            ],
        },
    ];
    let md = messages_to_markdown("调权限门", &msgs);
    // 逐段锚点：标题、摘要节、用户文本+图片占位、工具折行、助手结论。
    assert!(md.starts_with("# 调权限门\n\n> 导出自 LFTTDO\n"), "{md}");
    assert!(
        md.contains("## 📋 摘要\n\n早期对话摘要：用户在调权限门\n\n"),
        "{md}"
    );
    assert!(
        md.contains("## 🧑 用户\n\n看看这个目录\n\n[图片 png]\n\n"),
        "{md}"
    );
    assert!(
        md.contains("> 🔧 调用 `glob`：`{\"pattern\":\"*.rs\"}`\n\n"),
        "{md}"
    );
    assert!(md.contains("> ↳ main.rs\n\n"), "{md}");
    // 助手节标题后先跟工具折行，结论在最后。
    assert!(md.contains("## 🤖 助手\n\n> 🔧 调用 `glob`"), "{md}");
    assert!(md.contains("目录里有 main.rs。\n\n"), "{md}");
}

// G38：空正文不产空节；坏 JSON 的入参降级为 Debug 而不是丢行。
#[test]
fn export_skips_empty_blocks_and_degrades_broken_json_input() {
    let msgs = vec![Message {
        role: Role::Assistant,
        blocks: vec![
            ContentBlock::Text { text: "   ".into() },
            ContentBlock::ToolCall {
                id: "c2".into(),
                name: "weird".into(),
                input: json!({"deep": {"x": 1}}),
            },
        ],
    }];
    let md = messages_to_markdown("t", &msgs);
    // 唯一的可见内容是工具行：纯空白文本块不产「## 助手 + 空段」。
    assert!(md.contains("> 🔧 调用 `weird`"), "{md}");
    assert!(!md.contains("## 🤖 助手\n\n\n"), "{md}");
}

// G38：Windows 文件名非法字符与结尾的点/空格；清成空串时给兜底名。
#[test]
fn export_filename_replaces_illegal_characters_and_trailing_dots() {
    assert_eq!(
        sanitize_export_filename("a/b\\c:d*e?f\"g<h>i|j.. "),
        "a_b_c_d_e_f_g_h_i_j"
    );
    assert_eq!(sanitize_export_filename("普通标题"), "普通标题");
    // 「全都不合法」的兜底只在剥掉结尾后真剩空串时触发；??? 会清成 ___（合法文件名）。
    assert_eq!(sanitize_export_filename("???"), "___");
    assert_eq!(sanitize_export_filename("... "), "导出会话");
}

// 标题栏主题只认这份映射（`native_theme` 的说明）：两支各归各位。
// 写反了的症状是「设置里选了浅色、标题栏还是黑的」——一个纯函数就该有测试，
// 而不是等用户在真机上发现。
#[test]
fn native_theme_follows_the_settings_mode() {
    assert_eq!(
        native_theme(crate::settings::Mode::Dark),
        tauri::Theme::Dark
    );
    assert_eq!(
        native_theme(crate::settings::Mode::Light),
        tauri::Theme::Light
    );
}

// COLORREF 的字节序跟 CSS 相反（0x00BBGGRR），写反了的症状是标题栏染成
// 一支莫名的互补色。玻璃层那支带透明度的 rgb() 必须被拒收 —— DWM 上不了色，
// 静默收下等于染出一支谁也没选过的颜色。
#[cfg(windows)]
#[test]
fn colorref_parses_six_digit_hex_into_dwm_byte_order() {
    assert_eq!(colorref_of("#221812"), Ok(0x0012_1822));
    assert_eq!(colorref_of("1a120c"), Ok(0x000c_121a));
    assert!(colorref_of("rgb(255 255 255 / 8%)").is_err());
    assert!(colorref_of("#12345").is_err());
    assert!(colorref_of("#12h456").is_err());
}

/// 一个描述超大的只读工具：只为把工具声明推过预算，不参与调用。
struct BigSchemaTool;

#[async_trait]
impl Tool for BigSchemaTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "big_schema".to_string(),
            description: "a".repeat(16_000),
            input_schema: json!({"type": "object"}),
            risk: Risk::Read,
        }
    }
    async fn execute(
        &self,
        _input: serde_json::Value,
        _ctx: &ToolCtx,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("ok"))
    }
}

/// 测试用的出口：把 payload 记在账上，不发往任何 tauri 对象。
#[derive(Default)]
struct Recorded {
    published: Mutex<Vec<AgentPayload>>,
}

impl Recorded {
    fn frames(&self) -> Vec<AgentEvent> {
        self.published
            .lock()
            .unwrap()
            .iter()
            .map(|p| p.event.clone())
            .collect()
    }
    fn texts(&self) -> Vec<String> {
        self.frames()
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::TextDelta { text } => Some(text),
                _ => None,
            })
            .collect()
    }
    fn count(&self) -> usize {
        self.published.lock().unwrap().len()
    }
}

impl Publish for Recorded {
    fn publish(&self, payload: AgentPayload) {
        self.published.lock().unwrap().push(payload);
    }
}

fn emitter() -> (Arc<TauriEmitter>, Arc<Recorded>) {
    let rec = Arc::new(Recorded::default());
    let e = Arc::new(TauriEmitter::new(rec.clone(), "sess-1".into()));
    (e, rec)
}

fn delta(text: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        text: text.to_string(),
    }
}

#[test]
fn the_emitter_attaches_cached_image_paths() {
    let rec = Arc::new(Recorded::default());
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("images");
    let e = TauriEmitter::new(rec.clone(), "s".into()).with_images_dir(cache.clone());
    e.emit(AgentEvent::ToolResult {
        call_id: "c1".into(),
        is_error: false,
        ms: 1,
        preview: "[图片 png]".into(),
        images: vec![crate::agent::types::ToolImage {
            data: "QUJD".into(),
            mime: crate::agent::types::ImageMime::Png,
            path: None,
        }],
    });
    let images = rec
        .frames()
        .into_iter()
        .find_map(|e| match e {
            AgentEvent::ToolResult { images, .. } => Some(images),
            _ => None,
        })
        .expect("必须有 tool_result 帧");
    let path = images[0].path.as_deref().expect("事件出口必须补图片路径");
    assert!(std::path::Path::new(path).is_file(), "{path}");
    assert!(std::path::Path::new(path).starts_with(cache));
}

/// R106 断言①：30ms 内的多个 delta 合成一帧，且合成后内容一字不差。
#[test]
fn deltas_inside_the_30ms_window_stay_in_one_frame() {
    let (e, rec) = emitter();
    e.emit(delta("你"));
    e.emit(delta("好"));
    e.emit(delta("，世界"));
    assert_eq!(rec.count(), 0, "30ms 内不该有任何一帧落地");
    e.flush();
    assert_eq!(rec.count(), 1, "三段必须合成恰好一帧");
    assert_eq!(rec.texts(), vec!["你好，世界".to_string()]);
}

/// R106 断言②：累计到 512 字符立刻交帧，不等 30ms。
#[test]
fn reaching_512_chars_flushes_immediately() {
    let (e, rec) = emitter();
    let half: String = std::iter::repeat_n('字', 256).collect();
    e.emit(delta(&half));
    assert_eq!(rec.count(), 0, "256 字符还没到上限");
    e.emit(delta(&half));
    assert_eq!(rec.count(), 1, "第 512 个字符必须当场发帧");
    assert_eq!(rec.texts()[0].chars().count(), 512);
}

/// R106 断言③（合帧唯一的正确性约束）：非 TextDelta 之前先把缓冲发出去。
#[test]
fn a_non_delta_event_always_follows_the_buffered_text() {
    let (e, rec) = emitter();
    e.emit(delta("先看一眼"));
    e.emit(AgentEvent::tool_call(ToolCallView {
        id: "c1".into(),
        name: "fs.read".into(),
        risk: Risk::Read,
        input: json!({"path": "a.txt"}),
    }));
    let frames = rec.frames();
    assert_eq!(frames.len(), 2, "缓冲那一帧 + tool_call 那一帧");
    assert_eq!(
        frames[0],
        delta("先看一眼"),
        "文本必须排在 tool_call 之前，否则前端时间线倒序"
    );
    assert!(matches!(frames[1], AgentEvent::ToolCall { .. }));
    // tool_call 之后缓冲必须是空的：不能留半句到下一帧才落地。
    e.flush();
    assert_eq!(rec.count(), 2, "tool_call 不该在缓冲里留下东西");
}

/// R106 断言④：flush 之后缓冲为空 —— 再 flush 不产生空帧。
#[test]
fn flushing_empties_the_buffer_and_never_emits_an_empty_frame() {
    let (e, rec) = emitter();
    e.flush();
    assert_eq!(rec.count(), 0, "空缓冲不许发帧");
    e.emit(delta("一句"));
    e.flush();
    e.flush();
    assert_eq!(rec.count(), 1, "第二次 flush 不该再发一遍");
    assert_eq!(rec.texts(), vec!["一句".to_string()]);
}

/// 时间上限那一支也要有人测：过了 30ms 的下一个 delta 就该把整帧带走。
#[test]
fn the_30ms_threshold_releases_the_frame_on_the_next_delta() {
    let (e, rec) = emitter();
    e.emit(delta("早"));
    assert_eq!(rec.count(), 0);
    std::thread::sleep(Coalescer::MAX_WAIT + Duration::from_millis(20));
    e.emit(delta("到"));
    assert_eq!(rec.count(), 1, "超过 30ms 后下一个 delta 必须当场发帧");
    assert_eq!(rec.texts(), vec!["早到".to_string()]);
}

/// payload 的线上形状（plan:2842 的 `{ sessionId, event }`）。
/// 前端 `commands.ts` 直接读 `p.payload.sessionId`，键名改了就是一片 undefined。
#[test]
fn the_payload_wire_shape_is_session_id_plus_event() {
    let (_e, _) = emitter();
    let v = serde_json::to_value(AgentPayload {
        session_id: "sess-1".into(),
        event: delta("hi"),
    })
    .unwrap();
    assert_eq!(v["sessionId"], "sess-1");
    assert_eq!(v["event"]["t"], "text_delta");
    // 每一帧都自带 session_id：前端靠它把事件路由到对应会话的 reducer。
    let (e2, rec2) = emitter();
    e2.emit(delta("x"));
    e2.flush();
    assert_eq!(rec2.published.lock().unwrap()[0].session_id, "sess-1");
}

/// 非 delta 事件不吞文本、也不被合帧：内容逐字一致，且 `Done` 之后没有半帧残留。
#[test]
fn coalescing_preserves_the_exact_text() {
    let (e, rec) = emitter();
    for p in ["你好", "，", "我是", "Qwen"] {
        e.emit(delta(p));
    }
    e.emit(AgentEvent::Done {
        stop_reason: crate::agent::events::StopReason::EndTurn,
    });
    let frames = rec.frames();
    assert_eq!(frames[0], delta("你好，我是Qwen"));
    assert_eq!(
        frames.last().unwrap(),
        &AgentEvent::Done {
            stop_reason: crate::agent::events::StopReason::EndTurn
        }
    );
    assert_eq!(frames.len(), 2);
}

// ---- R103 / R104：落库只此一家，自动改名只覆盖默认标题 --------------------
//
// 这一组的夹具真的走 `AgentSession::run()`，测的是**生产路径本身**而不是抄出来的
// 一半逻辑：`run_turn` 不碰 tauri（模型是脚本化的、sink 是 `Publish` 的记账实现），
// 所以「重复写历史」这个 brief 里的真 bug 只有在这里才量得出来。

use crate::agent::context::ContextConfig;
use crate::agent::events::StopReason;
use crate::agent::types::{ContentBlock, Message, Role};
use crate::model::provider::{ChatRequest, ModelDelta, ModelError, ModelProvider};
use crate::permission::gate::PermissionGate;
use crate::store::db::{Db, events_from_messages};
use crate::tools::registry::ToolRegistry;
use async_trait::async_trait;
use futures::Stream;
use tempfile::TempDir;

/// 每轮吐一条固定回复的模型。Plan A 的生产工具表是空的，所以不需要工具轮。
///
/// G37 起名请求按**内容分流**：首条 system 是 `TITLE_INSTRUCTION` 的请求固定吐
/// [`TEST_TITLE_REPLY`]，不消耗主脚本队列 —— 起名是落库后异步跑的独立任务，
/// 不分流它就会插队吃掉下一轮的主回复（修复前 6 条端到端测试集体变红的根因）。
struct ReplyModel {
    replies: Mutex<std::collections::VecDeque<String>>,
}

/// 起名请求的固定标题回复（见 [`ReplyModel`] 的分流注释）。
const TEST_TITLE_REPLY: &str = "今天的天气";

#[async_trait]
impl ModelProvider for ReplyModel {
    fn stream<'a>(
        &'a self,
        req: ChatRequest,
    ) -> std::pin::Pin<Box<dyn Stream<Item = Result<ModelDelta, ModelError>> + Send + 'a>> {
        if req.messages.first().is_some_and(|m| {
            m.role == Role::System && m.text_of() == crate::agent::summarize::TITLE_INSTRUCTION
        }) {
            return Box::pin(futures::stream::iter(
                vec![ModelDelta::Text(TEST_TITLE_REPLY.to_string())]
                    .into_iter()
                    .map(Ok),
            ));
        }
        // 取完了就是夹具写错（轮数不够），不许安静地回一条空消息把测试做绿。
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("夹具轮数不够");
        Box::pin(futures::stream::iter(
            vec![ModelDelta::Text(reply)].into_iter().map(Ok),
        ))
    }
}

fn replies(v: &[&str]) -> Arc<dyn ModelProvider> {
    Arc::new(ReplyModel {
        replies: Mutex::new(v.iter().map(|s| s.to_string()).collect()),
    })
}

struct Fixture {
    db: Arc<Db>,
    registry: Arc<ToolRegistry>,
    gate: Arc<PermissionGate>,
    cancel: Arc<Mutex<Option<tokio_util::sync::CancellationToken>>>,
    _dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self {
            db: Arc::new(Db::open(&dir.path().join("qwen-do.db")).unwrap()),
            registry: Arc::new(ToolRegistry::new()),
            gate: Arc::new(PermissionGate::new()),
            cancel: Arc::new(Mutex::new(None)),
            _dir: dir,
        }
    }

    /// 组装一次真实的一轮：走生产函数 `run_turn`，夹具只提供模型与记账 sink。
    async fn turn(
        &self,
        sid: &str,
        model: Arc<dyn ModelProvider>,
        text: &str,
    ) -> (Result<StopReason, String>, Vec<AgentEvent>) {
        let rec = Arc::new(Recorded::default());
        let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.to_string()));
        let reason = run_turn(
            TurnDeps {
                output_dir: None,
                db: self.db.clone(),
                registry: self.registry.clone(),
                gate: self.gate.clone(),
                model,
                sink: sink.clone(),
                context: ContextConfig::default(),
                // 夹具不带温度 = 请求体里不带这个键，与 T39 之前的行为逐字相同
                // （`run_turn` 侧的那条线由 `session.rs` 的
                // `the_session_temperature_reaches_the_chat_request` 钉）。
                temperature: None,
                plan_mode: false,
                diagnostics_command: None,
                allowed_tools: None,
                active_agent_name: None,
                agents: Vec::new(),
                // 夹具默认不带 roots：除了那条专门量 roots 的测试（它自己搭 `TurnDeps`），
                // T27（R286）之后这里还要多说一句：`build_production_registry()` 的表里
                // **有** `read_file`，但空 roots 那一轮交给模型的声明表里没有它，
                // 而夹具的 registry 本来就是空的 —— 两件事各自成立，别混着读。
                // 生产装配点那行由 `the_assembly_wires_the_production_registry` 钉（M4：
                // 把 `app.manage` 换回空表，405 条单测全绿，所以判据只能是源码级守卫）。
                roots: Vec::new(),
                // 夹具不带技能目录：`use_skill` 因此在声明表里被隐藏，
                // 需要它的测试自己往会话上挂一个真的临时目录。
                skills_dir: None,
                skills_disabled: Vec::new(),
                // 夹具用默认记忆 cfg:既有测试量的都不是记忆开关,
                // disabled/自定义口径由 T5 那组测试自己定制。
                memory: MemoryTurnCfg::default(),
                // 夹具默认不带嵌入:embed=None 的行为与 T5 完全一致(纯关键词),
                // 带假嵌入的测试由 T6 那组自己定制。
                embed: None,
                extract_locks: crate::ipc::ExtractLocks::default(),
                data_dir: None,
                cancel: self.cancel.clone(),
            },
            sid,
            vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        )
        .await;
        sink.flush();
        (reason, rec.frames())
    }
}

fn text_rows(msgs: &[Message], role: Role) -> Vec<String> {
    msgs.iter()
        .filter(|m| m.role == role)
        // 工具结果行的 role 也是 User（types.rs:111），按块种类排除。
        .filter(|m| {
            !m.blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
        })
        .map(|m| m.text_of())
        .collect()
}

/// 事件的 serde 标签名，只用来让行序断言失败时能一眼看出「谁排在了谁前面」
/// （直接比 `Vec<AgentEvent>` 的话，失败信息是两大屏结构体字段）。
fn event_tag(e: &AgentEvent) -> &'static str {
    match e {
        AgentEvent::TurnStart { .. } => "turn_start",
        AgentEvent::TextDelta { .. } => "text_delta",
        AgentEvent::UserMessage { .. } => "user_message",
        AgentEvent::ToolCall { .. } => "tool_call",
        AgentEvent::PermissionRequest { .. } => "permission_request",
        AgentEvent::PermissionResolved { .. } => "permission_resolved",
        AgentEvent::Question { .. } => "question",
        AgentEvent::ToolResult { .. } => "tool_result",
        AgentEvent::Usage { .. } => "usage",
        AgentEvent::ContextCompacted { .. } => "context_compacted",
        AgentEvent::MemorySaved { .. } => "memory_saved",
        AgentEvent::WorkflowStep { .. } => "workflow_step",
        AgentEvent::ExperimentRun { .. } => "experiment_run",
        AgentEvent::Error { .. } => "error",
        AgentEvent::Done { .. } => "done",
    }
}

/// R103：落库只能有一个所有者。`run()` 自己会把这条用户轮次 push 进 history，
/// 所以一轮 = 两行（一问一答），**不许**再按块补写、也不许把整段历史重放一遍。
/// brief:152-160 的写法在这里会测出 8 行（2 + 2 + 4）而不是 4 行。
#[tokio::test]
async fn a_second_turn_appends_only_what_the_first_turn_left_out() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let model = replies(&["回复一", "回复二"]);

    let (reason, _) = f.turn(&sid, model.clone(), "第一句").await;
    assert_eq!(reason.unwrap(), StopReason::EndTurn);
    assert_eq!(
        f.db.messages(&sid).unwrap().len(),
        2,
        "一轮之后库里必须恰好两行"
    );

    let (reason, _) = f.turn(&sid, model, "第二句").await;
    assert_eq!(reason.unwrap(), StopReason::EndTurn);
    let msgs = f.db.messages(&sid).unwrap();
    assert_eq!(msgs.len(), 4, "第二轮只许追加两行，整段重放就是重复气泡");
    assert_eq!(
        text_rows(&msgs, Role::User).join("|"),
        "第一句|第二句",
        "一条用户轮次必须恰好一行"
    );
    assert_eq!(text_rows(&msgs, Role::Assistant).join("|"), "回复一|回复二");
}

/// R103 的后半边（T11「两套真相」那条红线）：关闭重开之后，重放出来的
/// 条数与内容和直播一致 —— 一句不多、一句不少。
#[tokio::test]
async fn reopening_a_session_replays_exactly_what_the_live_stream_showed() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let model = replies(&["回复一", "回复二"]);
    let live1 = f.turn(&sid, model.clone(), "第一句").await.1;
    let live2 = f.turn(&sid, model, "第二句").await.1;
    let live_text: String = live1
        .iter()
        .chain(live2.iter())
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    let live_turn_starts = live1
        .iter()
        .chain(live2.iter())
        .filter(|e| matches!(e, AgentEvent::TurnStart { .. }))
        .count();
    assert_eq!(live_turn_starts, 2, "两次发送各一个 turn_start");

    // 重开：换一个 Db 实例读同一个文件，等价于应用重启后重新打开会话。
    let path = f._dir.path().join("qwen-do.db");
    let reopened = Db::open(&path).unwrap();
    let msgs = reopened.messages(&sid).unwrap();
    // 注册表夹具为空：`risk_of` 只影响徽章，这里要的是「第二参必须是真函数」（R99）。
    let replay = events_from_messages(&msgs, &|n| f.registry.risk_of(n));
    let replay_text: String = replay
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    let replay_users = replay
        .iter()
        .filter(|e| matches!(e, AgentEvent::UserMessage { .. }))
        .count();

    // F1（T13 m1）：**行序**。上面与下面这几条断言全都是「按角色筛一遍再比」或
    // 「数个数」，所以一轮内部整段倒序在它们眼里看不出来 —— 评审变异 M6 实证过：
    // `append_rows` 的迭代一旦改成反向，230 条测试全绿，而 `messages()` 是
    // `ORDER BY id`（写入顺序，db.rs:202），落库顺序就是重放顺序 ⇒ 助手回复会排在
    // 用户提问之前。这里按**落库行的原始顺序**逐行断言角色与文本，谁把追加顺序弄倒
    // 就必须在这里红。
    assert_eq!(
        msgs.iter()
            .map(|m| (m.role.clone(), m.text_of()))
            .collect::<Vec<_>>(),
        vec![
            (Role::User, "第一句".to_string()),
            (Role::Assistant, "回复一".to_string()),
            (Role::User, "第二句".to_string()),
            (Role::Assistant, "回复二".to_string()),
        ],
        "落库行序必须是「一问一答」交替；T20 的重放直接吃这个顺序"
    );
    // 重放侧同样要按行序交替：剔除 `usage` 与 `done` 两支后的**全事件序列**。
    // `turn_start` 排在 `user_message` 之后不是笔误 —— `events_from_messages`
    // 是在助手行才发它（db.rs:395），顺序本身就是行序的镜像。
    let replay_tags: Vec<&str> = replay
        .iter()
        .filter(|e| !matches!(e, AgentEvent::Usage { .. } | AgentEvent::Done { .. }))
        .map(event_tag)
        .collect();
    assert_eq!(
        replay_tags,
        vec![
            "user_message",
            "turn_start",
            "text_delta",
            "user_message",
            "turn_start",
            "text_delta",
        ],
        "重放的事件顺序就是行序：user_message 必须排在它那条回复之前"
    );

    assert_eq!(replay_text, live_text, "重放的文字与直播不一致");
    assert_eq!(
        replay_users, 2,
        "一条用户轮次只许有重放里的一条 user_message"
    );
    assert_eq!(
        replay
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnStart { .. }))
            .count(),
        2
    );
}

/// G37 的正向半边（接替 R104 的同名测试）：默认标题「新对话」在首轮结束后被
/// **模型生成的标题**顶掉。起名在落库后的独立任务里异步跑，断言前轮询等它
/// 落库 —— 轮询耗尽仍没等到，说明起名任务丢了，同样是失败，不许静默放过。
#[tokio::test]
async fn a_default_title_adopts_the_model_title() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.turn(&sid, replies(&["好的"]), "帮我看看今天的天气")
        .await
        .0
        .unwrap();
    let mut title = title_of(&f.db, &sid);
    for _ in 0..200 {
        if title.as_deref() == Some(TEST_TITLE_REPLY) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        title = title_of(&f.db, &sid);
    }
    assert_eq!(
        title.as_deref(),
        Some(TEST_TITLE_REPLY),
        "首轮结束后模型标题应已落库；等不到 = 起名任务没跑或被降级路径抢先"
    );
}

/// R104（brief:161-163 的无条件 rename 会在这里转红）：用户手工改过的名字
/// 在下一次发送时必须原样保住。
#[tokio::test]
async fn a_hand_named_session_keeps_its_name() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.rename_session(&sid, "我的名字").unwrap();
    // 必须 unwrap：整轮失败时标题当然还是「我的名字」，那条断言会因为**什么都没发生**而变绿。
    f.turn(&sid, replies(&["好的"]), "随便聊聊")
        .await
        .0
        .unwrap();
    assert_eq!(title_of(&f.db, &sid).as_deref(), Some("我的名字"));
}

fn title_of(db: &Db, sid: &str) -> Option<String> {
    db.list_sessions()
        .unwrap()
        .into_iter()
        .find(|s| s.id == sid)
        .map(|s| s.title)
}

/// 收尾必须把取消令牌槽位清空：留着它，`interrupt` 会在下一轮里取消一个已经死掉的会话。
#[tokio::test]
async fn a_finished_turn_releases_the_cancel_slot() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.turn(&sid, replies(&["好的"]), "说一句").await.0.unwrap();
    assert!(
        f.cancel.lock().unwrap().is_none(),
        "一轮跑完不许把令牌留在槽里"
    );
}

// ==== Task 26：装配层真的把**有效表**交给了会话（R276 的唯一入口 + R284 的使用点）====
//
// 为什么这条测试非得住在 ipc 里：`session.rs` 那条
// `run_turn_passes_configured_roots_into_tool_ctx` 量的是 `AgentSession::with_roots`
// **这个方法本身**能用；而本任务修的是「它在生产里从未被调用」。把 `run_turn` 里
// `AgentSession::new(..).with_roots(effective_roots(&roots))` 那一行删掉（现 :438），
// `session.rs` 全绿、`settings.rs` 全绿，只有这里红 ——
// 判据必须能被打红，不然它不构成一条线（R56 的老规矩）。
// R284 之后这里还多钉一件事：把那一行的 `effective_roots(&roots)` 换回 `roots`，
// 同一处当场红（够不着的那条 root 就又能把闸门整体拖死了）。
//
// 取证方式和 `session.rs` 那边一致：从**工具自己手里**读 `ctx.roots`。`ToolCtx` 是
// 会话内部造的（`session.rs:123`），从 `AgentSession` 的字段读回来属于自证。

use crate::agent::types::ToolOutput;
use crate::tools::registry::{Tool, ToolCtx, ToolError, ToolMeta};
// `ModelDelta`/`ModelError`/`ChatRequest`/`ModelProvider`/`Stream`/`async_trait`/
// `VecDeque` 都由本模块后头那组 `use`（Task 24/41 段）带进来了 —— 模块级 `use` 与
// 书写顺序无关，这里再写一遍就是 E0252 重复导入。

/// 记下**执行那一刻**的 `ctx.roots`，每次调用一条。
#[derive(Default)]
struct RootsSpy {
    seen: Mutex<Vec<Vec<PathBuf>>>,
}

#[async_trait]
impl Tool for RootsSpy {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "roots_spy".into(),
            description: "记下执行那一刻 ctx.roots 的取证工具".into(),
            input_schema: json!({"type": "object"}),
            // Read 档是有意的：这条测试量的是 roots 到没到。Ask 档要在测试里搭一套
            // 应答任务，那会把「roots 没接上」的读数埋进应答时序里。
            risk: Risk::Read,
        }
    }
    async fn execute(
        &self,
        _input: serde_json::Value,
        ctx: &ToolCtx,
    ) -> Result<ToolOutput, ToolError> {
        self.seen.lock().unwrap().push(ctx.roots.clone());
        Ok(ToolOutput::text("记下了"))
    }
}

impl RootsSpy {
    fn seen(&self) -> Vec<Vec<PathBuf>> {
        self.seen.lock().unwrap().clone()
    }
}

/// 按轮吐剧本的模型。本模块原有的两只都不会发 `tool_call`（Plan A 的生产工具表是空的），
/// 而 roots 的唯一消费者就是工具 —— 不让模型真的调一次工具，「传到执行点」就无从取证。
/// **取空了就是夹具写错**，不许安静地回一条空消息把测试做绿。
struct Scripted {
    rounds: Mutex<VecDeque<Vec<ModelDelta>>>,
}

#[async_trait]
impl ModelProvider for Scripted {
    fn stream<'a>(
        &'a self,
        _req: ChatRequest,
    ) -> std::pin::Pin<Box<dyn Stream<Item = Result<ModelDelta, ModelError>> + Send + 'a>> {
        let round = self
            .rounds
            .lock()
            .unwrap()
            .pop_front()
            .expect("夹具轮数不够");
        Box::pin(futures::stream::iter(
            round.into_iter().map(Ok).collect::<Vec<_>>(),
        ))
    }
}

/// 一轮工具调用：id 带轮次（R57 —— 全复用 `"c1"` 的话配对断言会 latch 到早先那次）。
fn tool_round(round: usize, name: &str) -> Vec<ModelDelta> {
    vec![
        ModelDelta::ToolCallStart {
            index: 0,
            id: Some(format!("r{round}c0")),
            name: Some(name.to_string()),
        },
        ModelDelta::ToolCallArgs {
            index: 0,
            fragment: "{}".to_string(),
        },
    ]
}

/// brief Step 5 的生产侧那条线：`TurnDeps.roots`（= 用户写的那张表）必须**在交给会话的
/// 那一刻**被裁成有效表，然后一路走到 `ToolCtx.roots`。R284 把「这条今天还在不在」的
/// 判定从读写两侧搬到这里，所以这条测试钉的两件事是：**使用点裁剪**和 **R276 的单一入口**
/// —— 把 `run_turn` 里 `.with_roots(..)` 那一行删掉这里当场红（表变空）。
#[tokio::test]
async fn run_turn_hands_the_effective_roots_not_the_persisted_list() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let spy = Arc::new(RootsSpy::default());
    // 注册在**夹具的** registry 上（`Fixture::new` 每轮现构一只空表），
    // 生产那只在 `build_production_registry` 函数体里装配、由 `app.manage` 那一行接管
    // —— 夹具**不**改用它：这里的测试只量自己注册的那把替身工具。
    f.registry.register(spy.clone());

    // 两条真目录 + 一条**不存在**的路径 + 一条**相对**路径，原样塞进 `TurnDeps.roots` ——
    // 那是设置文件里的形状（R284 之后读写两侧不再裁盘，所以到这一层的表就是用户写的那份）。
    // 把不存在的那条塞进来是故意的，它就是 Important-1 的现场：用户拔了 U 盘，那条 root 照旧在
    // 设置里，但这一轮不许把它交给闸门 —— 规则 4 会整体拒绝，连带让别的合法 root 也读不到东西。
    // 相对那条钉的是 R285 的**使用点**一半：`"."` 是解得开的（就解在进程 CWD 上），
    // 所以只有绝对性这条判据能拦住它，而拦住它的地方必须是交闸门之前，不只是 settings 层。
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ghost = a.path().join("never-created");
    let roots = vec![
        a.path().to_path_buf(),
        b.path().to_path_buf(),
        ghost.clone(),
        PathBuf::from("."),
    ];

    let model = Arc::new(Scripted {
        rounds: Mutex::new(VecDeque::from(vec![
            tool_round(0, "roots_spy"),
            vec![ModelDelta::Text("看完了".into())],
        ])),
    });
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model,
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: roots.clone(),
            skills_dir: None,
            skills_disabled: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "读一下我的笔记".into(),
        }],
    )
    .await
    .expect("带一次工具调用的一轮必须整体成功");
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn, "剧本第二轮就是收尾文本");

    let seen = spy.seen();
    assert_eq!(
        seen.len(),
        1,
        "夹具只安排了一次工具调用，取证点就得恰好记下一条；0 条说明工具压根没跑，那条断言是空的"
    );
    assert_eq!(
        seen[0],
        vec![
            a.path().canonicalize().unwrap(),
            b.path().canonicalize().unwrap()
        ],
        "交到执行点的必须是**有效表**（绝对 + 解得开 + canonical 形态），不是设置文件里 \
         那份原表；这一条同时守住 R276 的单一入口 —— 删掉 `run_turn` 里 `.with_roots(..)` \
         那一行，这里就空了。夹具里那条 `\".\"` 钉的是绝对性：它 canonicalize 得开（就解在进程 \
         CWD 上），所以拦得住它的只有「绝对」这一条判据（R285），而拦截必须发生在交闸门之前 —— \
         删掉 `effective_roots` 里的 `is_absolute()`，这里就多出一条 `\\\\?\\<进程 CWD>`，\
         等于程序替用户选了一个他没指过的目录；实得 {:?}（原表 {:?}）",
        seen[0],
        roots
    );
    assert!(
        !seen[0].iter().any(|p| p == &ghost),
        "暂时够不着的那条 root 不许进这一轮的使用表（R284）；实得 {:?}",
        seen[0]
    );
}

/// 空表也要**原样**传下去：装配层不许在 roots 为空时替用户补一个「看起来合理」的目录
/// （进程 cwd、可执行文件目录、app data 目录都在此列）。R90 的空 `base_url` 是同一个
/// 取向 —— 那一支真被补过，症状是把会话内容发给用户没选过的端点；这一支补了的代价是
/// 模型能读用户没批准过的目录。两条都是「界面替用户做主」，只是方向相反。
#[tokio::test]
async fn an_empty_roots_table_is_handed_over_empty_instead_of_a_default_directory() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let spy = Arc::new(RootsSpy::default());
    f.registry.register(spy.clone());

    let model = Arc::new(Scripted {
        rounds: Mutex::new(VecDeque::from(vec![
            tool_round(0, "roots_spy"),
            vec![ModelDelta::Text("没读到东西".into())],
        ])),
    });
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model,
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            skills_dir: None,
            skills_disabled: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "读一下我的笔记".into(),
        }],
    )
    .await
    .expect("空 roots 不是错误，一轮照常跑完");
    sink.flush();

    let seen = spy.seen();
    assert_eq!(seen.len(), 1, "同上：取证点必须真的跑过一次");
    assert!(
        seen[0].is_empty(),
        "roots 为空时执行点却拿到了 {:?} —— 那是装配层替用户做的决定",
        seen[0]
    );
}

// ---- Task 27：生产 registry 的装配判据 ----

/// 生产工具表**就是这份名单**：多一把、少一把都算装配错，而这一条是它唯一的钉子
/// （Plan A 交的是空表，Plan B 到此才开始有真工具进生产）。
/// `echo` / `guarded` 这类替身只该活在 `#[cfg(test)]` 里 —— 它们的名字一旦进生产，
/// 请求体里就多出一条用户看不懂、实现也不是给用户的工具。
#[test]
fn production_registry_ships_the_builtin_tools() {
    let (_dir, db) = (
        tempfile::tempdir().unwrap(),
        Arc::new(Db::open(&tempfile::tempdir().unwrap().path().join("r.db")).unwrap()),
    );
    let r = build_production_registry(db, Arc::new(RwLock::new(ShellCatalog::new())));
    let names: Vec<String> = r.defs().into_iter().map(|d| d.name).collect();
    // 这个字面顺序由 `ToolRegistry` 的 `BTreeMap`（字典序）决定，**不是**注册顺序。
    // 刻意不加 `.sort()`、也不换成集合比较：T33 的 MCP 工具重挂之后仍然靠「名字序不变」
    // 保证界面与模型侧的工具列表不因重连而抖动，把这条性质藏起来就没法发现它被改坏。
    assert_eq!(
        names,
        vec![
            "edit_file",
            "experiment",
            "fetch_url",
            "glob",
            "grep",
            "memory_search",
            "question",
            "read_file",
            "render_html",
            "run_command",
            "task",
            "todo_write",
            "undo",
            "use_skill",
            "websearch",
            "workflow",
            "write_file",
        ],
        "模型看到的工具集必须是这份名单；echo/guarded 这类测试工具绝不能进生产（它们只活在 #[cfg(test)]）"
    );
    for name in [
        "fetch_url",
        "read_file",
        "use_skill",
        "websearch",
        "question",
    ] {
        assert_eq!(
            r.risk_of(name),
            Risk::Read,
            "{name} 是只读档（spec 5.1）；它变成 Write 就会开始弹窗"
        );
    }
    assert_eq!(
        r.risk_of("memory_search"),
        Risk::Read,
        "记忆检索只读本机库,与 fs.read 同档 ⇒ 零弹窗"
    );
    for name in ["write_file", "edit_file"] {
        assert_eq!(
            r.risk_of(name),
            Risk::Write,
            "{name} 必须走 Write 档，权限门才会弹窗；降成 Read 等于绕开用户"
        );
    }
    // G25：experiment 是 Exec 档 —— 跑任意命令 + 直接改 git 状态（keep 提交 /
    // discard 回滚），每一次调用都要用户点头，没有会话级免弹窗的形态。
    assert_eq!(
        r.risk_of("experiment"),
        Risk::Exec,
        "experiment 必须走 Exec 档：实验循环连续改工作区，用户要每步看得见、拦得住"
    );
    // G16：task 是 Write 档 —— 子代理自主烧多轮请求，这一下必须弹窗；
    // 降 Read 等于绕开权限门。
    assert_eq!(
        r.risk_of("task"),
        Risk::Write,
        "task 必须走 Write 档：派子代理前让用户看得见、拦得住"
    );
    // G24：workflow 是 Write 档 —— 按定义连续派子代理（task 的 N 连发），
    // 降 Read 等于把一串该弹窗的调用折叠成一次静默放行。
    assert_eq!(
        r.risk_of("workflow"),
        Risk::Write,
        "workflow 必须走 Write 档：编排多个子代理前让用户看得见、拦得住"
    );
    // Agent Shell（spec §2/§8）：`run_command` 是第一把内置 Exec 工具，风险唯一真相源
    // 是 `ShellTool::meta()`（经 `risk_of` 出）。它降档就等于绕开权限门的每次弹窗。
    assert_eq!(
        r.risk_of("run_command"),
        Risk::Exec,
        "run_command 必须走 Exec 档：每次调用弹窗，会话级放行的粒度是整条命令文本"
    );
    assert_eq!(
        r.risk_of("delete_everything"),
        Risk::Exec,
        "未知工具按最严一档（R158）"
    );
}

/// Agent Shell（spec §2/§5.5）：`send_message` 每轮开头用它把本轮设置变成 catalog。
/// 纯函数：settings 的 shells / default_shell 原样进 catalog；default 空白时
/// `from_parts` 只做 trim（catalog 里就是空串），真正的回落发生在 `resolve(None)`
/// —— 落到 `DEFAULT_PRESET_ID`（桩的既有语义；这条钉的是接线面别把它弄丢）。
/// 2026-11-01 修订后 catalog 的预设侧来自本机检测：回落那半句改用全在 probe 构造
/// （不依赖本机装了什么，R305）；自定义那半句仍走真构造 —— customs 先于 presets，
/// 检测与否不影响命中。
#[test]
fn shell_catalog_of_passes_settings_through_and_resolves_a_blank_default_to_the_preset() {
    let settings = Settings {
        shells: vec![crate::tools::shell::ShellProfile {
            id: "mine".into(),
            label: String::new(),
            program: "/opt/mine".into(),
            args: vec!["-c".into()],
        }],
        default_shell: "  mine  ".into(),
        ..Default::default()
    };
    let catalog = shell_catalog_of(&settings);
    assert_eq!(catalog.customs, settings.shells, "自定义表原样进 catalog");
    assert_eq!(
        catalog.default_id, "mine",
        "default_shell trim 后进 catalog"
    );
    assert_eq!(
        catalog.resolve(None).unwrap().program,
        "/opt/mine",
        "默认 shell 指向自定义 profile 时按它解析"
    );
    let blank = shell_catalog_of(&Settings {
        default_shell: "   ".into(),
        ..Default::default()
    });
    assert_eq!(
        blank.default_id, "",
        "空白 default 进 catalog 只是空串（from_parts 只 trim）"
    );
    // 回落语义钉在检测过的 catalog 上（全在 probe ⇒ 8 条候选全数在预设侧）。
    let blank_with_all_presets =
        crate::tools::shell::ShellCatalog::from_parts_with_probe(Vec::new(), "", &|_, _| {
            Ok(PathBuf::new())
        });
    assert_eq!(
        blank_with_all_presets.presets.len(),
        8,
        "全在 probe 下候选表全数进预设侧"
    );
    assert_eq!(
        blank_with_all_presets.resolve(None).unwrap().id,
        crate::tools::shell::DEFAULT_PRESET_ID,
        "空 default 在 resolve 时回落默认预设"
    );
}

// ---- Agent Shell：两条 IPC 命令（spec §5.5） ----

/// `shell_presets` 返回的是「候选表经本机检测的子集」（2026-11-01 修订：候选 +
/// 检测 —— 本机装了什么决定返回几条，例如这台 Linux 开发机是 bash/sh，其余候选
/// 缺席；也可能为空）。钉法相应改为：返回集必须是 spec §5.2 冻结顺序的**子序列**，
/// 且每一条与候选表**逐字段一致**（原「逐项一致」意图的检测版 —— 发布面没抄错由
/// 这里钉；候选表本身由 shell.rs 的 `candidate_table_has_exactly_the_eight_spec_ids`
/// 钉着）。每条 Serialize 成 id/label/program/args **恰好**四键 —— `ShellProfile`
/// 本身就是线格式，不新增 view 结构体（spec §5.5；TS 镜像 `ShellProfile` 靠这条钉着）。
#[test]
fn shell_presets_return_a_spec_ordered_subset_matching_the_candidate_table() {
    // 全在 probe 造出完整候选面，当「spec §5.2 冻结表」的参照。
    let all_candidates =
        ShellCatalog::from_parts_with_probe(Vec::new(), "", &|_, _| Ok(PathBuf::new())).presets;
    assert_eq!(
        all_candidates
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        [
            "bash",
            "sh",
            "zsh",
            "fish",
            "git-bash",
            "pwsh",
            "powershell",
            "cmd"
        ],
        "参照面 = spec §5.2 的 8 条（顺序即展示顺序）"
    );
    let presets = shell_presets();
    let expected: Vec<_> = all_candidates
        .iter()
        .filter(|c| presets.iter().any(|p| p.id == c.id))
        .cloned()
        .collect();
    assert_eq!(
        presets, expected,
        "发布面必须是候选表按冻结顺序过滤出的子集，且逐字段一致"
    );
    for p in &presets {
        let v = serde_json::to_value(p).unwrap();
        let obj = v.as_object().expect("每条预设必须能 Serialize 成对象");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["args", "id", "label", "program"],
            "{} 的线格式是恰好四键，多一个少一个都算错",
            p.id
        );
        assert!(
            !obj["program"].as_str().unwrap().is_empty(),
            "{} 的 program 不该是空串",
            p.id
        );
    }
}

/// 校验的命中支：裸名在注入的假 search_dirs 里命中 → `Ok`。validate_custom 先归一化
/// （trim 过的 id/args 才进判定），程序存在性用的是注入表 —— 单测不依赖本机 PATH
/// （同 mcp R305 的注入口径）。
#[cfg(unix)]
#[test]
fn shell_profile_validate_hits_a_bare_name_in_the_injected_search_dirs() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let hit = dir.path().join("mysh");
    std::fs::write(&hit, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&hit, std::fs::Permissions::from_mode(0o755)).unwrap();
    validate_shell_profile_with_dirs(
        ShellProfile {
            id: " mine ".into(),
            label: " 显示名 ".into(),
            program: "mysh".into(),
            args: vec![" -c ".into()],
        },
        &[dir.path().to_path_buf()],
    )
    .expect("裸名在注入目录里命中必须 Ok");
    std::fs::remove_dir_all(dir).ok();
}

/// Windows 镜像（与 unix 版同形状；Windows CI 上验证）：裸名按 PATHEXT 展开候选
/// （shell.rs 的 `candidate_names`），可执行判定看扩展名而不是 x 位 —— 夹具
/// 相应地放一个 `mysh.cmd`，裸名 `mysh` 必须经 `.CMD` 候选命中。
#[cfg(windows)]
#[test]
fn shell_profile_validate_hits_a_bare_name_in_the_injected_search_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let hit = dir.path().join("mysh.cmd");
    std::fs::write(&hit, b"@echo off\r\n").unwrap();
    validate_shell_profile_with_dirs(
        ShellProfile {
            id: " mine ".into(),
            label: " 显示名 ".into(),
            program: "mysh".into(),
            args: vec![" -c ".into()],
        },
        &[dir.path().to_path_buf()],
    )
    .expect("裸名在注入目录里命中必须 Ok");
    std::fs::remove_dir_all(dir).ok();
}

/// Windows 支：当前测试可执行文件是**真的**存在且带 .exe 的普通文件
/// （桩的启发式判据），绝对路径直判必须过（plan：Windows 分支用 current_exe 兜底真值）。
#[cfg(windows)]
#[test]
fn shell_profile_validate_on_windows_hits_the_test_executable_itself() {
    let exe = std::env::current_exe().unwrap();
    validate_shell_profile_with_dirs(
        ShellProfile {
            id: "self".into(),
            label: String::new(),
            program: exe.display().to_string(),
            args: Vec::new(),
        },
        &[],
    )
    .expect("测试可执行文件（.exe）必须判为可执行");
}

/// 未命中支：裸名不在假 PATH 里 → `Err` 且**点名 program 与查找范围**
/// （错误原样上屏，用户要能对着它改 profile）。
#[test]
fn shell_profile_validate_misses_name_the_program() {
    let empty = tempfile::tempdir().unwrap();
    let e = validate_shell_profile_with_dirs(
        ShellProfile {
            id: "mine".into(),
            label: String::new(),
            program: "no-such-shell".into(),
            args: vec!["-c".into()],
        },
        &[empty.path().to_path_buf()],
    )
    .unwrap_err();
    assert!(
        e.contains("no-such-shell") && e.contains("搜索目录"),
        "错误要点名 program 与查找范围：{e}"
    );
    std::fs::remove_dir_all(empty).ok();
}

/// 形状错误支：`validate_custom` 先拒（空 id），存在性检查**根本不发生** ——
/// program 指到不存在的地方也不影响错误内容（两步分开，spec §5.4）。
#[test]
fn shell_profile_validate_reports_shape_errors_without_any_existence_check() {
    let e = validate_shell_profile_with_dirs(
        ShellProfile {
            id: "  ".into(),
            label: String::new(),
            program: "/definitely/not/here".into(),
            args: vec!["-c".into()],
        },
        &[],
    )
    .unwrap_err();
    assert!(
        e.contains("id"),
        "形状错误要来自 validate_custom 而不是存在性判定：{e}"
    );
}

/// R282：`SettingsView` 的线上形状 —— `roots` 落在**顶层**、是**字符串数组**。
///
/// brief Step 5 写的是「`SettingsView` 加 `roots: Vec<String>`」，而那是**空转**：
/// `settings` 那一项是 `#[serde(flatten)]`（:655），新字段自己就冒到顶层来了，
/// 手加一个 `roots` 字段反而是同一份设置的两个来源（R276 要避免的那个形状，另一个方向）。
/// 但「不用改代码」不等于「不用测」：flatten 一旦被人改成嵌套（或者给这个 struct 加了
/// `rename_all`），前端 `settings.roots` 就静默变成 `undefined`，而 `tsc` 对着 Rust 侧
/// 什么都看不见。这条测试钉的就是这个形状，前端镜像（`src/types.ts`）由它兜住。
#[test]
fn the_settings_view_carries_roots_at_the_top_level_as_strings() {
    let view = SettingsView {
        settings: Settings {
            // `\\?\` 前缀是刻意选的：`PathBuf` 的 serde 实现原样交字符串，
            // 于是这条测试同时也在钉「Windows 长路径前缀不会被拆成结构体」。
            roots: vec![
                PathBuf::from(r"\\?\C:\notes"),
                PathBuf::from(r"\\?\D:\code"),
            ],
            ..Default::default()
        },
        api_key_masked: None,
        corrupt_backup: None,
    };
    let v = serde_json::to_value(&view).unwrap();
    let obj = v
        .as_object()
        .unwrap_or_else(|| panic!("view 必须序列化成一个对象，实得 {v:?}"));
    assert!(
        obj.contains_key("roots"),
        "`roots` 没在顶层 —— flatten 臂被改坏了，前端读到的会是 undefined。顶层键：{:?}",
        obj.keys().collect::<Vec<_>>()
    );
    let arr = v["roots"]
        .as_array()
        .unwrap_or_else(|| panic!("roots 必须是 JSON 数组，实得 {:?}", v["roots"]));
    assert_eq!(
        arr.iter()
            .map(|x| x.as_str().unwrap_or("<非字符串>"))
            .collect::<Vec<_>>(),
        vec![r"\\?\C:\notes", r"\\?\D:\code"],
        "每一项都得是字符串：TS 镜像声明的是 string[]，混进对象就是镜像骗人"
    );
    // 视图自带的两项与 flatten 进来的十三项**共处一层**（`roots` 不许被挤进 `settings` 里），
    // 而明文 key 依旧不许出现在任何一个键名上（R83 / spec:473）。
    assert!(obj.contains_key("api_key_masked") && obj.contains_key("corrupt_backup"));
    assert!(
        !obj.contains_key("settings"),
        "顶层冒出了嵌套的 `settings` 对象，前端镜像是按顶层读的"
    );
    assert!(
        !obj.contains_key("api_key"),
        "明文 key 不许出现在这条边界上（这里能出现 `api_key_masked`，不能出现 `api_key`）"
    );
    // 默认值那一支也得有键：空表序列化成 `[]` 而不是整个键消失，
    // 否则前端的 `settings.roots.map(...)` 在生产默认配置下直接抛。
    let empty = serde_json::to_value(SettingsView {
        settings: Settings::default(),
        api_key_masked: None,
        corrupt_backup: None,
    })
    .unwrap();
    assert_eq!(
        empty["roots"].as_array().map(|a| a.len()),
        Some(0),
        "空 roots 必须是可见的空数组，不是缺键"
    );
}

// ==== Task 41：两次发送之间的摘要压缩 ====
//
// 本组的形状由 brief §5 定死：`#[tauri::command]` 在单测里调不动（Plan A T13 实测过），
// 所以压缩段的全部真相住在**不依赖 `State`** 的 `compact_before_turn` 里，Step 1–4 的
// 测试一律直接调它、一处都不碰 `run_turn`。唯一例外是
// `prior_is_recounted_after_compaction`：`prior` 那一行下标只存在于 `run_turn` 内部，
// 判据 (a)「把 prior 放回压缩前」要红的就是它，理由写在那条测试的注释里。
//
// 假模型本地写：`session.rs` 的 `VecSink`/`ScriptedModel`/`HangingModel` 是那个私有测试
// 模块的（:356/:467/:523），跨不过模块边界，也**不许**为了让它们跨过来改成 `pub`。

use crate::agent::summarize::{SUMMARY_INSTRUCTION, SUMMARY_OMITTED, SUMMARY_PREFIX, summary_note};
use async_stream::stream;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

/// 摘要调用与本轮回答共用**同一只**模型 —— 生产就是这样（`TurnDeps.model` 只有一个
/// `Arc`，`compact_before_turn` 与 `AgentSession` 拿的是它），所以这个替身按请求头部分流：
/// 头一条 system 是哪条指令就走哪一支(摘要/记忆抽取/记忆合并/画像归并),否则走回答那一支。
/// `calls` 逐次记账（R57：「模型实际看到了什么」只能从模型这一侧取证）。
struct FakeModel {
    /// 摘要调用吐出的分片（失败那一支里它是「报错之前已经到的半句」）
    summary_parts: Vec<String>,
    /// `true` = 摘要调用先吐 `summary_parts` 再回一个 `Api` 错误（429 那一类）
    summary_fails: bool,
    /// `true` = 摘要调用吐完分片后**挂住不回**，等测试按停止
    summary_stalls: bool,
    /// 记忆抽取那一支的回复。默认 `[]`(「这段没什么可记」),让既有压缩测试
    /// 走「抽取成功但零候选」而不是「抽取报错」,日志干净、路径是真实生产路径。
    extraction_parts: Vec<String>,
    /// `true` = 抽取调用吐完分片后挂住,等测试按停止。
    extraction_stalls: bool,
    /// `true` = 抽取调用在吐完分片之后回一个错误(T15:游标必须不动的那些用例)。
    extraction_fails: bool,
    /// 记忆合并那一支的回复(只有存在同键冲突的测试会配;默认空串=模型答非所问→跳过)。
    consolidation_parts: Vec<String>,
    /// 多个候选时按调用顺序逐条取用；空队列回落到 `consolidation_parts`。
    /// 两个合并调用共用一个 `consolidation_parts` 会把两段 JSON 拼在一起 ⇒ 都解析失败,
    /// 多候选测试会假绿。
    consolidation_queue: Mutex<VecDeque<String>>,
    /// 画像归并那一支的回复(只有抽取产出 profile 候选的测试会配;默认空串=Err→保持现状)。
    profile_parts: Vec<String>,
    /// 本轮回答的队列（只有走 `run_turn` 的那条测试用得到）
    replies: Mutex<VecDeque<String>>,
    calls: Mutex<Vec<ChatRequest>>,
    /// 挂住夹具的同步点：置位 = 最后一个分片已经抵达消费者。不靠 sleep 猜时间。
    stalled: Arc<AtomicBool>,
}

impl FakeModel {
    fn build(parts: &[&str], answers: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            summary_parts: parts.iter().map(|s| s.to_string()).collect(),
            summary_fails: false,
            summary_stalls: false,
            extraction_parts: vec!["[]".to_string()],
            extraction_stalls: false,
            extraction_fails: false,
            consolidation_parts: vec![String::new()],
            consolidation_queue: Mutex::new(VecDeque::new()),
            profile_parts: vec![String::new()],
            replies: Mutex::new(answers.iter().map(|s| s.to_string()).collect()),
            calls: Mutex::new(vec![]),
            stalled: Arc::new(AtomicBool::new(false)),
        })
    }
    fn summarizing(parts: &[&str]) -> Arc<Self> {
        Self::build(parts, &[])
    }
    /// 报错之前先吐出 `parts` 那半句：半截摘要不是摘要。
    fn failing(parts: &[&str]) -> Arc<Self> {
        let mut raw = Self::build(parts, &[]);
        // 构造期改一次：`Arc` 之后不再可变，所以这里用 `get_mut`（唯一持有者）。
        Arc::get_mut(&mut raw).unwrap().summary_fails = true;
        raw
    }
    /// 吐完 `parts` 之后永久挂住：测试在 `stalled` 置位的那一刻按停止。
    fn stalling(parts: &[&str]) -> Arc<Self> {
        let mut raw = Self::build(parts, &[]);
        Arc::get_mut(&mut raw).unwrap().summary_stalls = true;
        raw
    }
    /// 抽取那一支回 `parts`(join 成一条文本),回答队列照旧。
    fn extracting(parts: &[&str], answers: &[&str]) -> Arc<Self> {
        let mut raw = Self::build(&[], answers);
        Arc::get_mut(&mut raw).unwrap().extraction_parts =
            parts.iter().map(|s| s.to_string()).collect();
        raw
    }
    /// 抽取吐完分片后挂住:取消语义的夹具(同 stalling,只是挂的那一支换成了抽取)。
    fn extraction_stalling(parts: &[&str]) -> Arc<Self> {
        let mut raw = Self::build(&[], &[]);
        let m = Arc::get_mut(&mut raw).unwrap();
        m.extraction_parts = parts.iter().map(|s| s.to_string()).collect();
        m.extraction_stalls = true;
        raw
    }
    /// T15:抽取先吐 `parts` 再回一个端点错误(半截输出不是结论 ——
    /// 有没有那半截,调用方都必须把它当成失败、游标不动)。
    fn extraction_failing(parts: &[&str]) -> Arc<Self> {
        let mut raw = Self::build(&[], &[]);
        let m = Arc::get_mut(&mut raw).unwrap();
        m.extraction_parts = parts.iter().map(|s| s.to_string()).collect();
        m.extraction_fails = true;
        raw
    }
    /// 在抽取之上再配合并/画像的回复(按需,没配的那支维持降级默认)。
    fn with_consolidation(mut raw: Arc<Self>, parts: &[&str]) -> Arc<Self> {
        Arc::get_mut(&mut raw).unwrap().consolidation_parts =
            parts.iter().map(|s| s.to_string()).collect();
        raw
    }
    /// 多候选场景：每次合并调用取队首的一条，而不是把整段拼给每一次调用。
    fn with_consolidation_queue(mut raw: Arc<Self>, parts: &[&str]) -> Arc<Self> {
        Arc::get_mut(&mut raw).unwrap().consolidation_queue =
            Mutex::new(parts.iter().map(|s| s.to_string()).collect());
        raw
    }
    fn with_profile(mut raw: Arc<Self>, parts: &[&str]) -> Arc<Self> {
        Arc::get_mut(&mut raw).unwrap().profile_parts =
            parts.iter().map(|s| s.to_string()).collect();
        raw
    }
    /// 在摘要+回答之上再配抽取的回复(T7 主闭环用:压缩段要真抽出一条事实候选,
    /// 而 `summarizing_and_replying` 默认的抽取支是 `[]` 零候选)。与上面两个
    /// 组合器同款:必须趁 Arc 引用计数还是 1 时改,构造链上不许有第二只手。
    fn with_extraction(mut raw: Arc<Self>, parts: &[&str]) -> Arc<Self> {
        Arc::get_mut(&mut raw).unwrap().extraction_parts =
            parts.iter().map(|s| s.to_string()).collect();
        raw
    }
    /// 走完整 `run_turn` 时用：摘要与回答都要能答。
    fn summarizing_and_replying(parts: &[&str], answers: &[&str]) -> Arc<Self> {
        Self::build(parts, answers)
    }
    fn calls(&self) -> Vec<ChatRequest> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ModelProvider for FakeModel {
    // ModelError::Transport 内联携带 OpenAIError（≥136 字节），替身签名躲不开
    // Result<_, ModelError>；Box 化属于错误类型布局重构，超出本次机械清理范围。
    #[allow(clippy::result_large_err)]
    fn stream<'a>(
        &'a self,
        req: ChatRequest,
    ) -> std::pin::Pin<Box<dyn Stream<Item = Result<ModelDelta, ModelError>> + Send + 'a>> {
        use crate::agent::memory::{
            CONSOLIDATE_INSTRUCTION, EXTRACT_INSTRUCTION, PROFILE_INSTRUCTION,
        };
        self.calls.lock().unwrap().push(req.clone());
        let head_sys = req
            .messages
            .first()
            .filter(|m| m.role == Role::System)
            .map(|m| m.text_of());
        let one_shot = |parts: &[String]| -> Vec<Result<ModelDelta, ModelError>> {
            parts
                .iter()
                .cloned()
                .map(ModelDelta::Text)
                .map(Ok)
                .collect()
        };
        match head_sys.as_deref() {
            Some(SUMMARY_INSTRUCTION) if self.summary_fails => {
                let head = self.summary_parts.clone();
                Box::pin(futures::stream::iter(
                    head.into_iter()
                        .map(|t| Ok(ModelDelta::Text(t)))
                        .chain(std::iter::once(Err(ModelError::Api(
                            "429 rate limited".to_string(),
                        )))),
                ))
            }
            Some(SUMMARY_INSTRUCTION) if self.summary_stalls => {
                let head: Vec<ModelDelta> = self
                    .summary_parts
                    .iter()
                    .cloned()
                    .map(ModelDelta::Text)
                    .collect();
                let ready = self.stalled.clone();
                Box::pin(stream! {
                    for d in head {
                        yield Ok(d);
                    }
                    ready.store(true, Ordering::SeqCst);
                    std::future::pending::<()>().await;
                })
            }
            Some(SUMMARY_INSTRUCTION) => {
                Box::pin(futures::stream::iter(one_shot(&self.summary_parts)))
            }
            Some(EXTRACT_INSTRUCTION) if self.extraction_fails => {
                let head = self.extraction_parts.clone();
                Box::pin(futures::stream::iter(
                    head.into_iter()
                        .map(|t| Ok(ModelDelta::Text(t)))
                        .chain(std::iter::once(Err(ModelError::Api(
                            "500 extraction failed".to_string(),
                        )))),
                ))
            }
            Some(EXTRACT_INSTRUCTION) if self.extraction_stalls => {
                let head: Vec<ModelDelta> = self
                    .extraction_parts
                    .iter()
                    .cloned()
                    .map(ModelDelta::Text)
                    .collect();
                let ready = self.stalled.clone();
                Box::pin(stream! {
                    for d in head {
                        yield Ok(d);
                    }
                    ready.store(true, Ordering::SeqCst);
                    std::future::pending::<()>().await;
                })
            }
            Some(EXTRACT_INSTRUCTION) => {
                Box::pin(futures::stream::iter(one_shot(&self.extraction_parts)))
            }
            Some(CONSOLIDATE_INSTRUCTION) => {
                if let Some(next) = self.consolidation_queue.lock().unwrap().pop_front() {
                    Box::pin(futures::stream::iter(vec![Ok(ModelDelta::Text(next))]))
                } else {
                    Box::pin(futures::stream::iter(one_shot(&self.consolidation_parts)))
                }
            }
            Some(PROFILE_INSTRUCTION) => {
                Box::pin(futures::stream::iter(one_shot(&self.profile_parts)))
            }
            // G37 起名请求：固定吐标题文本、不消耗回答队列 —— 与 ReplyModel 的
            // 分流同一条理由（起名任务插队会吃掉下一轮主回答，panic 就在兜底臂）。
            Some(crate::agent::summarize::TITLE_INSTRUCTION) => Box::pin(futures::stream::iter(
                vec![Ok(ModelDelta::Text(TEST_TITLE_REPLY.to_string()))],
            )),
            // 回答那一支：取完了就是夹具写错（轮数不够），不许安静地回一条空消息把测试做绿。
            _ => {
                let reply = self
                    .replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("夹具的回答轮数不够");
                Box::pin(futures::stream::iter(
                    vec![ModelDelta::Text(reply)].into_iter().map(Ok),
                ))
            }
        }
    }
}

/// 40 条 user/assistant 交替 = **20 轮**，每条**恰好 800 个 ASCII 字符** ⇒ 单条
/// `4 + 800/4 = 204 token`，与 `context.rs` 的 `mod compaction_planning` 同一个夹具口径。
/// 内容带行号（`000…012`），所以「哪一段被丢、哪一段留下」能逐字对撞，而不是靠数条数。
fn long_history() -> Vec<Message> {
    (0..40)
        .map(|i| Message {
            role: if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            },
            blocks: vec![ContentBlock::Text {
                text: format!("{i:0>800}"),
            }],
        })
        .collect()
}

/// 上面那份夹具在这组参数下**必然**越线，且 `compaction_plan` 恰好压掉最老的 10 轮 = 20 条
/// （`40/2 = 20`、`keep_turns: 10` ⇒ 刀口 `starts[10] = 20`）。算术本身归 `context.rs` 测，
/// 这里要的只是「压缩一定会发生」这个前提。
fn compacting_cfg() -> ContextConfig {
    ContextConfig {
        budget_tokens: 1_024,
        reserve_output: 0,
        keep_turns: 10,
        ..Default::default()
    }
}

fn seed(db: &Db, session_id: &str, msgs: &[Message]) {
    for m in msgs {
        db.append_message(session_id, m).unwrap();
    }
}

/// Step 1–4 共用的一次压缩。`ContextBuilder` 与生产同一处造出来（造一次、然后 move 进
/// `SessionDeps`），所以这里也是 `new(cfg)` 之后再借 `&builder`（§2a 的形状）。
/// T5 起多带一份记忆开关：压缩段对记忆的全部行为都由它定，夹具不许替调用方暗定。
/// T6 起多带一份嵌入客户端:`None` = off/构造失败,写路径整段跳过;
/// 假实现由 T6 的写路径测试注入。
// 8 个参数是 Step 1–4 共用夹具的真实签名，收拢成参数对象属于签名重构，超出本次机械清理范围。
#[allow(clippy::too_many_arguments)]
async fn compact_once(
    f: &Fixture,
    model: &Arc<FakeModel>,
    sid: &str,
    history: Vec<Message>,
    cfg: ContextConfig,
    memory: &MemoryTurnCfg,
    embed: Option<Arc<dyn EmbedProvider>>,
    cancel: &CancellationToken,
) -> (Vec<Message>, Option<AgentEvent>, usize) {
    // 每个测试夹具自带一把空锁:测试是顺序调用,不存在并发抽取的场景。
    // 并发语义由 `extract_locks_*` 那一组专测覆盖。
    let locks = ExtractLocks::default();
    compact_before_turn(
        &f.db,
        sid,
        &**model,
        &ContextBuilder::new(cfg),
        history,
        memory,
        embed,
        &locks,
        cancel,
    )
    .await
    .expect("压缩段本身不许报错（摘要失败的降级由另一条测试管）")
}

// ==== P1:MemorySaved 的可见性(spec §12)====

/// 记住了就说一声,而且数字要对。位置:排在 `context_compacted` 之后、本轮回答之前
/// —— 先「忘了多少」、再「记住了多少」、最后才回答。
#[tokio::test]
async fn a_compaction_that_saved_memories_emits_memory_saved_with_the_count() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    // 两条**不同键**的候选:同键会走合并(夹具没配合并支),只剩一条。
    let model = FakeModel::extracting(
        &[
            r#"[{"kind":"fact","subject":"用户","predicate":"住在","object":"上海","body":"用户搬到上海了","confidence":0.95},
             {"kind":"preference","subject":"用户","predicate":"偏好","object":"简洁","body":"用户喜欢简洁回答","confidence":0.9}]"#,
        ],
        &["好的"],
    );
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: compacting_cfg(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "上海那边怎么样".into(),
        }],
    )
    .await
    .expect("压缩 + 一轮发送必须整体成功");
    sink.flush();

    let tags: Vec<&str> = rec.frames().iter().map(event_tag).collect();
    assert_eq!(
        tags.first().copied(),
        Some("context_compacted"),
        "压缩通知排最前:{tags:?}"
    );
    let saved_at = tags
        .iter()
        .position(|t| *t == "memory_saved")
        .unwrap_or_else(|| panic!("记住了两条却没有 memory_saved:{tags:?}"));
    assert_eq!(
        tags[saved_at - 1],
        "context_compacted",
        "先说忘了多少,再说记住了多少:{tags:?}"
    );
    assert!(
        tags[saved_at..].contains(&"text_delta"),
        "记住的通知必须排在本轮回答之前:{tags:?}"
    );
    match &rec.frames()[saved_at] {
        AgentEvent::MemorySaved { count } => assert_eq!(*count, 2, "两条候选 ⇒ 2"),
        other => panic!("第 {saved_at} 帧不是 MemorySaved:{other:?}"),
    }
    // 用户可见面:只说「记住了几条」,记忆正文一个字都不许进帧流(R69 的推广)。
    let wire = serde_json::to_string(&rec.frames()).unwrap();
    assert!(!wire.contains("用户搬到上海了"), "记忆正文泄漏:{wire}");
    assert!(!wire.contains("用户喜欢简洁回答"), "记忆正文泄漏:{wire}");
}

/// 零条不弹:白弹一句「已记住 0 条」只会让用户以为它坏了。
#[tokio::test]
async fn a_compaction_that_saved_nothing_emits_no_memory_saved() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    // 默认抽取支回 `[]`:一次成功的「没什么值得记」。
    let model = FakeModel::summarizing_and_replying(&["早期要点:聊了些日常"], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: compacting_cfg(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "随便聊聊".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    assert!(
        !rec.frames()
            .iter()
            .any(|e| matches!(e, AgentEvent::MemorySaved { .. })),
        "零条时不许发 MemorySaved"
    );
}

/// 摘要文本**绝不进用户可见的事件流**（brief §1 的那句话）。
/// 用户可见的出口只有两处，这里两处都查：① `compact_before_turn` 交回、由 `run_turn`
/// emit 的那一条事件；② 重放投影 `events_from_messages`（前端重开会话走的那条路）。
/// 摘要调用压根拿不到 sink —— 它只在模型的返回流里存在过一次。
#[tokio::test]
async fn the_summary_call_never_leaks_into_the_event_stream() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::summarizing(&["早期要点：用户叫林，", "卡号尾 4021"]);
    let (_, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    // 压了就要说，而且只说这一件事（两个数：dropped / summary_chars）。
    let event = event.expect("越线的历史必须交回一条 ContextCompacted");
    assert!(matches!(event, AgentEvent::ContextCompacted { .. }));
    let wire = serde_json::to_string(&event).unwrap();
    assert!(
        !wire.contains("早期要点"),
        "摘要文本混进了 emit 的事件里：{wire}"
    );

    // 落库那条是 `Role::System` ⇒ 重放里压根没有它（R69 那一臂；判据 (b) 打的就是这里）。
    let replay = events_from_messages(&f.db.messages(&sid).unwrap(), &|n| f.registry.risk_of(n));
    let visible = serde_json::to_string(&replay).unwrap();
    assert!(
        !visible.contains("早期要点"),
        "摘要进了重放事件流 ⇒ 用户会看到一句莫名其妙的小结"
    );
    assert!(
        !replay
            .iter()
            .any(|e| matches!(e, AgentEvent::ContextCompacted { .. })),
        "压缩事件是直播时发的一条通知，不是历史（库里没有这一列，重放也不该编造它）"
    );
    assert_eq!(
        model.calls().len(),
        2,
        "一次压缩 = 摘要一次 + 记忆抽取一次(抽取默认回 [],零候选也算真跑过)"
    );
}

/// §9「存在一条覆盖被丢弃区间的摘要消息」的可测形式，加上「不重复」。
/// 覆盖范围从**模型那一侧**取证（R57）：摘要调用的输入必须逐字等于被删掉的那一段 ——
/// 只断言「库里有一条备注」是不够的，输入对不上就等于覆盖范围没兜住。
#[tokio::test]
async fn the_summary_row_is_written_once_and_covers_the_dropped_range() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::summarizing(&["早期要点：", "用户叫林，卡号尾 4021"]);
    let (history, _, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    let after = f.db.messages(&sid).unwrap();
    let notes: Vec<&Message> = after.iter().filter(|m| m.role == Role::System).collect();
    assert_eq!(
        notes.len(),
        1,
        "每次压缩一条备注；两条备注意味着同一段历史被摘要了两遍"
    );
    assert_eq!(
        notes[0].text_of(),
        summary_note("早期要点：用户叫林，卡号尾 4021"),
        "备注 = 前缀 + 摘要正文（分片拼接后的原文）"
    );
    assert!(
        notes[0].text_of().starts_with(SUMMARY_PREFIX),
        "前缀必须有，而且只加一次：`build()` 把 System 行当 notes 原样带上"
    );
    assert!(
        after.len() < all.len(),
        "被压掉的行必须真的没了，否则下一轮还是全量读"
    );
    assert_eq!(after.len(), 21, "40 删 20 加 1 条备注");

    let req = model.calls().remove(0);
    assert_eq!(
        req.messages.len(),
        1 + 20,
        "摘要输入 = 被丢掉的那 20 条，一条不多一条不少"
    );
    assert_eq!(
        &req.messages[1..],
        &all[..20],
        "摘要输入必须逐字是被丢的那一段（最老 10 轮）"
    );
    let conv: Vec<&Message> = after.iter().filter(|m| m.role != Role::System).collect();
    assert_eq!(
        conv,
        all[20..].iter().collect::<Vec<_>>(),
        "留下的必须是刀口之后那 20 条，原样原序"
    );
    assert_eq!(history, after, "交回给调用方的历史必须就是重读回来的那份");
}

/// spec :365 的降级：摘要调用报错（429、断流）时**照压**，正文换成 `[早期对话已省略]`。
/// 关键词是「照压」—— 不许有第二条「丢弃」路径，落库走的还是同一条 `compact_session`，
/// 该删的行照删（否则下一轮照样超预算，而这次连摘要都没了）。
#[tokio::test]
async fn a_failing_summary_still_frees_space_and_says_so() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let before = f.db.messages(&sid).unwrap();
    // 报错之前先到了半句：那半句**不是**摘要，也不许落进备注。
    let model = FakeModel::failing(&["早期要点：用户叫"]);
    let (history, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    let after = f.db.messages(&sid).unwrap();
    let notes: Vec<&Message> = after.iter().filter(|m| m.role == Role::System).collect();
    assert_eq!(notes.len(), 1, "降级也只落一条备注");
    assert_eq!(
        notes[0].text_of(),
        summary_note(SUMMARY_OMITTED),
        "降级只换正文，不换事务"
    );
    assert!(
        !notes[0].text_of().contains("早期要点"),
        "半截摘要被当成了摘要落库：{}",
        notes[0].text_of()
    );
    let conv: Vec<&Message> = after.iter().filter(|m| m.role != Role::System).collect();
    assert_eq!(
        conv,
        all[20..].iter().collect::<Vec<_>>(),
        "摘要失败也不能把该删的留着撑爆下一轮"
    );
    assert_eq!(after.len(), 21);

    // 「压了」这件事照样要说，而且说的就是删掉的行数与备注的字数。
    match event.expect("降级路径同样必须 emit") {
        AgentEvent::ContextCompacted {
            dropped,
            summary_chars,
        } => {
            assert_eq!(dropped, 20);
            assert_eq!(
                summary_chars,
                summary_note(SUMMARY_OMITTED).chars().count(),
                "字数是 chars().count() 不是 len()：前端文案「摘要 {{n}} 字」，\
                 用字节数中文会显示成 3 倍"
            );
        }
        other => panic!("降级也要说清楚压了多少，拿到的是 {other:?}"),
    }
    assert_eq!(history, after);
}

/// 取消发生在「模型还在写摘要」时：事务压根还没开始，所以库必须**一字未改**（原子性）。
/// 半截摘要同样不许当成成功（那条合同在 `summarize.rs` 的
/// `cancelling_a_summary_returns_an_error_and_keeps_nothing_back`）。
#[tokio::test]
async fn cancelling_mid_compaction_leaves_a_consistent_store() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::stalling(&["早期要点：用户叫林，"]);
    let cancel = CancellationToken::new();
    // join! 的两个 future 都借 `memory` —— 临时值活不过语句,必须先落成 let。
    let memory = MemoryTurnCfg::default();
    let ready = model.stalled.clone();
    let builder = ContextBuilder::new(compacting_cfg());
    // 同步点而非 sleep：`ready` 置位时分片已经抵达消费者，摘要此刻确实还挂在流上。
    // 临时值必须落成 let:join! 的两个 future 都借它,语句结束就失效。
    let locks = ExtractLocks::default();
    let (got, _) = tokio::join!(
        compact_before_turn(
            &f.db,
            &sid,
            &*model,
            &builder,
            before.clone(),
            &memory,
            None,
            &locks,
            &cancel
        ),
        async {
            while !ready.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            cancel.cancel();
        }
    );
    let (history, event, _) = got.expect("取消不是失败：压缩段必须正常返回");
    assert!(event.is_none(), "没压成就别说压了：{event:?}");
    assert_eq!(
        history, before,
        "交回的历史必须是原样那份，否则调用方的 `prior` 会跟着错"
    );
    assert_eq!(
        f.db.messages(&sid).unwrap(),
        before,
        "取消不许留下半条摘要、也不许留下已经删掉的行"
    );
    assert_eq!(
        model.calls().len(),
        1,
        "夹具前提：摘要调用确实发起过并被取消"
    );
}

/// 没越线 ⇒ 一次模型调用都不发起、一个字节都不写。
/// 这条看着像废话，但它守着的是「压缩段的 IO 全部在 `if let Some(plan)` 里面」：
/// 把 `collect_summary` 挪到判定之外，用户每次发消息都要白烧一次摘要调用（花钱、且慢）。
#[tokio::test]
async fn an_under_threshold_history_costs_no_model_call_and_writes_nothing() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let short = vec![
        Message::user(vec![ContentBlock::Text { text: "一".into() }]),
        Message {
            role: Role::Assistant,
            blocks: vec![ContentBlock::Text { text: "二".into() }],
        },
    ];
    seed(&f.db, &sid, &short);
    let model = FakeModel::summarizing(&["不该被调用的摘要"]);
    let (history, event, _) = compact_once(
        &f,
        &model,
        &sid,
        short.clone(),
        ContextConfig::default(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_none(), "没压就不许 emit：{event:?}");
    assert_eq!(history, short, "不压时交回的就是进来的那份");
    assert_eq!(
        model.calls().len(),
        0,
        "没越线一次都不该调模型：摘要调用要花钱"
    );
    assert_eq!(
        f.db.messages(&sid).unwrap(),
        short,
        "不压时库里一个字都不动"
    );
}

/// 落库失败：整轮必须回滚、`Error{fatal}` 先于 `Done{Error}`、命令返回 Err。
/// 这就是 `defer_done` 的存在理由 —— 先发 Done 再报错会让界面宣称完成，
/// 而库里一行都没有。
#[tokio::test]
async fn a_failed_append_leaves_no_partial_rows_and_emits_error_before_done() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    // 第二个连接建触发器：只要 blocks_json 里出现标记就 ABORT 这次 INSERT。
    let conn = rusqlite::Connection::open(f._dir.path().join("qwen-do.db")).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER boom BEFORE INSERT ON messages \
         WHEN NEW.blocks_json LIKE '%落库失败标记%' \
         BEGIN SELECT RAISE(ABORT, 'boom'); END;",
    )
    .unwrap();

    let model = FakeModel::summarizing_and_replying(&[], &["落库失败标记"]);
    let (result, frames) = f.turn(&sid, model, "这一轮不该留在库里").await;
    assert!(result.is_err(), "落库失败必须让这一轮返回 Err");
    assert!(
        f.db.messages(&sid).unwrap().is_empty(),
        "append 是单事务：user 行也必须跟着回滚"
    );

    let err_at = frames
        .iter()
        .position(|e| {
            matches!(
                e,
                AgentEvent::Error {
                    at: ErrorWhere::Store,
                    fatal: true,
                    ..
                }
            )
        })
        .expect("必须发一条 fatal 的 Store 错误");
    let done_at = frames
        .iter()
        .rposition(|e| {
            matches!(
                e,
                AgentEvent::Done {
                    stop_reason: StopReason::Error
                }
            )
        })
        .expect("必须发 Done{Error} 收口");
    assert!(err_at < done_at, "错误必须先于 Done：{frames:?}");
}

/// Step 6.3 的判据，也是本组**唯一**走 `run_turn` 的一条。
/// 为什么只能走它：`prior` 这一行下标住在 `run_turn` 里（原 :304 / :323），
/// brief §4 点名的失败模式（`prior` 停在压缩前的 40，而压缩后的历史只有 21 行
/// ⇒ `session.history()[prior..]` 当场越界）发生在装配层的 move/重读**顺序**上，
/// 抽出来的 `compact_before_turn` 里压根没有这个下标 —— 判据 (a)「把 prior 放回压缩前」
/// 要红的就是这条测试。
#[tokio::test]
async fn prior_is_recounted_after_compaction() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let model = FakeModel::summarizing_and_replying(&["早期要点：用户叫林"], &["本轮的回复"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: compacting_cfg(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            skills_dir: None,
            skills_disabled: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "本轮的提问".into(),
        }],
    )
    .await
    .expect("压缩 + 一轮发送必须整体成功");
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn);

    let after = f.db.messages(&sid).unwrap();
    // 压缩后 21 行 + 本轮 2 行 = 23。`prior` 停在压缩前的 40 时，
    // `session.history()[40..]` 在 23 行上直接 panic（测试表现为失败），
    // 而「少附」的写法会把本轮那两行整轮丢掉 —— 两种退化都过不了下面三条。
    assert_eq!(
        after.len(),
        21 + 2,
        "总行数 = 压缩后行数 + 本轮新增两行；实际 {}",
        after.len()
    );
    assert_eq!(
        after[after.len() - 2].text_of(),
        "本轮的提问",
        "倒数第二行必须是本轮的用户行"
    );
    assert_eq!(
        after[after.len() - 1].text_of(),
        "本轮的回复",
        "最后一行必须是本轮的助手行"
    );
    assert!(
        after[..21]
            .iter()
            .all(|m| m.role != Role::User || m.blocks.len() == 1),
        "留下那 21 行的形状不许被本轮污染"
    );

    // 事件顺序（Step 6.5 / brief §4）：`context_compacted` 在写新行之前发，
    // 也就是排在本轮的回答之前；且摘要文本不在任何一帧里。
    let tags: Vec<&str> = rec.frames().iter().map(event_tag).collect();
    assert_eq!(
        tags.first().copied(),
        Some("context_compacted"),
        "压缩通知必须排在整轮回答之前，实际顺序 {tags:?}"
    );
    let wire = serde_json::to_string(&rec.frames()).unwrap();
    assert!(
        !wire.contains("早期要点"),
        "摘要文本进了用户可见的事件流：{wire}"
    );
    assert_eq!(
        model.calls().len(),
        3,
        "摘要一次 + 记忆抽取一次(默认零候选) + 回答一次"
    );
}

// ==== 长期记忆:压缩钩子的抽取/合并/画像(T4)====

use crate::agent::memory::{CONSOLIDATE_INSTRUCTION, EXTRACT_INSTRUCTION, PROFILE_INSTRUCTION};
use crate::store::memory::NewMemory;

fn fact_candidate(object: &str, body: &str) -> String {
    format!(
        r#"{{"kind":"fact","subject":"用户","predicate":"住在","object":"{object}","body":"{body}","confidence":0.95}}"#
    )
}

fn profile_candidate(body: &str) -> String {
    format!(
        r#"{{"kind":"profile","subject":null,"predicate":null,"object":null,"body":"{body}","confidence":0.9}}"#
    )
}

/// 抽取产物真的落了库,而且输入逐字等于被删的那段(R57 同款取证)。
#[tokio::test]
async fn compaction_extracts_memories_from_the_dropped_segment_and_lands_them() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    // 无同键旧记忆 ⇒ 走零调用快路径,合并那一支不会被问到。
    let (history, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_some(), "压缩本体照常发生");

    let mems = f.db.valid_memories().unwrap();
    assert_eq!(mems.len(), 1, "抽取出的唯一事实必须落库");
    assert_eq!(mems[0].body, "用户搬到上海");
    assert_eq!(
        mems[0].session_id.as_deref(),
        Some(sid.as_str()),
        "来源会话要随写入落上"
    );

    // 抽取输入 = 被删的那 20 条,与摘要同一个 source(「即将永久消失的原文」)。
    let calls = model.calls();
    assert_eq!(
        calls[0].messages.first().unwrap().text_of(),
        SUMMARY_INSTRUCTION
    );
    assert_eq!(
        calls[1].messages.first().unwrap().text_of(),
        EXTRACT_INSTRUCTION
    );
    assert_eq!(
        &calls[1].messages[1..],
        &all[..20],
        "抽取输入必须逐字是被丢的那一段(与摘要同一份 source)"
    );
    // 压缩本体没有因此变形:仍然只有一条摘要备注、仍然压掉 20 条。
    let after = f.db.messages(&sid).unwrap();
    assert_eq!(after.len(), 21);
    assert_eq!(history, after);
}

/// 抽取输出是垃圾 ⇒ 本轮零写入、压缩照常:功能降级,不留半条状态。
#[tokio::test]
async fn a_garbage_extraction_degrades_to_no_memory_writes_but_compaction_stands() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extracting(&["这段对话没什么值得记的"], &[]);
    let (_, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_some(), "抽取失败不许阻塞压缩");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "垃圾输出不许产出半条记忆"
    );
    assert_eq!(f.db.messages(&sid).unwrap().len(), 21, "压缩照常完成");
    assert_eq!(model.calls().len(), 2, "摘要 + 抽取,各一次");
}

/// 用户在抽取还挂着的时候按停止:与摘要侧同一条语义 —— 事务还没开始,
/// 一个字节都不写,历史原样交回。
#[tokio::test]
async fn cancelling_during_extraction_leaves_the_history_untouched() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extraction_stalling(&[r#"[{"kind":"fact""#]);
    let cancel = CancellationToken::new();
    // 同上:join! 的两个 future 都借 `memory`,临时值必须先落成 let。
    let memory = MemoryTurnCfg::default();
    let ready = model.stalled.clone();
    let builder = ContextBuilder::new(compacting_cfg());
    // 临时值必须落成 let:join! 的两个 future 都借它,语句结束就失效。
    let locks = ExtractLocks::default();
    let (got, _) = tokio::join!(
        compact_before_turn(
            &f.db,
            &sid,
            &*model,
            &builder,
            before.clone(),
            &memory,
            None,
            &locks,
            &cancel
        ),
        async {
            // 超时护栏:抽取侧永远不被调用时(实现缺失)不许把测试挂死,超时即按停止。
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !ready.load(Ordering::SeqCst) {
                    tokio::task::yield_now().await;
                }
            })
            .await;
            cancel.cancel();
        }
    );
    let (history, event, _) = got.expect("取消不是失败");
    assert!(event.is_none(), "没压成就别说压了:{event:?}");
    assert_eq!(history, before, "交回的历史必须原样");
    assert_eq!(f.db.messages(&sid).unwrap(), before, "库里一个字都不动");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "取消不许留下半条记忆"
    );
}

#[test]
fn the_skills_view_lists_name_description_and_folder() {
    let dir = tempfile::tempdir().unwrap();
    let skill = dir.path().join("translate");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: 翻译助手\ndescription: 中英互译\n---\n正文\n",
    )
    .unwrap();

    let view = skills_view_of(dir.path(), &crate::skills::Selection::default());
    assert_eq!(view.root, dir.path().display().to_string());
    assert_eq!(view.skills.len(), 1, "{:?}", view.skills);
    assert_eq!(view.skills[0].name, "翻译助手");
    assert_eq!(view.skills[0].description, "中英互译");
    assert_eq!(view.skills[0].folder, "translate");
    assert!(view.skills[0].enabled, "没有选择表时默认启用");

    let mut sel = crate::skills::Selection::default();
    sel.disabled.push("translate".to_string());
    let view = skills_view_of(dir.path(), &sel);
    assert!(
        !view.skills[0].enabled,
        "停用的是 enabled=false，不是列表里消失"
    );

    let empty = tempfile::tempdir().unwrap();
    let view = skills_view_of(empty.path(), &sel);
    assert!(view.skills.is_empty(), "空目录要交空表，而不是报错");
    assert_eq!(view.root, empty.path().display().to_string());
}

#[test]
fn toggling_a_skill_persists_and_rejects_unknown_folders() {
    let skills = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let skill = skills.path().join("translate");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: 翻译助手\ndescription: 中英互译\n---\n正文\n",
    )
    .unwrap();

    set_skill_enabled_of(skills.path(), data.path(), "translate", false).unwrap();
    assert_eq!(
        crate::skills::load_selection(data.path()).disabled,
        vec!["translate".to_string()],
        "停用要落盘（skills.json）"
    );
    assert!(!crate::skills::is_enabled(
        &crate::skills::load_selection(data.path()),
        "translate"
    ));

    set_skill_enabled_of(skills.path(), data.path(), "translate", true).unwrap();
    assert!(
        crate::skills::load_selection(data.path())
            .disabled
            .is_empty(),
        "重新启用要把这条删掉"
    );

    let e = set_skill_enabled_of(skills.path(), data.path(), "../etc", false)
        .expect_err("不存在的文件夹必须拒，选择表里不许出现幽灵记录");
    assert!(e.contains("没有这个技能"), "{e}");
}

/// profile 候选不进检索库,经归并后落成**单行**画像。
#[tokio::test]
async fn profile_candidates_merge_into_the_single_profile_row() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::with_profile(
        FakeModel::extracting(&[&format!("[{}]", profile_candidate("用户在学日语"))], &[]),
        &["用户是 Rust 开发者,在学日语"],
    );
    let _ = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "profile 候选不进检索库"
    );
    assert_eq!(
        f.db.profile().unwrap(),
        "用户是 Rust 开发者,在学日语",
        "归并结果落成单行画像"
    );
    let calls = model.calls();
    assert_eq!(calls.len(), 3, "摘要 + 抽取 + 画像归并");
    assert_eq!(
        calls[2].messages.first().unwrap().text_of(),
        PROFILE_INSTRUCTION
    );
}

/// 同键冲突 ⇒ 问模型三选一;模型说 INVALIDATE ⇒ 旧行失效、新行**不**加。
#[tokio::test]
async fn a_conflicting_candidate_asks_the_model_and_applies_its_choice() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let old_id =
        f.db.add_memory(&NewMemory {
            session_id: Some(sid.clone()),
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some("北京".into()),
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
            &[],
        ),
        &[&format!(r#"{{"action":"INVALIDATE","id":{old_id}}}"#)],
    );
    let _ = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    let audit = f.db.all_memories().unwrap();
    assert_eq!(audit.len(), 1, "INVALIDATE 不添新行");
    assert_eq!(audit[0].id, old_id);
    assert!(audit[0].invalid_at.is_some(), "旧行必须被打上失效时间");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "失效后检索视角必须为空"
    );
    let calls = model.calls();
    assert_eq!(calls.len(), 3, "摘要 + 抽取 + 合并");
    assert_eq!(
        calls[2].messages.first().unwrap().text_of(),
        CONSOLIDATE_INSTRUCTION
    );
}

/// 第四种合并答案:新旧实质是同一件事(措辞不同)⇒ 不新增、不改正文,
/// 只把该行强度加一 —— 「重复接触使记忆变强」的落库形状。
#[tokio::test]
async fn a_reinforcing_candidate_bumps_the_existing_row_strength_without_a_new_row() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let old_id =
        f.db.add_memory(&NewMemory {
            session_id: Some(sid.clone()),
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some("北京".into()),
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("北京", "用户在北京居住"))],
            &[],
        ),
        &[&format!(r#"{{"action":"REINFORCE","id":{old_id}}}"#)],
    );
    let _ = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    let audit = f.db.all_memories().unwrap();
    assert_eq!(audit.len(), 1, "强化不添新行");
    assert_eq!(audit[0].id, old_id);
    assert_eq!(audit[0].reinforcements, 1, "同一事实再说一遍必须记一次强度");
    assert_eq!(
        audit[0].body, "用户住在北京",
        "强化不许动正文(改正文是 UPDATE 的事)"
    );
    assert!(audit[0].invalid_at.is_none(), "强化不等于失效");
    let calls = model.calls();
    assert_eq!(calls.len(), 3, "摘要 + 抽取 + 合并");
    assert_eq!(
        calls[2].messages.first().unwrap().text_of(),
        CONSOLIDATE_INSTRUCTION
    );
}

/// 模型编了一个不存在的 id:这条变更指向不存在行,若交给事务会整笔失败
/// (压缩也一起回滚)。管线必须在写入前丢掉它,压缩照常完成。
#[tokio::test]
async fn a_reinforce_decision_for_a_missing_id_is_dropped_and_compaction_stands() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let old_id =
        f.db.add_memory(&NewMemory {
            session_id: Some(sid.clone()),
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some("北京".into()),
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("北京", "用户在北京居住"))],
            &[],
        ),
        // 存在的行是 old_id,模型却报了一个不存在的 id(常见于它自己编序号)。
        &[&format!(
            r#"{{"action":"REINFORCE","id":{}}}"#,
            old_id + 999
        )],
    );
    let (_, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;

    assert!(event.is_some(), "跳过一条候选不许影响压缩");
    let audit = f.db.all_memories().unwrap();
    assert_eq!(audit.len(), 1, "不许新增行");
    assert_eq!(audit[0].reinforcements, 0, "不许误给任何行加计数");
    assert_eq!(f.db.messages(&sid).unwrap().len(), 21, "压缩照常完成");
}

// ==== 长期记忆:注入的装配(T4)====

/// `memory_notes_for_turn` 的装配规则:画像在前、召回按相关度、超限砍召回不砍画像。
#[test]
fn memory_notes_assemble_profile_first_then_ranked_recall_within_the_cap() {
    let f = Fixture::new();
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    for (object, body) in [("北京", "用户住在北京"), ("咖啡", "用户喜欢咖啡")] {
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some(object.into()),
            body: body.into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    }

    let notes = memory_notes_for_turn(&f.db, "北京有什么好玩的", None, 5, 600, 24, 60.0).unwrap();
    // T10:顺序是 画像 → 召回 → 概览;概览因为存在记忆而必须出现。
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0], "[用户画像]用户是 Rust 开发者");
    assert!(
        notes[1].starts_with("[长期记忆"),
        "召回备注必须有可识别前缀:{}",
        notes[1]
    );
    assert!(
        notes[1].contains("用户住在北京"),
        "与查询相关的记忆必须在召回里:{}",
        notes[1]
    );
    assert!(
        !notes[1].contains("咖啡"),
        "零重叠的记忆不许混进召回:{}",
        notes[1]
    );
    assert!(
        notes[2].starts_with("[记忆库]"),
        "概览必须是最后一条备注:{}",
        notes[2]
    );

    // 上限砍的是召回与概览,不是画像:cap 收到只够画像时,两条都让路。
    let tight = memory_notes_for_turn(&f.db, "北京有什么好玩的", None, 5, 20, 24, 60.0).unwrap();
    assert_eq!(tight.len(), 1, "画像永不因超限丢失");
    assert!(tight[0].starts_with("[用户画像]"));

    // k=0 = 召回关闭;画像与概览仍在(概览不依赖召回)。
    let no_recall =
        memory_notes_for_turn(&f.db, "北京有什么好玩的", None, 0, 600, 24, 60.0).unwrap();
    assert_eq!(no_recall.len(), 2, "画像 + 概览");
    assert!(no_recall[1].starts_with("[记忆库]"));

    // 空库 = 无备注,消费方(注入)据此完全跳过。
    let empty = Fixture::new();
    assert!(
        memory_notes_for_turn(&empty.db, "随便什么", None, 5, 600, 24, 60.0)
            .unwrap()
            .is_empty()
    );
}

/// run_turn 级的端到端:画像与召回进的是**发给模型的请求头**,而库里零新增行
/// (注入是易失的,不许渗进持久层 —— dropped/行数都不许动)。
#[tokio::test]
async fn run_turn_carries_profile_and_recall_in_the_request_head_only() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("北京".into()),
        body: "用户住在北京".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    seed(
        &f.db,
        &sid,
        &[
            Message::user(vec![ContentBlock::Text {
                text: "上一轮".into(),
            }]),
            Message {
                role: Role::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: "上一答".into(),
                }],
            },
        ],
    );
    let model = FakeModel::build(&[], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "北京有什么好玩的".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn);

    let calls = model.calls();
    let answer = &calls[0]; // 短历史不压缩,第一只调用就是回答
    assert_eq!(answer.messages[0].role, Role::System);
    assert!(
        answer.messages[1].text_of().starts_with("[用户画像]"),
        "画像必须紧跟系统提示词:{}",
        answer.messages[1].text_of()
    );
    assert!(
        answer.messages[2].text_of().starts_with("[长期记忆"),
        "召回必须排在画像之后:{}",
        answer.messages[2].text_of()
    );
    assert!(answer.messages[2].text_of().contains("用户住在北京"));

    // 注入是易失的:库里只多了本轮两行,没有 memory 行、没有 system 备注。
    let after = f.db.messages(&sid).unwrap();
    assert_eq!(after.len(), 4, "上一轮两行 + 本轮两行");
    assert!(
        !after.iter().any(|m| m.role == Role::System),
        "记忆备注不许落库"
    );
    assert_eq!(f.db.valid_memories().unwrap().len(), 1, "记忆本体没动");
}

// ==== T13：类人记忆的端到端（强化 / 衰减 / 待办 / 多轮指代 / 元记忆）====

/// 一段「人生」:同一件事被反复确认 → 强度上升;旧事因时间下沉但仍可被查到;
/// 待办到期被提醒一次;追问时靠上一轮的消息召回;概览告诉模型库里有东西。
/// 这条测试是升级后所有机制的**接缝**检查 —— 单条机制各有专测,这里证明它们
/// 在同一份库上互不打架。
#[tokio::test]
async fn a_memory_lifecycle_holds_together_across_confirmations_decay_and_due_dates() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();

    // ---- ① 同一事实被压缩抽到两次:第一次 ADD,第二次 REINFORCE ----
    seed(&f.db, &sid, &long_history());
    let first = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海了"))],
        &[],
    );
    let before = f.db.messages(&sid).unwrap();
    compact_once(
        &f,
        &first,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    let after_first = f.db.valid_memories().unwrap();
    assert_eq!(after_first.len(), 1);
    let memory_id = after_first[0].id;
    assert_eq!(after_first[0].reinforcements, 0, "第一次只是 ADD");

    // 第二次:再来一轮长历史并压缩,模型认出「是同一件事」→ REINFORCE。
    let more: Vec<Message> = (0..40)
        .map(|i| Message {
            role: if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            },
            blocks: vec![ContentBlock::Text {
                text: format!("第二轮历史 {i:0>800}"),
            }],
        })
        .collect();
    seed(&f.db, &sid, &more);
    let second = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("上海", "用户现在在上海"))],
            &[],
        ),
        &[&format!(r#"{{"action":"REINFORCE","id":{memory_id}}}"#)],
    );
    let before2 = f.db.messages(&sid).unwrap();
    compact_once(
        &f,
        &second,
        &sid,
        before2,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    let strengthened = f.db.valid_memories().unwrap();
    assert_eq!(strengthened.len(), 1, "REINFORCE 不添新行");
    assert_eq!(strengthened[0].id, memory_id);
    assert_eq!(strengthened[0].reinforcements, 1, "强度 +1");
    assert_eq!(
        strengthened[0].body, "用户搬到上海了",
        "REINFORCE 不动正文(改写是 UPDATE 的事)"
    );

    // ---- ② 一条很久以前的事:衰减让它排在后面,但绝不消失 ----
    let old =
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("养过".into()),
            object: Some("仓鼠".into()),
            body: "用户在上海养过一只仓鼠".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    {
        let c = f.db.lock();
        let ancient = Db::now_ms() - 900 * 86_400_000;
        c.execute(
            "UPDATE memories SET valid_from=?2, updated_at=?2 WHERE id=?1",
            params![old, ancient],
        )
        .unwrap();
    }
    let hits =
        f.db.search_memories("上海", None, 10, &RecallCfg::default())
            .unwrap();
    let ids: Vec<i64> = hits.iter().map(|m| m.id).collect();
    assert!(
        ids.contains(&memory_id) && ids.contains(&old),
        "两条都要在:{ids:?}"
    );
    assert_eq!(ids[0], memory_id, "新鲜的/被确认过的在前,陈年的在后");

    // ---- ③ 待办:本轮提醒一次,下一轮不再提 ----
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "prospective".into(),
        subject: None,
        predicate: None,
        object: None,
        body: "下周三把合同发给老王".into(),
        confidence: 0.95,
        due_at: Some(Db::now_ms() + 3_600_000),
    })
    .unwrap();

    let answerer = FakeModel::build(&[], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: answerer.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "上海那边最近怎么样".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();

    let head = &answerer.calls()[0];
    let systems: Vec<String> = head
        .messages
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.text_of())
        .collect();
    let due_at = systems
        .iter()
        .position(|t| t.starts_with("[待办提醒]"))
        .expect("到期的待办必须被提醒");
    let recall_at = systems
        .iter()
        .position(|t| t.starts_with("[长期记忆"))
        .expect("相关记忆必须被召回");
    assert!(due_at < recall_at, "待办要排在召回之前(最紧急的先被看到)");
    assert!(
        systems[due_at].contains("下周三把合同发给老王"),
        "待办正文要在提醒里:{}",
        systems[due_at]
    );
    let overview_at = systems
        .iter()
        .position(|t| t.starts_with("[记忆库]"))
        .expect("有记忆时必须有概览(元记忆)");
    assert!(recall_at < overview_at, "概览排最后(最元的信息)");
    assert!(
        systems[overview_at].contains("memory_search"),
        "概览要告诉模型可以主动查:{}",
        systems[overview_at]
    );

    // 第二轮:待办已标记,不再提醒。
    let answerer2 = FakeModel::build(&[], &["好"]);
    let rec2 = Arc::new(Recorded::default());
    let sink2 = Arc::new(TauriEmitter::new(rec2.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: answerer2.clone(),
            sink: sink2.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "那它呢".into(),
        }],
    )
    .await
    .unwrap();
    sink2.flush();
    let second_head = &answerer2.calls()[0];
    assert!(
        !second_head
            .messages
            .iter()
            .any(|m| m.text_of().starts_with("[待办提醒]")),
        "同一待办不该刷屏"
    );
    // ---- ④ 多轮指代:上一轮说了「上海」,这一轮只说「那它呢」,仍能召回 ----
    let recall = second_head
        .messages
        .iter()
        .find(|m| m.text_of().starts_with("[长期记忆"))
        .expect("追问必须靠上一轮的消息召回,而不是当前这句「那它呢」");
    assert!(
        recall.text_of().contains("上海"),
        "召回内容要来自上一轮的「上海」:{}",
        recall.text_of()
    );
}

/// 元记忆的主动通路:模型调 `memory_search` 能拿到库里的东西 ——
/// 这条把工具、注册表、存储层串起来(单测各自在测自己那一层)。
#[tokio::test]
async fn the_model_can_actively_search_memories_through_the_registry() {
    let f = Fixture::new();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("在做".into()),
        object: Some("一个桌面助手".into()),
        body: "用户在做一个小宋祺桌面助手".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    let registry =
        build_production_registry(f.db.clone(), Arc::new(RwLock::new(ShellCatalog::new())));
    let out = registry
        .get("memory_search")
        .expect("生产工具表里必须有 memory_search")
        .execute(
            serde_json::json!({"query": "桌面助手"}),
            &crate::tools::registry::ToolCtx {
                write_root: None,
                http: reqwest::Client::new(),
                cancel: CancellationToken::new(),
                roots: Vec::new(),
                skills_dir: None,
                skills_disabled: Vec::new(),
                subagents: None,
                events: None,
            },
        )
        .await
        .unwrap();
    let text = match out {
        crate::agent::types::ToolOutput::Text { text } => text,
        other => panic!("期望文本输出:{other:?}"),
    };
    assert!(
        text.contains("小宋祺桌面助手"),
        "主动检索必须能翻到自动召回漏掉的内容:{text}"
    );
    // 门槛:Risk::Read ⇒ 权限闸零弹窗(与 fs.read 同档)。
    assert_eq!(
        registry.risk_of("memory_search"),
        crate::agent::types::Risk::Read
    );
}

// ==== T7：端到端闭环（压缩→记忆落地→下一轮召回） ====

/// 整条闭环串起来:长历史触发压缩 → 抽取出的事实**落库** → 下一轮发送时它**回到请求头**,
/// 而库里只多本轮两行。分段测试(T1/T2/T4)各自只证明一段;这条证明段与段之间的**接缝**
/// 没错位:抽取输入是真正被删的那段、落库的 session_id 是当前会话、下一轮召回用的是
/// 本轮消息而不是上一轮的 query。
/// 守 spec §8 的不变量 1(est 含记忆)与不变量 6(注入不改行数/投影)。
#[tokio::test]
async fn a_full_memory_cycle_survives_compaction_and_returns_next_turn() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let all = long_history();
    seed(&f.db, &sid, &all);

    // ---- 第一段:压缩时抽取并落库 ----
    let before = f.db.messages(&sid).unwrap();
    let extractor = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海了"))],
        &[],
    );
    let (compacted_history, event, _) = compact_once(
        &f,
        &extractor,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_some(), "夹具前提:长历史必须触发压缩");

    let mems = f.db.valid_memories().unwrap();
    assert_eq!(mems.len(), 1, "抽取的唯一事实必须落库");
    assert_eq!(mems[0].body, "用户搬到上海了");
    assert_eq!(
        mems[0].session_id.as_deref(),
        Some(sid.as_str()),
        "来源会话必须是本会话(接缝:写入侧的 session_id 由编排层补)"
    );

    // ---- 第二段:下一轮发送,记忆回到请求头 ----
    // 换一只干净模型:它只该被问一次(回答),零压缩(压缩后的历史已在阈值内)。
    let answerer = FakeModel::build(&[], &["上海可以去外滩"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let rows_before_turn = f.db.messages(&sid).unwrap().len();
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: answerer.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "上海有什么好玩的地方".into(),
        }],
    )
    .await
    .expect("压缩后的历史在预算内,本轮不该再压缩");
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn);
    assert_eq!(
        compacted_history.len(),
        rows_before_turn,
        "压缩段交回的历史与库里的一致(两段之间没人动库)"
    );

    let calls = answerer.calls();
    assert_eq!(calls.len(), 1, "压缩后的历史不越线 ⇒ 本轮只有回答一次调用");
    let head = &calls[0];
    let recall = head
        .messages
        .iter()
        .find(|m| m.role == Role::System && m.text_of().starts_with("[长期记忆"))
        .unwrap_or_else(|| {
            panic!(
                "上一轮抽取的记忆没有回到请求头:{:?}",
                head.messages
                    .iter()
                    .map(|m| m.text_of())
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        recall.text_of().contains("用户搬到上海了"),
        "召回内容必须是那条记忆:{}",
        recall.text_of()
    );
    assert!(
        head.messages
            .iter()
            .filter(|m| m.role == Role::System)
            .all(|m| !m.text_of().starts_with("[用户画像]")),
        "画像为空时不许出现空画像行"
    );

    // ---- 不变量 1:记忆行真的计入了这次请求的粗估 ----
    // `estimate_tokens` 是 pub,所以这里能直接重放同一条算式:带记忆的请求
    // 必须严格贵于把同一份 head 里的记忆行摘掉之后。
    let with_memory = crate::agent::context::estimate_tokens(&head.messages);
    let stripped: Vec<Message> = head
        .messages
        .iter()
        .filter(|m| !m.text_of().starts_with("[长期记忆"))
        .cloned()
        .collect();
    let without_memory = crate::agent::context::estimate_tokens(&stripped);
    assert!(
        with_memory > without_memory,
        "记忆行没进预算:{with_memory} vs {without_memory}"
    );

    // ---- 不变量 6:注入是易失的,库里只多本轮两行 ----
    let after = f.db.messages(&sid).unwrap();
    assert_eq!(
        after.len(),
        rows_before_turn + 2,
        "只多 user + assistant 两行"
    );
    assert!(
        !after.iter().any(|m| m.role == Role::System
            && (m.text_of().starts_with("[长期记忆") || m.text_of().starts_with("[用户画像]"))),
        "记忆备注不许落库"
    );
    assert_eq!(f.db.valid_memories().unwrap().len(), 1, "记忆本体没动");
}

/// 记忆比会话活得久:schema 层刻意没有外键,删会话不许连带删记忆。
#[tokio::test]
async fn memories_outlive_the_session_that_produced_them() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海了"))],
        &[],
    );
    compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        None,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        f.db.valid_memories().unwrap().len(),
        1,
        "夹具前提:先有一条记忆"
    );

    f.db.delete_session(&sid).unwrap();

    assert!(
        f.db.messages(&sid).unwrap().is_empty(),
        "夹具前提:消息确实被 CASCADE 删掉了"
    );
    let survivors = f.db.valid_memories().unwrap();
    assert_eq!(survivors.len(), 1, "删会话不许连带删记忆");
    assert_eq!(survivors[0].body, "用户搬到上海了");
    assert_eq!(
        survivors[0].session_id.as_deref(),
        Some(sid.as_str()),
        "来源会话 id 留作审计痕迹 —— 它现在指向一个已不存在的会话,这是有意的"
    );
}

/// R69 的推广:记忆对模型可见、对用户不可见。整轮(含压缩与注入)的全部可见帧
/// 序列化后不许出现记忆备注的文本。
#[tokio::test]
async fn a_memory_note_never_reaches_the_event_stream() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("上海".into()),
        body: "用户住在上海".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    seed(&f.db, &sid, &long_history());

    // 抽取支也配一条真候选:这一轮因此同时覆盖「压缩时新抽出并落库的记忆」与
    // 「预置的画像/记忆」两条来源,证明它们**都**不会出现在用户可见帧里。
    // 候选**必须换一个 key**(predicate 不用「住在」):同键会触发查重合并,
    // 而本夹具没配合并支的模型答复 —— 那条路归 T4 的专测,这里要的只是「真的落了一条」。
    let model = FakeModel::with_extraction(
        FakeModel::summarizing_and_replying(&["早期要点:搬到了上海"], &["好的"]),
        &[
            r#"[{"kind":"fact","subject":"用户","predicate":"常去","object":"杭州","body":"用户也常去杭州","confidence":0.95}]"#,
        ],
    );
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: compacting_cfg(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "上海有什么好玩的".into(),
        }],
    )
    .await
    .expect("压缩 + 一轮发送必须整体成功");
    sink.flush();

    let wire = serde_json::to_string(&rec.frames()).unwrap();
    assert!(
        !wire.contains("[长期记忆"),
        "记忆备注进了用户可见的帧流:{wire}"
    );
    assert!(
        !wire.contains("[用户画像]"),
        "画像进了用户可见的帧流:{wire}"
    );
    assert!(
        !wire.contains("用户住在上海") && !wire.contains("用户是 Rust 开发者"),
        "记忆正文进了用户可见的帧流:{wire}"
    );
    assert!(
        !wire.contains("用户也常去杭州"),
        "压缩时新抽出的记忆也进了用户可见的帧流:{wire}"
    );
    // 夹具守卫:上面第四条不是空断言 —— 抽取真的落了一条新记忆。
    assert!(
        f.db.valid_memories()
            .unwrap()
            .iter()
            .any(|m| m.body == "用户也常去杭州"),
        "夹具前提:抽取出的新记忆确实落库了"
    );
    // 夹具守卫:这一轮确实注入了(否则上面三条只是在复述「没注入」)。
    let injected = model.calls().iter().any(|c| {
        c.messages
            .iter()
            .any(|m| m.text_of().starts_with("[长期记忆"))
    });
    assert!(injected, "夹具前提:记忆确实进过某次请求头");
}

// ==== T5：记忆的总开关与设置流转（压缩段 + run_turn 两级） ====

/// 总开关关掉 = 抽取整段跳过：压缩路径上的模型调用恰好 1（只有摘要），库零记忆。
/// 这一刀砍在 `plan_memory_writes` **之前**，不是「抽了不写」—— 那是用户可见的
/// 行为差（一次端点调用 vs 零次），也是 spec §10「设置页可见调用计数」能对上账的前提。
#[tokio::test]
async fn memory_disabled_skips_extraction_entirely() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    // 抽取那一支配的是真候选：开关若只是「抽了丢弃」，这里会看到两次调用与一条记忆。
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let (history, event, _) = compact_before_turn(
        &f.db,
        &sid,
        &*model,
        &ContextBuilder::new(compacting_cfg()),
        before.clone(),
        &MemoryTurnCfg {
            enabled: false,
            ..Default::default()
        },
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("disabled 只关记忆，压缩本体照常");
    assert!(event.is_some(), "压缩本体不许被记忆开关拦下");
    assert_eq!(
        model.calls().len(),
        1,
        "只许有摘要一次：抽取/合并/画像一个调用都不许发起"
    );
    assert!(f.db.valid_memories().unwrap().is_empty(), "零记忆写入");
    assert!(f.db.profile().unwrap().is_empty(), "画像也不许动");
    assert_eq!(f.db.messages(&sid).unwrap().len(), 21, "压缩本体一个字不少");
    assert_eq!(history, f.db.messages(&sid).unwrap());
}

/// run_turn 级：开关关掉时请求头不带任何记忆备注（画像/召回都不许出现）。
#[tokio::test]
async fn memory_disabled_request_head_carries_no_memory_notes() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("北京".into()),
        body: "用户住在北京".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    seed(
        &f.db,
        &sid,
        &[
            Message::user(vec![ContentBlock::Text {
                text: "上一轮".into(),
            }]),
            Message {
                role: Role::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: "上一答".into(),
                }],
            },
        ],
    );
    let model = FakeModel::build(&[], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg {
                enabled: false,
                ..Default::default()
            },
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "北京有什么好玩的".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn);
    let calls = model.calls();
    assert_eq!(calls.len(), 1, "短历史不压缩，唯一的调用就是回答");
    for m in &calls[0].messages {
        let t = m.text_of();
        assert!(
            !t.starts_with("[用户画像]") && !t.starts_with("[长期记忆"),
            "总开关关掉时请求头不许带任何记忆备注，实得:{t}"
        );
    }
    // 数据还在：关闭是停用，不是清除 —— 开回来记忆必须原样召回（清空的入口在设置页）。
    assert_eq!(f.db.valid_memories().unwrap().len(), 1);
    assert_eq!(f.db.profile().unwrap(), "用户是 Rust 开发者");
}

/// `memory_settings_flow_from_turn_deps` 的共用跑法：种好库、按给定 cfg 跑一轮，
/// 取模型看到的请求（R57：装配结果只能从模型那一侧取证）。
/// T6:嵌入客户端也是一轮的输入,None = off/构造失败(与 T5 行为逐字一致)。
async fn run_one_memory_turn(
    f: &Fixture,
    memory: MemoryTurnCfg,
    embed: Option<Arc<dyn EmbedProvider>>,
) -> Vec<ChatRequest> {
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let model = FakeModel::build(&[], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory,
            embed,
            extract_locks: ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "北京有什么好玩的".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    model.calls()
}

/// 三条都命中「北京」的记忆 + 一份画像，两个 run 共用。
fn seed_beijing_memories(f: &Fixture) {
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    for (object, body) in [
        ("北京", "用户住在北京"),
        ("出差", "用户去北京出差"),
        ("秋天", "用户喜欢北京的秋天"),
    ] {
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在北京".into()),
            object: Some(object.into()),
            body: body.into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    }
}

/// 设置真的流进了装配：recall_k=2 时三条命中最多进两条（默认 5 会是三条）；
/// note cap 压到极小时召回整条让路、画像照常 —— 两条装配口径都吃 `TurnDeps.memory`。
#[tokio::test]
async fn memory_settings_flow_from_turn_deps() {
    let f = Fixture::new();
    seed_beijing_memories(&f);
    let calls = run_one_memory_turn(
        &f,
        MemoryTurnCfg {
            recall_k: 2,
            ..Default::default()
        },
        None,
    )
    .await;
    let recall = calls[0]
        .messages
        .iter()
        .find(|m| m.text_of().starts_with("[长期记忆"))
        .expect("k=2 时召回必须在请求头里");
    let lines = recall
        .text_of()
        .lines()
        .filter(|l| l.starts_with("- "))
        .count();
    assert_eq!(
        lines,
        2,
        "recall_k=2 必须生效（默认 5 会是三条），实得:{}",
        recall.text_of()
    );

    let f = Fixture::new();
    seed_beijing_memories(&f);
    let calls = run_one_memory_turn(
        &f,
        MemoryTurnCfg {
            note_token_cap: 1,
            ..Default::default()
        },
        None,
    )
    .await;
    assert!(
        !calls[0]
            .messages
            .iter()
            .any(|m| m.text_of().starts_with("[长期记忆")),
        "cap=1 连表头都装不下 ⇒ 召回必须整条让路"
    );
    assert!(
        calls[0]
            .messages
            .iter()
            .any(|m| m.text_of().starts_with("[用户画像]")),
        "画像永不因超限丢失"
    );
}

/// 两份默认值必须逐字相同：`MemoryTurnCfg::default` 是测试夹具的锚，
/// `Settings::default` 是生产装配（`send_message`）的锚 —— 分叉了，
/// 「测试看到的记忆行为」和「用户拿到的」就不是一回事。
#[test]
fn memory_turn_cfg_defaults_match_the_settings_defaults() {
    let s = Settings::default();
    let c = MemoryTurnCfg::default();
    assert_eq!(c.enabled, s.memory_enabled);
    assert_eq!(c.recall_k, s.memory_recall_k as usize);
    assert_eq!(c.note_token_cap, s.memory_note_token_cap as usize);
    assert_eq!(c.profile_char_cap, s.memory_profile_char_cap as usize);
    // T10:待办窗口没有对应设置项(裁定:先固定 24 小时),所以它只与常量对齐 ——
    // 这一条断言的意义是「默认值不许在别处被悄悄改成别的数字」。
    assert_eq!(c.prospective_window_hours, MEMORY_PROSPECTIVE_WINDOW_HOURS);
    assert_eq!(c.prospective_window_hours, 24);
    // T12:两个节奏旋钮也必须在默认值上对齐 —— 这一对是「同一条线」在默认值层面的证明:
    // `Send_message` 与 `MemoryTurnCfg::default` 任何一侧偷偷写死别的数,这里当场红。
    assert_eq!(
        c.prospective_window_hours,
        s.memory_prospective_window_hours
    );
    assert_eq!(c.fact_half_life_days, s.memory_decay_half_life_days as f64);
    // T6 两键:布尔是 source=="endpoint" 的现算形态;模型名逐字透传。
    assert_eq!(
        c.embedding_endpoint,
        s.memory_embedding_source == "endpoint"
    );
    assert_eq!(c.embedding_model, s.memory_embedding_model);
    // source 归一后只有两种取值,布尔与字符串的换算因此是精确的(没有第三态可漏)。
    assert_eq!(s.memory_embedding_source, "endpoint");
}

/// G22:`agent_profiles` 是 `Settings.agents → Vec<AgentProfile>` 的唯一转换点。
/// name/prompt/tools 逐字映射;`description` 是给人看的,映射时**丢弃** ——
/// 「少搬一个字段」只能这样钉住:给 description 塞哨兵值,断言目标结构里没有它。
#[test]
fn agent_profiles_maps_name_prompt_tools_and_drops_the_description() {
    use crate::settings::AgentProfileJson;
    let s = Settings {
        agents: vec![AgentProfileJson {
            name: "explore".into(),
            description: "只读调研员的说明(给人看)".into(),
            prompt: Some("你是只读调研员".into()),
            tools: Some(vec!["read_file".into(), "grep".into(), "glob".into()]),
        }],
        ..Default::default()
    };
    let got = agent_profiles(&s);
    assert_eq!(got.len(), 1, "档案逐条映射:{got:?}");
    // 与「不含 description 的字面值」精确相等:name/prompt/tools 逐字透传。
    // `AgentProfile` 本身就没有 description 字段(编译期事实),将来谁往结构里加
    // 字段、往映射里塞多余的值,这条等式当场红。
    assert_eq!(
        got[0],
        AgentProfile {
            name: "explore".into(),
            prompt: Some("你是只读调研员".into()),
            tools: Some(vec!["read_file".into(), "grep".into(), "glob".into()]),
        },
        "name/prompt/tools 逐字透传,description 不在目标结构里"
    );
    // 空档案表 = 空表:未配置时 task 的行为与从前一致。
    assert!(agent_profiles(&Settings::default()).is_empty());
}

/// T12:装配函数是 `Settings → MemoryTurnCfg` 的唯一一处转换,所以两个新设置项
/// **改了就一定到达装配**这件事只能在这里钉。`send_message` 本身是
/// `#[tauri::command]`、单测调不动(本模块测试头的 R103 段);把它拆成这个纯函数,
/// 就是为了让「设置里改了 7 天半衰期 / 72 小时窗口,这一轮就按 7 天 / 72 小时跑」
/// 有一个不依赖 tauri 的判据。
#[test]
fn the_turn_cfg_takes_both_pacing_settings_from_the_settings_table() {
    let s = Settings {
        memory_decay_half_life_days: 7,
        memory_prospective_window_hours: 72,
        ..Default::default()
    };
    let c = memory_turn_cfg(&s);
    assert_eq!(c.fact_half_life_days, 7.0, "半衰期必须原样转成 f64");
    assert_eq!(
        c.prospective_window_hours, 72,
        "待办窗口必须读设置,不许写死 24"
    );
    // 其余键不许被这次转换顺手改掉(一个装配函数改两项,别的项必须原样)。
    assert_eq!(c.recall_k, s.memory_recall_k as usize);
    assert_eq!(c.note_token_cap, s.memory_note_token_cap as usize);
    assert_eq!(c.profile_char_cap, s.memory_profile_char_cap as usize);
    assert_eq!(c.enabled, s.memory_enabled);
}

/// T12:半衰期真的从参数流进打分。夹具是 store 侧 `decay_can_flip_the_order_…`
/// 的同款:一条**更相关但更旧**的记忆(含查询全部 gram)对一条较新、较不相关的。
/// 半衰期长到 10 年时旧行仍然霸榜;缩到 60 天(默认锚点)时旧行被压到新的后面。
///
/// 两次测量必须用**两个独立的库**:`search_memories` 的「访问即再巩固」会把
/// `last_accessed` 刷成 now,同一个库上跑第二次时旧行已经变新鲜了,测的就是
/// 再巩固而不是半衰期 —— 那正是这条测试最容易被写假的地方。
#[test]
fn a_shorter_half_life_sinks_an_old_but_more_relevant_memory_below_a_fresh_one() {
    const DAY_MS: i64 = 86_400_000;
    // 每个夹具种同样的两行:旧的含查询全部 gram(相关性 1.0),新的只含 5/8(0.625)。
    let seed = |f: &Fixture| {
        // 新的那条:只含查询的一部分 gram(相关性 0.625),但 age = 0。
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some("北京".into()),
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
        let old =
            f.db.add_memory(&NewMemory {
                session_id: None,
                kind: "fact".into(),
                subject: Some("用户".into()),
                predicate: Some("住在".into()),
                object: Some("北京朝阳区".into()),
                body: "用户住在北京朝阳区".into(),
                confidence: 0.9,
                due_at: None,
            })
            .unwrap();
        // 旧的那条:含查询全部 gram(相关性 1.0),但 900 天没被碰过。
        {
            let c = f.db.lock();
            let ancient = Db::now_ms() - 900 * DAY_MS;
            c.execute(
                "UPDATE memories SET valid_from=?2, updated_at=?2 WHERE id=?1",
                params![old, ancient],
            )
            .unwrap();
        }
    };

    let long_lived = Fixture::new();
    seed(&long_lived);
    let note = memory_notes_for_turn(
        &long_lived.db,
        "用户住在北京朝阳区",
        None,
        1,
        600,
        24,
        3650.0,
    )
    .unwrap();
    let recall = note
        .iter()
        .find(|n| n.starts_with("[长期记忆"))
        .expect("召回备注必须在");
    assert!(
        recall.contains("朝阳区"),
        "半衰期 10 年 ⇒ 900 天只掉了约 15%,更相关的旧行仍该排第一:{recall}"
    );

    let short_lived = Fixture::new();
    seed(&short_lived);
    let note = memory_notes_for_turn(
        &short_lived.db,
        "用户住在北京朝阳区",
        None,
        1,
        600,
        24,
        60.0,
    )
    .unwrap();
    let recall = note
        .iter()
        .find(|n| n.starts_with("[长期记忆"))
        .expect("召回备注必须在");
    assert!(
        !recall.contains("朝阳区"),
        "半衰期 60 天 ⇒ 900 天掉到下限,更相关的旧行必须沉到新的后面:{recall}"
    );
    assert!(recall.contains("用户住在北京"), "新的那条必须在:{recall}");
}

// ==== T6:嵌入端点客户端 + 混合召回(读/写两条路径) ====

use rusqlite::params;

/// 关键词假嵌入器:文本含 `keyword` → 与查询同向的单位向量,否则 → 正交向量。
/// 向量本身由测试设定,这里只做「哪个词」的映射。
struct KeywordEmbedder {
    keyword: &'static str,
    hit: &'static [f32],
    miss: &'static [f32],
}

#[async_trait]
impl EmbedProvider for KeywordEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts
            .iter()
            .map(|t| {
                (if t.contains(self.keyword) {
                    self.hit
                } else {
                    self.miss
                })
                .to_vec()
            })
            .collect())
    }
}

/// 永远失败的假嵌入器:写路径/读路径的降级测试都用它。
struct FailingEmbedder;

#[async_trait]
impl EmbedProvider for FailingEmbedder {
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Err("端点不可达(测试注入)".to_string())
    }
}

/// 混合召回端到端:gram 分 0 的「帝都日常」靠与查询同向的向量(0.6 权重)压过
/// gram 分 0.5 但无向量的「用户住在北京」(只剩 0.4 权重) —— 语义通道与关键词
/// 通道都在同一份排序里真实生效。对照组:embed=None 时「帝都日常」gram 为 0,
/// 根本不该出现在召回里(T5 行为逐字保持)。
#[tokio::test]
async fn a_fake_embedder_reranks_the_recall_head_by_hybrid_score() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("北京".into()),
        body: "用户住在北京".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    let semantic =
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: None,
            predicate: None,
            object: None,
            body: "帝都日常".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    f.db.set_memory_embedding(semantic, &[1.0, 0.0]).unwrap();

    let model = FakeModel::build(&[], &["好的"]);
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.clone()));
    let reason = run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory: MemoryTurnCfg::default(),
            extract_locks: ExtractLocks::default(),
            data_dir: None,
            // 假嵌入器只对**本轮查询**生效(「北京住在哪」含「北京」→ [1,0]);
            // 行向量走库里已写好的 BLOB —— 读路径的向量来源与写路径解耦,
            // 这正是要端到端验证的线。
            embed: Some(Arc::new(KeywordEmbedder {
                keyword: "北京",
                hit: &[1.0, 0.0],
                miss: &[0.0, 1.0],
            })),
            cancel: f.cancel.clone(),
            skills_dir: None,
            skills_disabled: Vec::new(),
        },
        &sid,
        vec![ContentBlock::Text {
            text: "北京住在哪".into(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    assert_eq!(reason, StopReason::EndTurn);

    let recall = model
        .calls()
        .remove(0)
        .messages
        .into_iter()
        .find(|m| m.text_of().starts_with("[长期记忆"))
        .expect("带嵌入的查询必须召回出结果");
    let beijing = recall
        .text_of()
        .find("用户住在北京")
        .expect("gram 命中的行必须在");
    let dudu = recall.text_of().find("帝都日常").expect("向量行必须进召回");
    assert!(
        dudu < beijing,
        "gram=0 的向量行(0.6 分)必须排在 gram 命中的无向量行(0.4 分)之前,实得:{}",
        recall.text_of()
    );

    // 对照组:同一批数据,embed=None → 纯关键词,「帝都日常」gram=0 不入召回。
    let calls = run_one_memory_turn(&f, MemoryTurnCfg::default(), None).await;
    let plain = calls
        .into_iter()
        .next()
        .unwrap()
        .messages
        .into_iter()
        .find(|m| m.text_of().starts_with("[长期记忆"))
        .expect("纯关键词路径照样召回到北京行");
    assert!(
        !plain.text_of().contains("帝都日常"),
        "embed=None 必须与 T5 逐字一致:gram 零重叠的行不许出现,实得:{}",
        plain.text_of()
    );
    assert!(plain.text_of().contains("用户住在北京"));
}

/// 查询嵌入失败(网络断/端点 4xx):本轮退化为纯关键词,**不许阻塞对话** ——
/// 回答照常、召回照常有 gram 命中的行。无跨轮状态 ⇒ 下一次发送自然重试,
/// 没有任何缓存要失效。
#[tokio::test]
async fn a_failing_query_embed_degrades_to_keyword_recall_without_blocking() {
    let f = Fixture::new();
    seed_beijing_memories(&f);
    let calls = run_one_memory_turn(
        &f,
        MemoryTurnCfg::default(),
        Some(Arc::new(FailingEmbedder)),
    )
    .await;
    let recall = calls[0]
        .messages
        .iter()
        .find(|m| m.text_of().starts_with("[长期记忆"))
        .expect("嵌入失败只降级通道,不消灭召回");
    assert!(
        recall.text_of().contains("用户住在北京"),
        "gram 通道必须还在工作:{}",
        recall.text_of()
    );
    assert_eq!(
        calls[0]
            .messages
            .iter()
            .filter(|m| m.text_of().starts_with("[用户画像]"))
            .count(),
        1,
        "画像照常注入:降级的只是向量半边"
    );
}

/// 压缩事务提交后,新增记忆行被补写向量:BLOB 里就是假嵌入器对 body 的输出
/// (直接读 BLOB 取证,不经 search —— 检索行为由 store 层自己的测试管)。
#[tokio::test]
async fn compaction_embeds_the_new_memory_rows_after_the_transaction() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    // body 含「上海」⇒ 假嵌入器对它给 hit 向量 [1,0],事务提交后应原样落进 BLOB。
    let (_, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        Some(Arc::new(KeywordEmbedder {
            keyword: "上海",
            hit: &[1.0, 0.0],
            miss: &[0.0, 1.0],
        })),
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_some(), "压缩本体照常");

    let rows = f.db.valid_memories().unwrap();
    assert_eq!(rows.len(), 1);
    let c = f.db.lock();
    let blob: Vec<u8> = c
        .query_row(
            "SELECT embedding FROM memories WHERE id=?1",
            params![rows[0].id],
            |r| r.get(0),
        )
        .unwrap();
    drop(c);
    assert_eq!(
        crate::store::memory::from_vec(&blob).unwrap(),
        vec![1.0f32, 0.0],
        "嵌入补写必须把向量写进新行的 BLOB"
    );
}

/// 嵌入失败(端点挂了):记忆行**照常落库**,纯关键词照样能召回 ——
/// 失败只降级,不回滚压缩事务、不丢行、不阻塞(裁定 4,spec §8 不变量 5)。
#[tokio::test]
async fn a_failing_memory_embedder_keeps_new_rows_keyword_searchable() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let (_, event, _) = compact_once(
        &f,
        &model,
        &sid,
        before,
        compacting_cfg(),
        &MemoryTurnCfg::default(),
        Some(Arc::new(FailingEmbedder)),
        &CancellationToken::new(),
    )
    .await;
    assert!(event.is_some(), "嵌入失败不许阻塞压缩");
    let rows = f.db.valid_memories().unwrap();
    assert_eq!(rows.len(), 1, "行必须保留");
    // BLOB 仍是 NULL(嵌入没成),但关键词通道完好。
    let c = f.db.lock();
    let blob: Option<Vec<u8>> = c
        .query_row(
            "SELECT embedding FROM memories WHERE id=?1",
            params![rows[0].id],
            |r| r.get(0),
        )
        .unwrap();
    drop(c);
    assert!(blob.is_none(), "失败不许写出半截向量");
    let hits =
        f.db.search_memories("上海", None, 10, &RecallCfg::default())
            .unwrap();
    assert_eq!(hits.len(), 1, "关键词召回照常工作");
}

/// 设置页审计视图的线上形状：字段与 `MemoryRow` 同名同序（snake_case，与
/// `SessionRow` 的线上惯例一致）+ 计算字段 `valid`（= invalid_at 是否为空 ——
/// 判据与当前时间无关，前端不该被邀请去比时间）。
#[test]
fn the_memory_view_keeps_the_row_shape_and_marks_validity() {
    let f = Fixture::new();
    let dead =
        f.db.add_memory(&NewMemory {
            session_id: Some("s1".into()),
            kind: "fact".into(),
            subject: Some("用户".into()),
            predicate: Some("住在".into()),
            object: Some("北京".into()),
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "preference".into(),
        subject: None,
        predicate: None,
        object: None,
        body: "用户喜欢咖啡".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    f.db.invalidate_memory(dead).unwrap();

    let views: Vec<MemoryView> =
        f.db.all_memories()
            .unwrap()
            .into_iter()
            .map(MemoryView::from)
            .collect();
    assert_eq!(views.len(), 2);
    let by_id: std::collections::HashMap<i64, &MemoryView> =
        views.iter().map(|v| (v.id, v)).collect();
    assert!(!by_id[&dead].valid, "invalid_at 非空 ⇒ valid=false");
    assert!(by_id[&dead].invalid_at.is_some());
    assert!(
        views.iter().any(|v| v.id != dead && v.valid),
        "invalid_at 为空 ⇒ valid=true"
    );

    let v = serde_json::to_value(&views[0]).unwrap();
    for key in [
        "id",
        "session_id",
        "kind",
        "subject",
        "predicate",
        "object",
        "body",
        "confidence",
        "valid_from",
        "invalid_at",
        "updated_at",
        "valid",
        // T12：强度与待办状态也必须出现在线上形状里（设置页每行要显示）。
        "reinforcements",
        "due_at",
        "is_due",
    ] {
        assert!(v.get(key).is_some(), "缺键 {key}:{v:?}");
    }
    let wire = serde_json::to_string(&views).unwrap();
    assert!(
        !wire.contains("sessionId"),
        "线上惯例是 snake_case（与 SessionRow 一致），camelCase 键前端读不到:{wire}"
    );
}

/// T12:设置页每行要显示的三项 —— 确认次数、待办日期、「已到期」。
/// `is_due` 在 Rust 侧现算(= `due_at` 非空且 `<= now`),前端不复算第二遍:
/// 两处比较时间的写法会在时区/时钟漂移上分叉,而这里只需要一个布尔。
#[test]
fn the_memory_view_exposes_reinforcement_count_and_the_prospective_due_state() {
    let f = Fixture::new();
    const DAY_MS: i64 = 86_400_000;
    let overdue = Db::now_ms() - DAY_MS;
    let id =
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "prospective".into(),
            subject: Some("用户".into()),
            predicate: Some("要".into()),
            object: Some("交季度报告".into()),
            body: "用户要在明天交季度报告".into(),
            confidence: 0.9,
            due_at: Some(overdue),
        })
        .unwrap();
    f.db.reinforce_memory(id).unwrap();
    f.db.reinforce_memory(id).unwrap();
    let up =
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "prospective".into(),
            subject: Some("用户".into()),
            predicate: Some("要".into()),
            object: Some("体检".into()),
            body: "用户下个月要体检".into(),
            confidence: 0.9,
            due_at: Some(Db::now_ms() + DAY_MS),
        })
        .unwrap();
    let fact =
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: "fact".into(),
            subject: None,
            predicate: None,
            object: None,
            body: "用户住在北京".into(),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();

    let views: std::collections::HashMap<i64, MemoryView> =
        f.db.all_memories()
            .unwrap()
            .into_iter()
            .map(|r| (r.id, MemoryView::from(r)))
            .collect();
    let v = &views[&id];
    assert_eq!(v.reinforcements, 2, "确认次数必须原样出现在视图里");
    assert_eq!(v.due_at, Some(overdue), "到期时刻原样透传");
    assert!(v.is_due, "due_at <= now ⇒ 已到期");
    let v = &views[&up];
    assert_eq!(v.reinforcements, 0);
    assert!(!v.is_due, "未来的待办不算已到期");
    let v = &views[&fact];
    assert_eq!(v.due_at, None, "非待办没有到期时刻");
    assert!(!v.user_edited, "模型抽出来的行不是用户编辑的");

    // P0:用户改过的行必须在视图里看得出来 —— 前端的「已编辑」标记只认这个字段,
    // 它漏传的话用户永远看不到「模型以后不会改写它」的那条承诺被兑现。
    f.db.update_memory_body_by_user(fact, "用户住在上海")
        .unwrap();
    let v = MemoryView::from(
        f.db.all_memories()
            .unwrap()
            .into_iter()
            .find(|r| r.id == fact)
            .unwrap(),
    );
    assert!(v.user_edited, "用户编辑标记必须出现在视图里");
    assert_eq!(v.body, "用户住在上海");
    assert!(!v.is_due, "没有 due_at 的永远不算已到期");

    // 线上形状是 snake_case 的三个新键(前端 types.ts 的镜像靠这一条兜住)。
    let wire = serde_json::to_value(&views[&id]).unwrap();
    assert_eq!(wire["reinforcements"], 2);
    assert_eq!(wire["is_due"], true);
    assert_eq!(wire["due_at"], overdue);
}

// ==== T10:注入侧的三个类人机制(编码特异性 / 前瞻记忆 / 元记忆)====

/// 机制一的纯函数:上一轮用户消息拼在本轮消息之前,换行分隔。
#[test]
fn recall_query_prefixes_the_current_message_with_the_previous_user_turn() {
    let history = vec![
        Message::user(vec![ContentBlock::Text {
            text: "更早的一句".into(),
        }]),
        Message {
            role: Role::Assistant,
            blocks: vec![ContentBlock::Text {
                text: "上一答".into(),
            }],
        },
        Message::user(vec![ContentBlock::Text {
            text: "上一轮说的话".into(),
        }]),
    ];
    assert_eq!(
        recall_query(&history, "那它呢"),
        "上一轮说的话\n那它呢",
        "最后一条用户消息才是线索,助手消息不许顶替"
    );
}

/// 历史里没有用户消息(空历史 / 全是助手与备注)→ 只返回当前消息。
#[test]
fn recall_query_returns_only_the_current_message_when_no_prior_user_turn_exists() {
    assert_eq!(recall_query(&[], "那它呢"), "那它呢", "空历史");
    let history = vec![
        Message::system_note("早期对话摘要：……"),
        Message {
            role: Role::Assistant,
            blocks: vec![ContentBlock::Text {
                text: "上一答".into(),
            }],
        },
    ];
    assert_eq!(recall_query(&history, "那它呢"), "那它呢");
}

#[test]
fn recall_query_skips_the_synthetic_tool_image_followup() {
    let history = vec![
        Message::user(vec![ContentBlock::Text {
            text: "这张图里是什么".into(),
        }]),
        Message::user(vec![
            ContentBlock::Text {
                text: crate::agent::types::TOOL_IMAGE_FOLLOWUP_NOTE.into(),
            },
            ContentBlock::Image {
                data: "QUJD".into(),
                mime: crate::agent::types::ImageMime::Png,
            },
        ]),
    ];
    assert_eq!(
        recall_query(&history, "那它呢"),
        "这张图里是什么\n那它呢",
        "内部图片说明不能冒充上一句用户话"
    );
}

/// 工具结果消息的 role 也是 User,但它不是用户说的话 —— 跳过它,继续往前找。
/// 只剩工具结果时退化为「没有上一轮」。
#[test]
fn recall_query_skips_tool_result_messages_when_looking_for_the_prior_turn() {
    let tool_result = || {
        Message::tool(
            "c1",
            ToolOutput::Text {
                text: "工具返回的一堆数据".into(),
            },
            false,
        )
    };
    let history = vec![
        Message::user(vec![ContentBlock::Text {
            text: "真正的上一句".into(),
        }]),
        tool_result(),
    ];
    assert_eq!(
        recall_query(&history, "那它呢"),
        "真正的上一句\n那它呢",
        "工具结果不许被当成用户的话"
    );

    let only_tool = vec![tool_result()];
    assert_eq!(recall_query(&only_tool, "那它呢"), "那它呢");
}

/// 上一轮文本超过 200 字符 → 截前 200 字符,别把窗口外的旧话题拖进检索。
#[test]
fn recall_query_truncates_the_prior_turn_to_two_hundred_characters() {
    let long: String = std::iter::repeat_n('字', 250).collect();
    let history = vec![Message::user(vec![ContentBlock::Text { text: long }])];
    let got = recall_query(&history, "那它呢");
    let (prior, current) = got.split_once('\n').expect("换行分隔");
    assert_eq!(
        prior.chars().count(),
        RECALL_QUERY_PRIOR_CHARS,
        "截到 200 字符"
    );
    assert_eq!(current, "那它呢");
    assert_eq!(RECALL_QUERY_PRIOR_CHARS, 200);
}

/// 上一轮用户消息的文本为空(纯图片轮)→ 只返回当前消息,不拼一条空线索。
#[test]
fn recall_query_returns_only_the_current_message_when_the_prior_turn_has_no_text() {
    let history = vec![Message::user(vec![ContentBlock::Image {
        data: "iVBORw0KGgo=".into(),
        mime: crate::agent::types::ImageMime::Png,
    }])];
    assert_eq!(recall_query(&history, "这是什么"), "这是什么");
}

/// 共用跑法:按给定 cfg 跑一轮,交回模型看到的全部请求(R57:装配结果只能从
/// 模型那一侧取证)。与本文件既有的 `run_one_memory_turn` 分开,是因为 T10 的
/// 多轮测试要自己控制 session 与历史。
async fn run_memory_turn(
    f: &Fixture,
    sid: &str,
    model: &Arc<FakeModel>,
    memory: MemoryTurnCfg,
    text: &str,
) -> Vec<ChatRequest> {
    let rec = Arc::new(Recorded::default());
    let sink = Arc::new(TauriEmitter::new(rec.clone(), sid.to_string()));
    run_turn(
        TurnDeps {
            output_dir: None,
            db: f.db.clone(),
            registry: f.registry.clone(),
            gate: f.gate.clone(),
            model: model.clone(),
            sink: sink.clone(),
            context: ContextConfig::default(),
            temperature: None,
            plan_mode: false,
            diagnostics_command: None,
            allowed_tools: None,
            active_agent_name: None,
            agents: Vec::new(),
            roots: Vec::new(),
            memory,
            embed: None,
            extract_locks: crate::ipc::ExtractLocks::default(),
            data_dir: None,
            skills_dir: None,
            skills_disabled: Vec::new(),
            cancel: f.cancel.clone(),
        },
        sid,
        vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    )
    .await
    .unwrap();
    sink.flush();
    // G37：起名是落库后的独立任务，它的 TITLE 请求同样进 FakeModel 的录制序列，
    // 且时序上可能插在任意两轮之间。这组测试量的是「主对话与记忆管线」的调用，
    // 起名调用一律滤掉 —— 不过滤的话 calls 的长度与下标都会随调度漂移。
    model
        .calls()
        .into_iter()
        .filter(|req| {
            req.messages
                .first()
                .is_none_or(|m| m.text_of() != crate::agent::summarize::TITLE_INSTRUCTION)
        })
        .collect()
}

/// 请求头里以某前缀开头的那条备注(没有就是 None)。
fn head_note(req: &ChatRequest, prefix: &str) -> Option<String> {
    req.messages
        .iter()
        .find(|m| m.text_of().starts_with(prefix))
        .map(|m| m.text_of())
}

/// 批量种待办:第 i 条的到期时刻是 `now + i*hours`,body 由调用方给。
fn seed_prospective(f: &Fixture, body: &str, due_in_hours: i64) -> i64 {
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "prospective".into(),
        subject: Some("用户".into()),
        predicate: Some("要做".into()),
        object: Some(body.into()),
        body: body.into(),
        confidence: 0.95,
        due_at: Some(Db::now_ms() + due_in_hours * 60 * 60 * 1_000),
    })
    .unwrap()
}

/// 提醒里的到期日按 UTC 的 `YYYY-MM-DD` 格式化(与 T9 的解析同一口径:
/// 无时区字面值按 UTC 解释)。`1_793_523_600_000` = 2026-11-01T09:00:00Z。
#[test]
fn due_date_labels_use_the_same_utc_calendar_as_t9_parsing() {
    assert_eq!(due_date_label(1_793_523_600_000), "2026-11-01");
    assert_eq!(due_date_label(0), "1970-01-01", "月/日必须补零");
}

/// 机制二的端到端:第一轮追问「那它呢」本身没有实体,靠编码特异性把上一轮的
/// 「我在上海工作」拼进 query,才召回到含「上海」的记忆。
/// 对照:两条文本的 2-gram 零重叠(「那它呢」⇔「用户在上海工作」),所以这条
/// 召回只可能来自上一轮上下文的拼接。
#[tokio::test]
async fn a_follow_up_question_recalls_memories_through_the_previous_user_turn() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("工作于".into()),
        object: Some("上海".into()),
        body: "用户在上海工作".into(),
        confidence: 0.95,
        due_at: None,
    })
    .unwrap();
    let model = FakeModel::build(&[], &["好的", "好的"]);

    // 第一轮:把「我在上海工作」写进历史(压缩后历史里它就是上一轮)。
    run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "我在上海工作").await;
    // 第二轮:只有指代词。
    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "那它呢").await;
    assert_eq!(calls.len(), 2, "短历史 ⇒ 每轮只有回答一次调用");

    let recall = head_note(&calls[1], "[长期记忆")
        .unwrap_or_else(|| panic!("第二轮必须召回:上一轮上下文被拼进了 query"));
    assert!(
        recall.contains("用户在上海工作"),
        "含「上海」的记忆必须靠上一轮上下文被召回,实得:{recall}"
    );
}

/// 机制二:到期窗口内的待办在本轮 head 里出现一次,并立刻标记 `notified_at`;
/// 第二轮不再提醒。提醒文本的形状也在这里钉住(前缀 + 正文 + 到期日)。
#[tokio::test]
async fn a_due_prospective_is_reminded_once_and_then_marked_notified() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed_prospective(&f, "交季度报告", 3);
    let model = FakeModel::build(&[], &["好的", "好的"]);

    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "你好").await;
    let due = head_note(&calls[0], "[待办提醒]").expect("到期窗口内的待办必须提醒");
    assert!(due.contains("交季度报告"), "提醒里必须有待办正文:{due}");
    assert!(
        due.contains('(') && due.contains(')'),
        "到期日必须附在正文后面:{due}"
    );
    assert!(
        f.db.all_memories().unwrap()[0].notified_at.is_some(),
        "注入之后必须立刻标记 notified_at"
    );

    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "又一句").await;
    assert!(
        head_note(&calls[1], "[待办提醒]").is_none(),
        "提醒过的不许每轮重复:notified_at 是唯一闸门"
    );
}

/// 机制二的上限:一次最多提醒 5 条;第 6 条这一轮**不提醒也不标记**,
/// 留给下一轮 —— 一次刷十条会淹没真正的紧急项。
#[tokio::test]
async fn at_most_five_due_prospectives_are_reminded_and_the_rest_wait_for_the_next_turn() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let bodies = ["待办一", "待办二", "待办三", "待办四", "待办五", "待办六"];
    for (i, body) in bodies.iter().enumerate() {
        seed_prospective(&f, body, i as i64 + 1);
    }
    let model = FakeModel::build(&[], &["好的", "好的"]);

    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "开始").await;
    let due = head_note(&calls[0], "[待办提醒]").expect("有 6 条待办,前 5 条必须提醒");
    for body in &bodies[..5] {
        assert!(due.contains(body), "前 5 条都该在:{body} / {due}");
    }
    assert!(
        !due.contains("待办六"),
        "第 6 条这一轮不许提醒(否则 5 条上限没有意义):{due}"
    );
    let notified: Vec<bool> =
        f.db.all_memories()
            .unwrap()
            .iter()
            .map(|m| m.notified_at.is_some())
            .collect();
    assert_eq!(
        notified,
        vec![true, true, true, true, true, false],
        "只标记真正注入过的那 5 条"
    );

    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "继续").await;
    let due = head_note(&calls[1], "[待办提醒]").expect("上一轮留下的第 6 条必须在下一轮提醒");
    assert!(due.contains("待办六"), "轮到第 6 条了:{due}");
    for body in &bodies[..5] {
        assert!(!due.contains(body), "前 5 条已经提醒过,不许再来一次:{due}");
    }
}

/// 机制二的总开关:disabled 时既不提醒、也**不标记** —— 待办还在库里,
/// 用户把记忆开回来时它仍然该被提醒(「停用」不许偷偷消费掉提醒)。
#[tokio::test]
async fn a_disabled_memory_switch_neither_reminds_nor_marks_due_prospectives() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed_prospective(&f, "交季度报告", 3);
    let model = FakeModel::build(&[], &["好的"]);

    let calls = run_memory_turn(
        &f,
        &sid,
        &model,
        MemoryTurnCfg {
            enabled: false,
            ..Default::default()
        },
        "你好",
    )
    .await;
    assert!(
        head_note(&calls[0], "[待办提醒]").is_none(),
        "总开关关掉:请求头零记忆备注"
    );
    assert!(
        f.db.all_memories().unwrap()[0].notified_at.is_none(),
        "disabled 不许标记:开回来时这条待办还得提醒"
    );
}

/// 机制三的纯函数:按 kind 计数,中文标签;未知 kind 原样显示;空库 None。
#[test]
fn overview_note_counts_valid_memories_by_kind_with_chinese_labels() {
    let f = Fixture::new();
    assert_eq!(overview_note(&f.db).unwrap(), None, "没有记忆就不注入概览");
    for kind in ["fact", "fact", "fact", "preference", "preference"] {
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: kind.into(),
            subject: None,
            predicate: None,
            object: None,
            body: format!("一条 {kind} 记忆"),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    }
    // 未知 kind 原样显示(不猜一个中文名把模型的分类洗掉)。
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "entity_rel".into(),
        subject: None,
        predicate: None,
        object: None,
        body: "一条关系".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();

    // SQL 的 ORDER BY kind:entity_rel < fact < preference。
    assert_eq!(
        overview_note(&f.db).unwrap().as_deref(),
        Some(
            "[记忆库]你有 6 条长期记忆(entity_rel 1、事实 3、偏好 2),可用 memory_search 工具按需检索。"
        )
    );
}

/// 机制三的端到端:有记忆时概览进请求头且条数正确;空库不注入。
#[tokio::test]
async fn a_memory_overview_is_injected_only_when_the_library_is_non_empty() {
    // 空库:没有任何 [记忆库] 备注。
    let empty = Fixture::new();
    let esid = empty.db.create_session(DEFAULT_TITLE).unwrap();
    let emodel = FakeModel::build(&[], &["好的"]);
    let calls = run_memory_turn(&empty, &esid, &emodel, MemoryTurnCfg::default(), "今天天气").await;
    assert!(head_note(&calls[0], "[记忆库]").is_none(), "空库不注入概览");

    // 有记忆:3 条事实 + 2 条偏好,概览条数必须对得上。
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    for kind in ["fact", "fact", "fact", "preference", "preference"] {
        f.db.add_memory(&NewMemory {
            session_id: None,
            kind: kind.into(),
            subject: None,
            predicate: None,
            object: None,
            body: format!("一条 {kind} 记忆"),
            confidence: 0.9,
            due_at: None,
        })
        .unwrap();
    }
    let model = FakeModel::build(&[], &["好的"]);
    let calls = run_memory_turn(&f, &sid, &model, MemoryTurnCfg::default(), "今天天气").await;
    let overview = head_note(&calls[0], "[记忆库]").expect("有记忆就必须有概览");
    assert!(
        overview.contains("你有 5 条长期记忆(事实 3、偏好 2)"),
        "概览条数与分布必须正确:{overview}"
    );
    assert!(
        overview.contains("memory_search"),
        "概览要告诉模型去查:{overview}"
    );
}

/// 机制顺序(硬性):待办 → 画像 → 召回 → 概览 → 历史摘要。一条测试同时钉住
/// 五个相对位置 —— 任何两段调换都该在这里红。
#[tokio::test]
async fn the_head_orders_due_then_profile_then_recall_then_overview_then_summary() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed_prospective(&f, "交季度报告", 3);
    f.db.set_profile("用户是 Rust 开发者").unwrap();
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("北京".into()),
        body: "用户住在北京".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    // 历史里放一条摘要备注:head 里它排在全部记忆备注之后。
    seed(
        &f.db,
        &sid,
        &[
            Message::system_note("早期对话摘要：之前聊过 Debian"),
            Message::user(vec![ContentBlock::Text {
                text: "上一轮".into(),
            }]),
            Message {
                role: Role::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: "上一答".into(),
                }],
            },
        ],
    );
    let model = FakeModel::build(&[], &["好的"]);
    let calls = run_memory_turn(
        &f,
        &sid,
        &model,
        MemoryTurnCfg::default(),
        "北京有什么好玩的",
    )
    .await;

    let msgs = &calls[0].messages;
    let index_of = |prefix: &str| {
        msgs.iter()
            .position(|m| m.text_of().starts_with(prefix))
            .unwrap_or_else(|| panic!("请求头缺少 {prefix} 备注"))
    };
    let due = index_of("[待办提醒]");
    let profile = index_of("[用户画像]");
    let recall = index_of("[长期记忆");
    let overview = index_of("[记忆库]");
    let summary = index_of("早期对话摘要：");
    assert!(
        due < profile,
        "待办必须最先被看到:due={due} profile={profile}"
    );
    assert!(profile < recall, "画像是常驻背景,排在召回之前");
    assert!(recall < overview, "概览是最后一条记忆备注");
    assert!(
        overview < summary,
        "概览必须紧挨历史摘要之前:overview={overview} summary={summary}"
    );
    assert_eq!(
        0,
        msgs.iter().position(|m| m.role == Role::System).unwrap(),
        "系统提示词仍在最前"
    );
}

/// 机制二不受 note token 预算约束:cap 压到极小,召回整条让路、概览被砍,
/// 待办提醒仍然在 —— 它是「必须说的事」,与可选的召回不是一类。
#[tokio::test]
async fn due_notes_survive_a_tiny_note_token_cap_while_recall_yields() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed_prospective(&f, "交季度报告", 3);
    f.db.add_memory(&NewMemory {
        session_id: None,
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some("住在".into()),
        object: Some("北京".into()),
        body: "用户住在北京".into(),
        confidence: 0.9,
        due_at: None,
    })
    .unwrap();
    let model = FakeModel::build(&[], &["好的"]);
    let calls = run_memory_turn(
        &f,
        &sid,
        &model,
        MemoryTurnCfg {
            note_token_cap: 1,
            ..Default::default()
        },
        "北京有什么好玩的",
    )
    .await;

    let due = head_note(&calls[0], "[待办提醒]").expect("待办不受 cap 约束");
    assert!(due.contains("交季度报告"), "待办正文必须在:{due}");
    assert!(
        head_note(&calls[0], "[长期记忆").is_none(),
        "cap=1 连召回表头都装不下 ⇒ 召回必须让路"
    );
    assert!(
        head_note(&calls[0], "[记忆库]").is_none(),
        "概览计入 cap,这里也该被砍掉"
    );
}

#[test]
fn workspace_commands_are_registered_in_the_handler_list() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let list = src
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|rest| rest.split("]").next())
        .expect("找不到 generate_handler 的登记表");
    let registered: Vec<&str> = list.split(',').map(str::trim).collect();
    for cmd in [
        "reveal_path",
        "pick_folder",
        "session_output_view",
        "session_workspace_set",
    ] {
        assert!(
            registered.contains(&cmd),
            "{cmd} 没登记，前端只会拿到 not found"
        );
    }
}

/// T5 的四条记忆命令与 `normalize_image` 同一个坑（`generate_handler!` 是宏）：
/// 漏登记不编译报错，只会在运行时让前端拿到 not found。机制照抄上面那条守卫。
#[test]
fn the_memory_commands_are_registered_in_the_handler_list() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let list = src
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|rest| rest.split("]").next())
        .expect("找不到 generate_handler 的登记表");
    let registered: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    for cmd in [
        "memories_list",
        "memories_delete",
        "memories_clear",
        "memory_profile",
        // T15/T16(P0):空闲/切换抽取与用户的编辑权。这三条同样是宏登记,
        // 漏了只会在运行时变成前端的 not found。
        "memory_flush_session",
        "memory_update_body",
        "memory_add",
    ] {
        assert!(
            registered.contains(&cmd),
            "命令 {cmd} 写了但没登记，前端调用只会拿到 not found:{registered:?}"
        );
    }
}

/// 单轮并发闸门：任何一轮都在跑的时候都不许起第二轮 —— 同会话也不行，
/// 因为第二轮的 `prior` 会在第一轮的写落地之前读库（R103 只有一个所有者才成立）。
#[test]
fn one_running_turn_blocks_every_other_claim() {
    let slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let _first = claim_running(&slot, "A").expect("第一轮应当放行");
    assert_eq!(slot.lock().unwrap().as_deref(), Some("A"));
    assert!(claim_running(&slot, "B").is_err(), "别的会话在跑时必须拒绝");
    assert!(
        claim_running(&slot, "A").is_err(),
        "同一会话的重入也必须拒绝：两轮同时落库就是把 R103 破掉"
    );
    drop(_first);
    assert!(slot.lock().unwrap().is_none(), "guard 析构即释放");
    assert!(claim_running(&slot, "A").is_ok(), "释放后必须能再起一轮");
}

/// R109：命令层不许 `.lock().unwrap()`。这里真的把锁弄中毒，断言闸门照常服务
/// 而不是让之后每一条命令都 panic。
#[test]
fn a_poisoned_slot_still_serves_claims() {
    let slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let poisoning = slot.clone();
    let h = std::thread::spawn(move || {
        let _held = poisoning.lock();
        panic!("故意中毒");
    });
    let _ = h.join();
    assert!(slot.lock().is_err(), "夹具前提：这把锁现在确实是中毒的");
    let g = claim_running(&slot, "A").expect("中毒不该让命令层 panic");
    assert_eq!(
        slot.lock().unwrap_or_else(|e| e.into_inner()).as_deref(),
        Some("A")
    );
    drop(g);
    assert!(slot.lock().unwrap_or_else(|e| e.into_inner()).is_none());
}

// ---- Task 18 Step 4：raw body → 归一化命令 ----

/// `InvokeBody::Raw` 里的字节必须原样交给归一化，一个字节都不许动：
/// 图片解码按内容判定格式（`with_guessed_format`），途中任何转码/裁剪都会让它判错。
#[test]
fn a_raw_body_is_handed_over_byte_for_byte() {
    let bytes: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
    let got = take_raw_body(&InvokeBody::Raw(bytes.clone())).expect("raw 必须收");
    assert_eq!(got, bytes, "raw body 不许被改写");
}

/// JSON 分支必须**明确拒**，不许静默按空字节去解码。
/// R141：raw 与 JSON 字段互斥 —— 前端 `invoke("normalize_image", bytes)` 一旦写成
/// `invoke("normalize_image", { bytes })`，body 就变成 `Json`，这条测试是唯一能抓住它的地方。
#[test]
fn a_json_body_is_refused_and_named_as_json() {
    let err = take_raw_body(&InvokeBody::Json(json!({ "bytes": "aGVsbG8=" })))
        .expect_err("JSON body 必须拒收");
    assert!(
        err.contains("json"),
        "报错要指名收到的形态，否则前端只能对着「图片无法识别」猜：{err}"
    );
}

/// `generate_handler!` 是宏，漏登记不会编译报错，只会在运行时让前端收到
/// "command normalize_image not found" —— 单测碰不到，所以按本项目既有的「源码级守卫」
/// 一路（`tokens.test.ts` 解析 styles.css、T14 的线格式镜像）在这里直接读源码断言。
#[test]
fn the_normalization_command_is_registered_in_the_handler_list() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let list = src
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|rest| rest.split("]").next())
        .expect("找不到 generate_handler 的登记表");
    // F6-N4：原来写的是 `list.contains("normalize_image,")` —— 它硬依赖尾逗号，
    // 登记项哪天被挪成末项（末项无逗号）就**假红**。按分隔符拆开、逐项 trim 再比，
    // 对空白与尾逗号都不敏感；少登记仍然是红（拆出来的集合里根本没有这一项）。
    let registered: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    assert!(
        registered.contains(&"normalize_image"),
        "命令写了但没登记，前端调用只会拿到 not found：{registered:?}"
    );
    // 技能面板与记忆面板的命令同一条判据：没登记就是面板点开一片空。
    for name in [
        "skills_list",
        "skills_open_folder",
        "skills_set_enabled",
        "memories_list",
        "memories_delete",
        "memories_clear",
        "memory_profile",
        "memory_flush_session",
        "memory_update_body",
        "memory_add",
    ] {
        assert!(
            registered.contains(&name),
            "{name} 写了但没登记，对应面板只会拿到 not found：{registered:?}"
        );
    }
}

/// Agent Shell 的两条命令与上面同一个坑（`generate_handler!` 是宏）：漏登记不编译
/// 报错，只会在运行时让前端拿到 not found。机制照抄
/// `the_normalization_command_is_registered_in_the_handler_list`。
#[test]
fn the_shell_commands_are_registered_in_the_handler_list() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let list = src
        .split("tauri::generate_handler![")
        .nth(1)
        .and_then(|rest| rest.split("]").next())
        .expect("找不到 generate_handler 的登记表");
    let registered: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    for cmd in ["shell_presets", "shell_profile_validate"] {
        assert!(
            registered.contains(&cmd),
            "命令 {cmd} 写了但没登记，前端调用只会拿到 not found:{registered:?}"
        );
    }
}

// ---- resolve_api_key：本机模型免 key 的唯一闸门 ----
//
// 这三条断言是一组「放宽不许越界」的锁：第一条是新行为，后两条是既有行为，
// 谁哪天把分支写成 `if base_url.contains("localhost")` 之类，后两条之一必红。

#[test]
fn a_loopback_endpoint_needs_no_stored_key() {
    // ollama / LM Studio 的默认形态。空串而不是 None 是刻意的：provider 侧
    // 只认「有 key 就带 header」，空串正好等于不带。
    assert_eq!(
        resolve_api_key(None, "http://127.0.0.1:11434/v1"),
        Ok(String::new())
    );
    assert_eq!(
        resolve_api_key(None, "http://localhost:11434/v1"),
        Ok(String::new())
    );
}

#[test]
fn a_remote_endpoint_still_rejects_a_missing_key() {
    for base_url in [
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "https://api.openai.com/v1",
        // 前缀骗人：字符串看像 127.，实际是 DNS 名。
        "http://127.0.0.1.evil.test/v1",
        // 0.0.0.0 语义是「所有网卡」，不算本机。
        "http://0.0.0.0:11434/v1",
    ] {
        let err = resolve_api_key(None, base_url)
            .expect_err("非回环端点没有 key 必须停在这里，不能带着空 header 发出去");
        assert_eq!(
            err, "尚未配置 API key，请在设置中填写",
            "报文是前端直接展示的文案，不许漂"
        );
    }
}

#[test]
fn a_stored_key_passes_through_byte_identical_on_both_paths() {
    for base_url in ["http://127.0.0.1:11434/v1", "https://remote.test/v1"] {
        assert_eq!(
            resolve_api_key(Some("sk-secret".into()), base_url),
            Ok("sk-secret".into())
        );
        // 抽函数之前 `Some("")` 就是能过的。这里维持原样：本轮只做「None 且回环」
        // 这一件事，顺手收紧空串会让非回环路径的行为不再是「逐字不变」。
        assert_eq!(
            resolve_api_key(Some(String::new()), base_url),
            Ok(String::new())
        );
    }
}

/// T27 的**接线**判据（评审 Important-1 → R287）。为什么只能是这样：
/// `app.manage(AppState{…})` 在 Tauri 的 setup 闭包里，单测既造不出 `App` 也进不去那个闭包，
/// 于是 `build_production_registry()` 自己绿了 405 条，**装配那行退回空表却一条都抓不到**
/// （实现者亲跑的 M4 就是这个形状：405 passed / 0 failed）。
/// 本项目对「宏/闭包造不出来的接线」已有同一路判例 —— `the_normalization_command_is_registered_in_the_handler_list`
/// （`:2435`，`generate_handler!` 是宏）与前端 `tokens.test.ts`（解析 styles.css），
/// 所以这里读自己的源码，不新开机制，只沿用既有那条线。
/// 取法：**逐行**取 `registry:` 那一行并按行剥掉注释与尾逗号。为什么不按 `,` 拆整个字面量 ——
/// 本文件是 CRLF，按 `,` 拆会把行尾注释和下一行粘成一段（M4 第一次跑就是这样露出来的），
/// 而 `rustfmt` 对这个 7 字段字面量恒按「一字段一行」排；不硬依赖尾逗号则是 F6-N4 的教训。
#[test]
fn the_assembly_wires_the_production_registry() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let manage = src
        .split("app.manage(AppState {")
        .nth(1)
        .and_then(|rest| rest.split("});").next())
        .expect("找不到 app.manage(AppState { … }) 那段装配");
    let wired: Vec<&str> = manage
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("registry:"))
        .map(|line| line.split("//").next().unwrap_or("").trim())
        .map(|line| line.trim_end_matches(',').trim())
        .collect();
    assert_eq!(
        wired.len(),
        1,
        "装配段里 `registry:` 那一行必须**恰好一条**（0 条=换了写法或没了，2 条=两条装配路径）；\
         实得 {wired:?}"
    );
    // 按**前缀**匹配而不是整行:T11 起这个函数要收一个 `Arc<Db>` 才能注册
    // `memory_search`,把整行钉死会让守卫在参数变化时假红。它要防的是
    // 「这一行不再是生产表的装配点」,不是「参数永远为空」。
    assert!(
        wired[0].starts_with("registry: build_production_registry("),
        "装配点没接上生产工具表：模型永远只有 0 把工具，而其余单测全绿 \
         （`production_registry_ships_fetch_url_and_read_file` 量的是那个函数自己，\
         不是这行接线 —— 实现者亲跑的 M4 就是这个形状）；实得 {:?}",
        wired[0]
    );
    // 第二半：`#[cfg(test)]` 之前**只许**有一处 `Arc::new(ToolRegistry::new())`，而且它必须在
    // `build_production_registry` 的函数体里 —— 出现第二处就等于有了第二条装配路径，
    // 而 R101（一个目录只许一个 `SettingsStore`）说的正是这种重复会各自漂移。
    let production = src
        .split("#[cfg(test)]")
        .next()
        .expect("找不到 #[cfg(test)] 边界");
    let empty_constructions: Vec<&str> = production
        .lines()
        .filter(|line| line.contains("Arc::new(ToolRegistry::new())"))
        .map(str::trim)
        .collect();
    assert_eq!(
        empty_constructions.len(),
        1,
        "生产侧只许在 `build_production_registry` 体内空构造注册表一次；实得 {empty_constructions:?}"
    );
    assert!(
        empty_constructions[0].starts_with("let registry ="),
        "那唯一一处空构造必须是 `build_production_registry` 的第一行，不是别处的第二条装配路径；\
         实得 {:?}",
        empty_constructions[0]
    );
    // Agent Shell（spec §5.5）：装配段还必须把 `shells` 字段接进 `AppState` ——
    // `ShellTool` 经 `build_production_registry` 拿到的是同一个共享句柄，漏接这一半
    // 就等于工具表里有一把永远解析不到自定义 profile 的 run_command（每轮现刷的
    // 另一半由 `send_message` 那行做，单测调不动，见该行注释）。按同一取法逐行取：
    // 字段初始化若写成简写 `shells,`（无冒号）也认得 —— 按**前缀**匹配「shells」，
    // 不硬钉 `shells:` 的写法（F6-N4 同一教训：不依赖具体拼写形状）。
    let shells_wired: Vec<&str> = manage
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("shells"))
        .map(|line| line.split("//").next().unwrap_or("").trim())
        .map(|line| line.trim_end_matches(',').trim())
        .collect();
    assert_eq!(
        shells_wired.len(),
        1,
        "装配段里 `shells` 那一行必须**恰好一条**（0 条=没接线，2 条=两条接线路径）；\
         实得 {shells_wired:?}"
    );
    assert_eq!(
        shells_wired[0], "shells",
        "`AppState.shells` 必须接到共享句柄上（字段初始化 `shells`/`shells: shells` 均可）；\
         实得 {:?}",
        shells_wired[0]
    );
}

// ───────────────────────── T35：MCP 视图与工具注册 ─────────────────────────

fn stdio_def(name: &str) -> ServerDef {
    ServerDef::Stdio {
        name: name.to_string(),
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@x/y".to_string()],
        env: std::collections::HashMap::new(),
    }
}

#[test]
fn mcp_servers_view_never_leaks_env_or_headers() {
    let mut env = std::collections::HashMap::new();
    env.insert("TOKEN".to_string(), "sk-REAL-SECRET".to_string());
    let def = ServerDef::Stdio {
        name: "search".to_string(),
        command: "npx".to_string(),
        args: vec![],
        env,
    };
    let bridge = McpBridge::new();
    let gate = PermissionGate::new();
    let v = mcp_servers_of(&[def], &bridge, &gate);
    let s = serde_json::to_string(&v).unwrap();
    assert!(
        !s.contains("sk-REAL-SECRET"),
        "命令返回值会进 DevTools 与日志：{s}"
    );
    assert!(s.contains("\"tools\":0"), "视图只要状态与数量：{s}");
    assert!(
        s.contains("\"trust\":\"ask\""),
        "没登记过的 server 在 gate 眼里就是 Ask（R321），界面不许显示成 auto：{s}"
    );
}

#[test]
fn a_ready_server_reports_its_tools_and_its_trust_word() {
    let bridge = McpBridge::new();
    bridge.set_status("search", ServerStatus::Ready { tools: 4 });
    let gate = PermissionGate::new();
    gate.rules().set_server_trust("search", Verdict::Auto);
    let v = mcp_servers_of(&[stdio_def("search")], &bridge, &gate);
    assert_eq!(v[0].status, "ready");
    assert_eq!(v[0].tools, 4);
    assert_eq!(v[0].trust, "auto");
    assert!(v[0].reason.is_none());

    let bridge = McpBridge::new();
    bridge.set_status(
        "dead",
        ServerStatus::Disconnected {
            reason: "spawn 失败：program not found".to_string(),
        },
    );
    let v = mcp_servers_of(&[stdio_def("dead")], &bridge, &PermissionGate::new());
    assert_eq!(v[0].status, "disconnected");
    assert_eq!(
        v[0].reason.as_deref(),
        Some("spawn 失败：program not found")
    );
}

#[tokio::test]
async fn an_empty_mcp_config_bootstraps_to_nothing() {
    let out = mcp_bootstrap(
        &Arc::new(McpBridge::new()),
        &ToolRegistry::new(),
        &PermissionGate::new(),
        vec![],
    )
    .await;
    assert!(out.is_empty(), "没有配置就不许 spawn 任何东西");
}

/// 注册期必须把「注册名 → 真名」喂进 gate 的 alias 表：这是兜底命名 server 的
/// 信任档（Block/Auto）能生效的唯一通路（T33 按真名存、T30 注册名还原不出真名）。
/// 走真注册函数 + 真 gate.check 全链验证：alias 没喂上，Block 就落空。
#[tokio::test]
async fn mcp_registration_feeds_the_gate_the_registered_to_real_name_alias() {
    let bridge = Arc::new(McpBridge::new());
    let registry = ToolRegistry::new();
    let gate = PermissionGate::new();
    let server = "a-very-long-community-server-name-that-goes-on";
    let decl = rmcp::model::Tool::new(
        "get_the_current_weather_forecast_for_a_location",
        "查天气",
        serde_json::Map::new(),
    );
    mcp_register_tools(
        &registry,
        &gate,
        &bridge,
        server,
        std::slice::from_ref(&decl),
    );

    // 前提：这条工具名确实是兜底形状（截断的 server 段，还原不出真名）。
    let registered = registry.defs()[0].name.clone();
    assert!(
        !registered.starts_with(&format!("mcp__{server}__")),
        "前提：注册名是兜底形状（server 段被截断）：{registered}"
    );
    gate.rules().set_server_trust(server, Verdict::Block);

    let view = ToolCallView {
        id: "c1".into(),
        name: registered,
        risk: Risk::Read,
        input: json!({}),
    };
    let d = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        gate.check(&view, &tokio_util::sync::CancellationToken::new()),
    )
    .await
    .expect("Block 必须当场拒绝");
    assert!(
        matches!(&d, crate::agent::events::Decision::Deny { reason }
            if reason.contains("被设为 Block") && reason.contains(server)),
        "Block 必须经 alias 命中真名并压下来（alias 没喂上时这里落空成默认 Ask）：{d:?}"
    );
}

#[test]
fn trust_is_written_to_disk_and_read_back_on_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let gate = PermissionGate::new();
    mcp_set_trust_of(&gate, dir.path(), "search", "auto").expect("写盘");
    assert_eq!(
        crate::mcp::trust::load(dir.path()).get("search"),
        Some(&Verdict::Auto),
        "R329：下次启动必须读回同一档位"
    );
    assert!(
        mcp_set_trust_of(&gate, dir.path(), "x", "yes").is_err(),
        "未知档位不许写盘"
    );
}

#[test]
fn trust_words_round_trip_and_reject_junk() {
    for word in ["auto", "ask", "block"] {
        let v = parse_trust(word).expect("三个合法档位都要认");
        assert_eq!(trust_word(v), word);
    }
    assert!(
        parse_trust("yes").is_err(),
        "未知档位必须报错，不许静默当成 ask"
    );
}

// ───────────────────────── T4：MCP 配置编辑命令 ─────────────────────────

#[test]
fn mcp_config_status_never_carries_secrets() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mcp.json"),
        r#"{ "mcpServers": { "x": { "command": "npx", "env": { "TOKEN": "sk-REAL-SECRET" } } } }"#,
    )
    .unwrap();

    let v = mcp_config_status_of(dir.path()).unwrap();
    let s = serde_json::to_string(&v).unwrap();
    assert!(!s.contains("sk-REAL-SECRET"), "{s}");
    assert!(s.contains("\"parseError\":null"), "{s}");
    assert!(s.contains("mcp.json"), "{s}");
}

#[test]
fn mcp_config_read_carries_the_text_only_for_the_explicit_editor() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mcp.json"),
        r#"{ "mcpServers": { "x": { "command": "npx", "env": { "TOKEN": "sk-REAL-SECRET" } } } }"#,
    )
    .unwrap();

    let v = mcp_config_read_of(dir.path()).unwrap();
    assert!(
        v.text.contains("sk-REAL-SECRET"),
        "编辑器必须能显示要编辑的原文"
    );
    let s = serde_json::to_string(&v).unwrap();
    assert!(s.contains("\"text\""), "{s}");
}

#[test]
fn mcp_config_save_of_rejects_invalid_text_and_writes_valid_text() {
    let dir = tempfile::tempdir().unwrap();
    assert!(mcp_config_save_of(dir.path(), "{ nope").is_err());
    assert!(!dir.path().join("mcp.json").exists());

    mcp_config_save_of(dir.path(), r#"{ "mcpServers": {} }"#).unwrap();
    assert!(dir.path().join("mcp.json").exists());
}

#[test]
fn registered_mcp_tools_are_removed_by_exact_name_even_when_truncated() {
    let bridge = Arc::new(McpBridge::new());
    let registry = ToolRegistry::new();
    let decl = rmcp::model::Tool::new(
        "get_the_current_weather_forecast_for_a_location",
        "查天气",
        serde_json::Map::new(),
    );
    // 长 server 名 ⇒ T30 的兜底命名（server 段被截断）⇒ 前缀法摘不到，必须按精确名摘。
    let server = "a-very-long-community-server-name-that-goes-on";
    registry.register(Arc::new(McpTool::from_decl(server, &decl, bridge.clone())));
    let registered = registry.defs();
    assert_eq!(registered.len(), 1);
    assert!(
        !registered[0].name.starts_with(&format!("mcp__{server}__")),
        "前提：这条工具名确实是兜底形状（截断的 server 段）"
    );
    let removed = mcp_clear_tools(&registry, server, std::slice::from_ref(&decl));
    assert_eq!(removed, vec![registered[0].name.clone()]);
    assert!(registry.get(&registered[0].name).is_none());
}

/// `run_turn` 必须把**工具声明的粗估**交给 ContextBuilder。删掉那行接线，
/// 带 tool 的请求会留下和不带 tool 时一样多的历史，这条就红。
/// 历史是连续 4 条 user（= 1 轮），所以压缩计划的 batch 够不到下限：
/// 测的纯粹是滑窗预算，不是压缩。
#[tokio::test]
async fn run_turn_counts_tool_declarations_against_the_budget() {
    async fn last_request_len(with_tool: bool) -> usize {
        let f = Fixture::new();
        if with_tool {
            f.registry.register(Arc::new(BigSchemaTool));
        }
        let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
        let hist: Vec<Message> = (0..4)
            .map(|i| {
                Message::user(vec![ContentBlock::Text {
                    text: format!("{}{i}", "测".repeat(1_000)),
                }])
            })
            .collect();
        seed(&f.db, &sid, &hist);
        let model = FakeModel::summarizing_and_replying(&[], &["ok"]);
        let rec = Arc::new(Recorded::default());
        let sink = Arc::new(TauriEmitter::new(rec, sid.clone()));
        let reason = run_turn(
            TurnDeps {
                output_dir: None,
                db: f.db.clone(),
                registry: f.registry.clone(),
                gate: f.gate.clone(),
                model: model.clone(),
                sink: sink.clone(),
                context: ContextConfig {
                    budget_tokens: 3_000,
                    reserve_output: 0,
                    window_messages: 40,
                    ..Default::default()
                },
                temperature: None,
                plan_mode: false,
                diagnostics_command: None,
                allowed_tools: None,
                active_agent_name: None,
                agents: Vec::new(),
                roots: Vec::new(),
                skills_dir: None,
                skills_disabled: Vec::new(),
                memory: MemoryTurnCfg::default(),
                embed: None,
                extract_locks: ExtractLocks::default(),
                data_dir: None,
                cancel: f.cancel.clone(),
            },
            &sid,
            vec![ContentBlock::Text {
                text: "这一轮的问话".into(),
            }],
        )
        .await
        .expect("一轮必须成功");
        sink.flush();
        assert_eq!(reason, StopReason::EndTurn);
        model
            .calls()
            .last()
            .expect("模型必须收到请求")
            .messages
            .len()
    }

    let plain = last_request_len(false).await;
    let tooled = last_request_len(true).await;
    assert!(
        tooled < plain,
        "工具声明的 token 必须进预算：plain={plain} tooled={tooled}"
    );
}

// ==== T15(P0)：抽取触发（空闲 tick / 切换会话） ====

/// 测试专用的一条用户消息。抽取源只认带文本的对话行。
fn user_says(text: &str) -> Message {
    Message {
        role: Role::User,
        blocks: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    }
}

/// 把会话的最后活动时刻推到过去。空闲 tick 的唯一判据是 `sessions.updated_at`，
/// 而生产里没有任何路径该改它 —— 所以这里直接写库，**不**为测试开一个 store 方法。
fn backdate_session(db: &Db, sid: &str, at_ms: i64) {
    let c = db.lock();
    c.execute(
        "UPDATE sessions SET updated_at=?2 WHERE id=?1",
        rusqlite::params![sid, at_ms],
    )
    .unwrap();
}

/// 抽取请求 = 头一条 system 是 `EXTRACT_INSTRUCTION` 的那些。合并/画像请求也进
/// `FakeModel::calls`，不筛掉的话「模型被调用了几次」会把三件事混成一个数。
fn extraction_requests(model: &FakeModel) -> Vec<crate::model::provider::ChatRequest> {
    model
        .calls()
        .into_iter()
        .filter(|r| {
            r.messages
                .first()
                .filter(|m| m.role == Role::System)
                .map(|m| m.text_of())
                .as_deref()
                == Some(EXTRACT_INSTRUCTION)
        })
        .collect()
}

/// T15 的核心不变量之一：一次抽取最多吃 40 条，超出的部分留给下一个 tick，
/// 游标只推进到**真正送进模型**的那一条。
///
/// 为什么这条重要：没有上限时，一个聊了 500 轮的会话会在第一次 tick 里把整段
/// 历史塞进一次请求（成本不可控）；推进到 45 而不是 40 则会把 41..=45 永久跳过
/// —— 那正是「游标推过没抽成的对话」的静默丢失。两半都要钉。
#[tokio::test]
async fn idle_extraction_takes_at_most_forty_messages_and_advances_only_to_the_last_one_it_saw() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    for i in 0..45 {
        f.db.append_message(&sid, &user_says(&format!("第 {i} 句")))
            .unwrap();
    }
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let memory = MemoryTurnCfg::default();

    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &memory,
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("抽取成功");
    assert_eq!(changed, 1, "一条新事实");
    assert_eq!(
        f.db.memory_cursor(&sid).unwrap(),
        40,
        "游标停在第 40 条：41..=45 必须留给下一个 tick"
    );
    let first = extraction_requests(&model);
    assert_eq!(first.len(), 1, "第一次只该有一次抽取调用");
    assert_eq!(
        first[0].messages.len(),
        41,
        "指令 + 40 条对话，多一条都是超限"
    );

    // 第二个 tick 把剩下的 5 条抽掉。同一候选会撞上第一次写的同键行 ⇒ 合并请求
    // 走 FakeModel 的默认空回复（解析失败 → 跳过候选），但**抽取本身成功**，
    // 游标必须继续前进 —— 「没什么新东西」不等于「没抽过」。
    let changed2 = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &memory,
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("第二次抽取也成功");
    assert_eq!(changed2, 0, "同键候选在合并段被跳过，不产生变更");
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 45, "剩余 5 条处理完");
    let all = extraction_requests(&model);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].messages.len(), 6, "指令 + 剩下的 5 条");
}

/// 可抽取行全部处理完后，游标要跨过尾部那些**不可抽取**的行（工具结果等）。
/// 只推到「最后一条可抽取行」会让 `max(id) > cursor` 永远成立，这条会话每个
/// 60s tick 都会被重新查一遍，后台永久空转。
#[tokio::test]
async fn an_unextractable_tail_advances_the_cursor_instead_of_rescanning_forever() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("我住在北京")).unwrap();
    f.db.append_message(
        &sid,
        &Message::tool(
            "c1",
            crate::agent::types::ToolOutput::text("工具输出不是对话"),
            false,
        ),
    )
    .unwrap();
    let model = FakeModel::extracting(&["[]"], &[]);

    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &MemoryTurnCfg::default(),
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("抽取成功");
    assert_eq!(changed, 0, "空候选 = 这次没什么可记");
    assert_eq!(
        f.db.memory_cursor(&sid).unwrap(),
        2,
        "游标必须跨过尾部那条工具结果，推到 max(id)"
    );
    assert_eq!(model.calls().len(), 1, "可抽取的那一条仍然送模型");
}

/// 整段都不可抽取（例如会话只有工具结果）：不构造抽取调用，但游标也要推过去。
#[tokio::test]
async fn a_session_with_nothing_extractable_advances_the_cursor_without_a_model_call() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(
        &sid,
        &Message::tool(
            "c1",
            crate::agent::types::ToolOutput::text("只有工具结果"),
            false,
        ),
    )
    .unwrap();
    let model = FakeModel::extracting(&["[]"], &[]);

    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &MemoryTurnCfg::default(),
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("空行集是成功");
    assert_eq!(changed, 0);
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 1);
    assert!(model.calls().is_empty(), "没有可抽取行时不许调用模型");
}

/// T15 与压缩路径**取向相反**的那条：抽取失败时游标不动，下一个 tick 重试。
/// 如果这里把 Err 吞成「零写入 + 推进游标」，那 3 条对话就永久不会再被抽取
/// （它们已经落在游标之前），下一次压缩会把它们当摘要处理。
#[tokio::test]
async fn a_failed_idle_extraction_keeps_the_cursor_so_the_next_tick_retries() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    for i in 0..3 {
        f.db.append_message(&sid, &user_says(&format!("第 {i} 句")))
            .unwrap();
    }
    let memory = MemoryTurnCfg::default();
    let broken = FakeModel::extraction_failing(&[r#"[{"kind":"fact""#]);
    let err = extract_session_memories(
        &f.db,
        &sid,
        &*broken,
        &memory,
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect_err("抽取调用报错必须冒泡");
    assert!(err.contains("抽取"), "失败面要说清是哪一段：{err}");
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 0, "游标必须原地不动");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "半截输出不许落库"
    );

    // 重试：换一个能答的模型，同一条对话仍然抽得到（证明游标真的没吞掉它）。
    let healthy = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*healthy,
        &memory,
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("重试成功");
    assert_eq!(changed, 1);
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 3, "这次才推进");
}

/// 没有新消息时连模型都不调：tick 每分钟跑一次，query 不该变成一次白烧的调用。
#[tokio::test]
async fn an_idle_extraction_with_nothing_new_never_calls_the_model() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("已经抽过的一句话"))
        .unwrap();
    let max = f.db.max_message_id(&sid).unwrap();
    f.db.apply_memories_and_advance_cursor(&sid, &[], max)
        .unwrap();

    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &MemoryTurnCfg::default(),
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("空集是成功，不是错误");
    assert_eq!(changed, 0);
    assert!(
        model.calls().is_empty(),
        "游标之后没有内容 ⇒ 一次模型调用都不该发生"
    );
}

/// 画像写入不计入「变更条数」：它是覆盖写，不是一条新记忆。设置页用它显示
/// 「这次抽到几条」，把画像算进来会让数字对不上账。
#[tokio::test]
async fn the_idle_extraction_count_excludes_the_profile_upsert() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("我在学日语")).unwrap();
    let model = FakeModel::with_profile(
        FakeModel::extracting(&[&format!("[{}]", profile_candidate("用户在学日语"))], &[]),
        &["用户在学日语"],
    );
    let changed = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &MemoryTurnCfg::default(),
        None,
        &ExtractLocks::default(),
        &CancellationToken::new(),
    )
    .await
    .expect("抽取成功");
    assert_eq!(changed, 0, "画像不算一条记忆变更");
    assert_eq!(f.db.profile().unwrap(), "用户在学日语");
    assert_eq!(
        f.db.memory_cursor(&sid).unwrap(),
        1,
        "画像写成功也要推进游标"
    );
}

/// 「本会话都允许」必须跟着会话走：同一会话的两次发送不清，切到另一条会话才清。
#[test]
fn session_scope_rules_are_cleared_when_the_session_changes() {
    let gate = PermissionGate::new();
    let last = Mutex::new(None);

    enter_session_scope(&last, &gate, "A");
    gate.rules().remember_session("read_file|read");
    enter_session_scope(&last, &gate, "A");
    assert!(
        gate.rules().lookup("read_file|read").is_some(),
        "同一会话内不许清掉用户答过的授权"
    );

    enter_session_scope(&last, &gate, "B");
    assert!(
        gate.rules().lookup("read_file|read").is_none(),
        "切到另一条会话必须清掉上一会话的授权"
    );
}

// ==== T16(P0)：user_edited 保护 ====

/// 种一条**用户改过**的记忆。先按模型路径落一行（必须有 subject+predicate，
/// 否则 `find_active_by_key` 捞不到它，保护集就会是空的），再走用户的编辑入口
/// 打上 `user_edited`。返回行 id。
fn seed_user_edited_row(db: &Db, sid: &str, body: &str) -> i64 {
    let ids = db
        .apply_memories_and_advance_cursor(
            sid,
            &[MemoryWrite::Add(NewMemory {
                session_id: Some(sid.to_string()),
                kind: "fact".into(),
                subject: Some("用户".into()),
                predicate: Some("住在".into()),
                object: Some("北京".into()),
                body: body.into(),
                confidence: 0.95,
                due_at: None,
            })],
            0,
        )
        .unwrap();
    let id = ids[0];
    db.update_memory_body_by_user(id, "用户亲手改过的正文")
        .unwrap();
    id
}

/// 模型说 UPDATE，但那一行被用户改过 → 降级成 ADD（并存），用户正文不变。
#[tokio::test]
async fn an_update_aimed_at_a_user_edited_row_is_downgraded_to_an_add() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let id = seed_user_edited_row(&f.db, &sid, "用户住在北京");
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
            &[],
        ),
        &[&format!(
            r#"{{"action":"UPDATE","id":{id},"body":"用户住在上海"}}"#
        )],
    );
    let writes = plan_memory_writes(
        &f.db,
        &sid,
        &*model,
        &[user_says("我搬到上海了")],
        500,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(writes.len(), 1, "一条候选只产出一条写入：{writes:?}");
    assert!(
        matches!(&writes[0], MemoryWrite::Add(m) if m.body == "用户搬到上海"),
        "UPDATE 必须降级成 ADD，而不是改写保护行：{writes:?}"
    );

    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .unwrap();
    let valid = f.db.valid_memories().unwrap();
    assert!(
        valid
            .iter()
            .any(|r| r.id == id && r.body == "用户亲手改过的正文" && r.user_edited),
        "用户改过的正文一个字都不许动：{valid:?}"
    );
    assert!(
        valid.iter().any(|r| r.body == "用户搬到上海"),
        "新候选必须并存入库：{valid:?}"
    );
}

/// 模型返回的 id 属于**另一条键**的真实记忆：跳过候选，不写库，
/// 也不许把整个压缩事务拖回滚。用真实 id 而不是 999 —— 「只要 id 在库中存在」的
/// 弱实现能骗过伪造 id，但骗不过这一条。
#[tokio::test]
async fn a_consolidation_id_outside_the_same_key_set_is_skipped() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let add = |predicate: &str, object: &str, body: &str| NewMemory {
        session_id: Some(sid.clone()),
        kind: "fact".into(),
        subject: Some("用户".into()),
        predicate: Some(predicate.into()),
        object: Some(object.into()),
        body: body.into(),
        confidence: 0.95,
        due_at: None,
    };
    let beijing =
        f.db.apply_memories_and_advance_cursor(
            &sid,
            &[MemoryWrite::Add(add("住在", "北京", "用户住在北京"))],
            0,
        )
        .unwrap()[0];
    let other =
        f.db.apply_memories_and_advance_cursor(
            &sid,
            &[MemoryWrite::Add(add("喜欢", "咖啡", "用户喜欢咖啡"))],
            0,
        )
        .unwrap()[0];
    assert_ne!(beijing, other, "两条记忆必须是不同的行");

    let model = FakeModel::with_consolidation_queue(
        FakeModel::extracting(
            // 两个候选必须在**同一个 JSON 数组**里；拆成两段会让抽取解析失败，
            // 测试就会因为「零候选」而假绿。
            &[&format!(
                "[{},{}]",
                fact_candidate("上海", "用户搬到上海"),
                fact_candidate("杭州", "用户搬到杭州")
            )],
            &[],
        ),
        &[
            &format!(r#"{{"action":"UPDATE","id":{other},"body":"被跨键改写"}}"#),
            &format!(r#"{{"action":"INVALIDATE","id":{other}}}"#),
        ],
    );
    let writes = plan_memory_writes_for_source(
        &f.db,
        &sid,
        &*model,
        &[user_says("我搬家了")],
        500,
        &CancellationToken::new(),
    )
    .await
    .expect("抽取与两次合并都必须成功（否则「零写入」可能只是夹具坏了）");
    assert_eq!(
        model.calls().len(),
        3,
        "一次抽取 + 两个候选各一次合并；少于三条说明管线根本没跑到合并"
    );
    assert!(
        writes.is_empty(),
        "不属于同键集的 id 必须被跳过，而不是写进事务：{writes:?}"
    );
    // 真正的回归点：这些写入交给事务时不许报错（0 行命中会回滚整次压缩）。
    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .expect("被跳过的候选不许让事务失败");
    let valid = f.db.valid_memories().unwrap();
    assert_eq!(valid.len(), 2, "两条记忆都必须还在：{valid:?}");
    let coffee = valid.iter().find(|r| r.id == other).expect("咖啡那条");
    assert_eq!(coffee.body, "用户喜欢咖啡", "跨键行不许被改写");
    let bj = valid.iter().find(|r| r.id == beijing).expect("北京那条");
    assert_eq!(bj.body, "用户住在北京");
}

/// 两个同键候选都判同一行 INVALIDATE：两条决定都要保留（计划层不按 id 去重），
/// 但写入层必须把第二条的 0 行命中当幂等，而不是回滚整次压缩。
#[tokio::test]
async fn duplicate_invalidate_for_one_id_is_idempotent() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let id =
        f.db.apply_memories_and_advance_cursor(
            &sid,
            &[MemoryWrite::Add(NewMemory {
                session_id: Some(sid.clone()),
                kind: "fact".into(),
                subject: Some("用户".into()),
                predicate: Some("住在".into()),
                object: Some("北京".into()),
                body: "用户住在北京".into(),
                confidence: 0.95,
                due_at: None,
            })],
            0,
        )
        .unwrap()[0];
    let model = FakeModel::with_consolidation_queue(
        FakeModel::extracting(
            &[&format!(
                "[{},{}]",
                fact_candidate("上海", "用户搬到上海"),
                fact_candidate("杭州", "用户搬到杭州")
            )],
            &[],
        ),
        &[
            &format!(r#"{{"action":"INVALIDATE","id":{id}}}"#),
            &format!(r#"{{"action":"INVALIDATE","id":{id}}}"#),
        ],
    );
    let writes = plan_memory_writes_for_source(
        &f.db,
        &sid,
        &*model,
        &[user_says("我搬家了")],
        500,
        &CancellationToken::new(),
    )
    .await
    .expect("抽取与两次合并都必须成功");
    assert_eq!(model.calls().len(), 3, "一次抽取 + 两次合并");
    assert_eq!(
        writes.len(),
        2,
        "计划层不许吞掉第二条决定（否则 REINFORCE 之后的 INVALIDATE 会丢）：{writes:?}"
    );
    assert!(
        writes
            .iter()
            .all(|w| matches!(w, MemoryWrite::Invalidate { id: got } if *got == id)),
        "两条都必须是针对同一行的失效：{writes:?}"
    );
    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .expect("重复失效必须幂等：0 行命中不许回滚整批");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "那一条必须已经失效"
    );
}

/// 模型说 INVALIDATE，但那一行被用户改过 → 同样降级成 ADD；**旧行保持有效**。
/// 这是「用户权威优先于系统自洽」最直接的一条：模型认为它过时了，但那是用户写的。
#[tokio::test]
async fn an_invalidate_aimed_at_a_user_edited_row_is_downgraded_and_the_row_stays_valid() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let id = seed_user_edited_row(&f.db, &sid, "用户住在北京");
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
            &[],
        ),
        &[&format!(r#"{{"action":"INVALIDATE","id":{id}}}"#)],
    );
    let writes = plan_memory_writes(
        &f.db,
        &sid,
        &*model,
        &[user_says("我搬到上海了")],
        500,
        &CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(&writes[..], [MemoryWrite::Add(m)] if m.body == "用户搬到上海"),
        "INVALIDATE 必须降级成 ADD：{writes:?}"
    );

    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .unwrap();
    let valid = f.db.valid_memories().unwrap();
    assert!(
        valid.iter().any(|r| r.id == id),
        "用户编辑过的行在模型说 INVALIDATE 之后仍然有效：{valid:?}"
    );
    assert!(
        valid.iter().any(|r| r.body == "用户搬到上海"),
        "新候选并存：{valid:?}"
    );
}

/// 单独钉住「失效之后它还在」：上面那条测的是**降级动作**，这条测的是**结果状态**。
/// `valid_memories` 是检索/注入的唯一口径（`invalid_at IS NULL`），只要它还在，
/// 「用户写下的东西仍然能用」就成立。
#[tokio::test]
async fn a_user_edited_row_stays_valid_after_the_model_invalidates_it() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let id = seed_user_edited_row(&f.db, &sid, "用户住在北京");
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
            &[],
        ),
        &[&format!(r#"{{"action":"INVALIDATE","id":{id}}}"#)],
    );
    let writes = plan_memory_writes(
        &f.db,
        &sid,
        &*model,
        &[user_says("我搬到上海了")],
        500,
        &CancellationToken::new(),
    )
    .await;
    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .unwrap();

    let valid = f.db.valid_memories().unwrap();
    assert!(
        valid.iter().any(|r| r.id == id && r.invalid_at.is_none()),
        "用户编辑过的行不许被模型软删：{valid:?}"
    );
    // 反向断言让「根本没写进去」也不至于假绿:同一批里必须有一条新记忆。
    assert!(
        valid.iter().any(|r| r.body == "用户搬到上海"),
        "新旧必须并存：{valid:?}"
    );
}

/// REINFORCE 指向保护行**允许**：它不改正文，只把「同一件事又被说了一遍」的
/// 计数加一。用户写的句子被再说一遍而变强，是正确语义，没有内容需要保护。
#[tokio::test]
async fn a_reinforce_aimed_at_a_user_edited_row_still_counts() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    let id = seed_user_edited_row(&f.db, &sid, "用户住在北京");
    let model = FakeModel::with_consolidation(
        FakeModel::extracting(
            &[&format!("[{}]", fact_candidate("北京", "用户还住在北京"))],
            &[],
        ),
        &[&format!(r#"{{"action":"REINFORCE","id":{id}}}"#)],
    );
    let writes = plan_memory_writes(
        &f.db,
        &sid,
        &*model,
        &[user_says("我还是住在北京")],
        500,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        writes,
        vec![MemoryWrite::Reinforce { id }],
        "强化保护行必须原样放行：{writes:?}"
    );

    f.db.apply_memories_and_advance_cursor(&sid, &writes, 1)
        .unwrap();
    let row =
        f.db.valid_memories()
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .expect("保护行仍然有效");
    assert_eq!(row.reinforcements, 1, "确认计数必须加一");
    assert_eq!(
        row.body, "用户亲手改过的正文",
        "强化不许碰正文（这正是它可以指向保护行的理由）"
    );
}

// ==== T15：空闲 tick + 装配接线 ====

#[tokio::test]
async fn the_idle_tick_does_nothing_when_the_user_turned_it_off() {
    let locks = ExtractLocks::default();
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("该被抽但关闭了"))
        .unwrap();
    backdate_session(&f.db, &sid, Db::now_ms() - 60 * 60 * 1_000);

    let settings = Settings {
        memory_idle_extract_minutes: 0,
        ..Default::default()
    };
    let running = Arc::new(Mutex::new(None));
    let factories = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = factories.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        };

    let n = idle_extract_tick(&f.db, &settings, &running, &factory, &locks).await;
    assert_eq!(n, 0);
    assert_eq!(
        factories.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "关闭档下连模型工厂都不许被调用"
    );
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 0, "游标不许动");
}

#[tokio::test]
async fn the_idle_tick_does_nothing_when_memory_is_disabled() {
    let locks = ExtractLocks::default();
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("记忆总开关关了"))
        .unwrap();
    backdate_session(&f.db, &sid, Db::now_ms() - 60 * 60 * 1_000);

    let settings = Settings {
        memory_enabled: false,
        ..Default::default()
    };
    let running = Arc::new(Mutex::new(None));
    let factories = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = factories.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        };
    assert_eq!(
        idle_extract_tick(&f.db, &settings, &running, &factory, &locks).await,
        0
    );
    assert_eq!(
        factories.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "总开关关了 ⇒ 零模型调用（与压缩路径同一条纪律）"
    );
}

/// 「绝不并发同一会话」：running 槽里是它时跳过，连模型工厂都不调。
#[tokio::test]
async fn the_idle_tick_skips_the_session_that_is_currently_running() {
    let locks = ExtractLocks::default();
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("正在跑的一轮"))
        .unwrap();
    backdate_session(&f.db, &sid, Db::now_ms() - 60 * 60 * 1_000);

    let settings = Settings::default();
    let running = Arc::new(Mutex::new(Some(sid.clone())));
    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let factories = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = factories.clone();
    let fake = model.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some((fake.clone() as Arc<dyn ModelProvider>, None))
        };

    assert_eq!(
        idle_extract_tick(&f.db, &settings, &running, &factory, &locks).await,
        0
    );
    assert_eq!(
        factories.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "正在跑的会话必须整个跳过（连客户端都不构造）"
    );
    assert!(model.calls().is_empty(), "一次模型调用都不许发生");
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 0);
}

// ==== 并发缺口(spec §12.5 缺口 1):同一会话只许一条抽取路径 ====

/// 锁的三条性质:认领、释放后可用、跨会话不互斥。
/// 第三条是「按会话」这个设计的全部理由:全局锁会让 A 会话的抽取挡住 B 会话。
#[test]
fn the_extract_lock_is_per_session_and_released_on_drop() {
    let locks = ExtractLocks::default();
    let guard = locks.try_acquire("A").expect("第一次认领必须成功");
    assert!(
        locks.try_acquire("A").is_none(),
        "同一会话的第二条路径必须被挡下"
    );
    assert!(
        locks.try_acquire("B").is_some(),
        "不同会话之间没有冲突的理由"
    );
    drop(guard);
    assert!(
        locks.try_acquire("A").is_some(),
        "guard 析构即释放,失败的抽取不该把会话永久钉死"
    );
}

/// 端到端:idle 抽取进行中时,同一会话的另一次抽取必须**放弃**而不是排队,
/// 且游标只推进一次 —— 这正是「两条路径同时抽同一段、堆出重复记忆」那个缺口的反面。
#[tokio::test]
async fn a_concurrent_extraction_of_the_same_session_is_dropped_not_queued() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&sid, &user_says("我在上海工作"))
        .unwrap();
    backdate_session(&f.db, &sid, Db::now_ms() - 60 * 60 * 1_000);

    // 先占住这个会话的抽取权(模拟「另一条路径正在抽」)。
    let locks = ExtractLocks::default();
    let held = locks.try_acquire(&sid).expect("主路径先认领");

    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户在上海工作"))],
        &[],
    );
    let got = extract_session_memories(
        &f.db,
        &sid,
        &*model,
        &MemoryTurnCfg::default(),
        None,
        &locks,
        &CancellationToken::new(),
    )
    .await
    .expect("被放弃不是失败");
    assert_eq!(got, 0, "第二条路径必须安静放弃");
    assert_eq!(
        model.calls().len(),
        0,
        "放弃发生在模型调用之前 —— 否则「放弃」也照样烧钱"
    );
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 0, "游标不许动");

    // 主路径释放之后,再抽就正常。
    drop(held);
    let model2 = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户在上海工作"))],
        &[],
    );
    let got = extract_session_memories(
        &f.db,
        &sid,
        &*model2,
        &MemoryTurnCfg::default(),
        None,
        &locks,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(got, 1);
    assert_eq!(f.db.valid_memories().unwrap().len(), 1);
}

/// 压缩段同样认领:拿不到就跳过抽取、压缩照常(内容归另一条路径,不丢)。
#[tokio::test]
async fn a_compaction_skips_extraction_when_another_path_holds_the_lock() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    seed(&f.db, &sid, &long_history());
    let before = f.db.messages(&sid).unwrap();

    let locks = ExtractLocks::default();
    let held = locks.try_acquire(&sid).expect("占住抽取权");

    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &["好的"],
    );
    let (history, event, saved) = compact_before_turn(
        &f.db,
        &sid,
        &*model,
        &ContextBuilder::new(compacting_cfg()),
        before,
        &MemoryTurnCfg::default(),
        None,
        &locks,
        &CancellationToken::new(),
    )
    .await
    .expect("压缩段不许被锁挡住");

    assert!(event.is_some(), "压缩照常发生");
    assert_eq!(saved, 0, "本轮没有抽取,自然没有「记住了几条」");
    assert_eq!(history.len(), 21, "压缩结果不受影响");
    assert!(
        f.db.valid_memories().unwrap().is_empty(),
        "跳过抽取时不许写半条记忆"
    );
    // 只调了摘要那一次:抽取的模型调用根本没发起。
    assert_eq!(
        model.calls().len(),
        1,
        "跳过的抽取不许烧掉一次模型调用,实际 {:?}",
        model.calls().len()
    );
    drop(held);
}

/// 空闲 tick 的成本(spec §12.5 缺口 3):没有待抽会话时,它只是**一次 SELECT**。
/// 这条给出量级承诺而不是精确值 —— 60 秒一次、亚毫秒量级,才对得起「永远在跑的后台任务」。
/// 100 个会话 × 100 条消息的库上,一次 tick 的守卫部分必须远低于 60ms。
#[tokio::test]
async fn an_idle_tick_with_nothing_to_do_costs_one_cheap_query() {
    let f = Fixture::new();
    // 100 个会话,每个 100 条消息,全部刚活动过(都不该被抽)。
    for s in 0..100 {
        let sid = f.db.create_session(&format!("会话 {s}")).unwrap();
        for i in 0..100 {
            f.db.append_message(&sid, &user_says(&format!("消息 {i}")))
                .unwrap();
        }
    }
    // 上面的夹具要写 100×100 条消息；慢 runner（Windows CI）上这段准备时间会逼近
    // 默认 5 分钟的空闲窗口，于是最早创建的会话在 tick 那一刻「看起来已空闲」，
    // 工厂被调用、断言随机翻车（2026-11-02 在 main 上复现过一次，重跑即绿）。
    // 本用例验证的是「没有待抽会话时一次便宜查询、零工厂调用」，与阈值取值无关，
    // 因此把窗口拉到远大于任何可能的准备时长，消除对机器速度的依赖。
    let settings = Settings {
        memory_idle_extract_minutes: 24 * 60,
        ..Settings::default()
    };
    let running = Arc::new(Mutex::new(None));
    let locks = ExtractLocks::default();
    let factories = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = factories.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        };

    let started = std::time::Instant::now();
    let n = idle_extract_tick(&f.db, &settings, &running, &factory, &locks).await;
    let elapsed = started.elapsed();
    assert_eq!(n, 0, "刚活动过的会话不该被抽");
    assert_eq!(
        factories.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "无待抽会话时连模型工厂都不该被调用"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(60),
        "一次 tick 的守卫部分必须远低于 60ms(实际 {elapsed:?});\
         60 秒一次的常驻任务不该有可感知的开销"
    );
}

#[tokio::test]
async fn the_idle_tick_extracts_an_idle_session_and_advances_its_cursor() {
    let f = Fixture::new();
    let sid = f.db.create_session(DEFAULT_TITLE).unwrap();
    for text in ["我在上海工作", "我最近在学日语"] {
        f.db.append_message(&sid, &user_says(text)).unwrap();
    }
    backdate_session(&f.db, &sid, Db::now_ms() - 60 * 60 * 1_000);

    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户在上海工作"))],
        &[],
    );
    let fake = model.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            Some((fake.clone() as Arc<dyn ModelProvider>, None))
        };

    let n = idle_extract_tick(
        &f.db,
        &Settings::default(),
        &Arc::new(Mutex::new(None)),
        &factory,
        &ExtractLocks::default(),
    )
    .await;
    assert_eq!(n, 1, "一个空闲会话被处理");
    assert_eq!(f.db.memory_cursor(&sid).unwrap(), 2);
    assert!(
        f.db.valid_memories()
            .unwrap()
            .iter()
            .any(|r| r.body == "用户在上海工作"),
        "抽取产物必须落库"
    );
}

/// 多会话按**最后活动升序**处理（store 的 SQL 已排序，这条钉的是 tick 没有
/// 自己再排一遍/乱序取）。用抽取请求里第一段用户文本识别是哪个会话。
#[tokio::test]
async fn the_idle_tick_processes_sessions_in_last_active_order() {
    let f = Fixture::new();
    let now = Db::now_ms();
    let older = f.db.create_session(DEFAULT_TITLE).unwrap();
    let newer = f.db.create_session(DEFAULT_TITLE).unwrap();
    f.db.append_message(&older, &user_says("会话 A 的话"))
        .unwrap();
    f.db.append_message(&newer, &user_says("会话 B 的话"))
        .unwrap();
    backdate_session(&f.db, &older, now - 2 * 60 * 60 * 1_000);
    backdate_session(&f.db, &newer, now - 90 * 60 * 1_000);

    let model = FakeModel::extracting(
        &[&format!("[{}]", fact_candidate("上海", "用户搬到上海"))],
        &[],
    );
    let fake = model.clone();
    // 与 idle_extract_tick 注入形状同一套，收拢为 type alias 属于签名重构，超出机械清理范围。
    #[allow(clippy::type_complexity)]
    let factory =
        move |_s: &Settings| -> Option<(Arc<dyn ModelProvider>, Option<Arc<dyn EmbedProvider>>)> {
            Some((fake.clone() as Arc<dyn ModelProvider>, None))
        };

    let n = idle_extract_tick(
        &f.db,
        &Settings::default(),
        &Arc::new(Mutex::new(None)),
        &factory,
        &ExtractLocks::default(),
    )
    .await;
    assert_eq!(n, 2, "两个空闲会话都处理");
    let seen: Vec<String> = extraction_requests(&model)
        .into_iter()
        .map(|r| r.messages[1].text_of())
        .collect();
    assert_eq!(
        seen,
        vec!["会话 A 的话".to_string(), "会话 B 的话".to_string()],
        "最久没动的先抽"
    );
}

/// 用户手写记忆的 kind 白名单：不认识的值回落 `fact`，**不**在这一层报错。
/// 用户手写的内容比模型的分类意图更可信，拼错一个标签不该让「我想记一件事」失败。
#[test]
fn an_unknown_user_memory_kind_falls_back_to_fact_instead_of_rejecting() {
    for (input, want) in [
        ("fact", "fact"),
        ("preference", "preference"),
        ("prospective", "prospective"),
        (" preference ", "preference"),
        ("entity_rel", "fact"),
        ("Fact", "fact"),
        ("", "fact"),
    ] {
        assert_eq!(normalize_user_memory_kind(input), want, "输入 {input:?}");
    }
}

/// 空闲 worker 的接线判据。`spawn_memory_idle_worker` 在 `run()` 的 setup 闭包里，
/// 单测进不去那个闭包（与 `the_assembly_wires_the_production_registry` 同一类问题），
/// 所以读自己的源码：调用点必须在 `app.manage(AppState {` 之后、窗口几何恢复之前。
/// 只取 `#[cfg(test)]` 之前的部分 —— 否则 `include_str!` 会把本测试里的字符串也算进去。
#[test]
fn the_idle_extract_worker_is_wired_after_manage_and_before_window_restore() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ipc.rs"));
    let production = src
        .split("#[cfg(test)]")
        .next()
        .expect("找不到 #[cfg(test)] 边界");
    let manage_at = production
        .find("app.manage(AppState {")
        .expect("找不到 app.manage 装配段");
    let worker_at = production
        .rfind("spawn_memory_idle_worker(")
        .expect("空闲 worker 写了但没在装配段调用：它永远不会跑");
    let restore_at = production
        .find("restore_window_geometry(app.handle());")
        .expect("找不到窗口几何恢复那一行");
    assert!(
        worker_at > manage_at,
        "worker 必须在 `app.manage` 之后启动（它要的四个 Arc 那时才存在）"
    );
    assert!(
        worker_at < restore_at,
        "worker 的启动点必须在 setup 收尾之前，否则它压根不会被执行"
    );
}
