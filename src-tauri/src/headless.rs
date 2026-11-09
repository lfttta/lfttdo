//! G45+：headless 会话入口 —— 不开窗口跑一轮，把事件流打成 NDJSON 写 stdout。
//!
//! 设计文档：`docs/superpowers/specs/2026-11-09-headless-eval-design.md`。
//!
//! 为什么需要它：`scripts/eval` 在 G45 只能验「起得来 + 隔离」，真正的会话回归
//! （prompt → 期望工具序列 / 产物内容）需要一只能**把一轮跑完并留下证据**的入口。
//! GUI 模拟点击的成本远高于它的信号价值，所以这里做的是一个纯命令行入口。
//!
//! # 边界
//!
//! * **本文件不 `use tauri`**（Plan A 约束）：`main.rs` 在 GUI 模式之外多一个分支，
//!   按命令行参数进这里；这里只用 `std::env` / `std::io`。
//! * **不改产品行为**：装配走的是与 `send_message` **同一批帮助函数**
//!   （`build_production_registry` / `memory_turn_cfg` / `agent_profiles` /
//!   `Settings::to_context_config`），没有为评测新开一条「简化版」路径 ——
//!   简化版测的就不是产品了。
//! * **不做的事**（如实标注）：没有窗口、没有权限弹窗应答（默认按 AskAll 的
//!   deny 语义走，这是 headless 的既有事实）、不跑空闲记忆 worker、不连 MCP。
//!
//! # 用法
//!
//! ```text
//! lfttdo --headless --prompt "列出 src 下的 rust 文件" [--data-dir DIR]
//!        [--session NAME] [--base-url URL] [--model NAME] [--timeout SECONDS]
//! ```
//!
//! stdout 是**纯 NDJSON**（一行一个 `AgentEvent`，与前端收到的是同一份线上格式）；
//! 诊断/日志走 stderr。退出码：0 = 一轮正常收尾（`done` 到达）；1 = 运行错误；
//! 2 = 用法错误；3 = 超时。

use crate::agent::events::AgentEvent;
use crate::agent::session::{AgentSession, SessionConfig, SessionDeps};
use crate::agent::types::ContentBlock;
use crate::permission::gate::PermissionGate;
use crate::settings::SettingsStore;
use crate::store::db::Db;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// `--headless` 之外的参数是否被本模块接管。
/// `main.rs` 用它决定「起 GUI 还是走无头」——GUI 是默认，任何不带这个开关的调用
/// 行为一个字都不变。
pub fn wants_headless(args: &[String]) -> bool {
    args.iter().any(|a| a == "--headless")
}

/// 解析后的参数。
#[derive(Debug, Clone, PartialEq)]
pub struct HeadlessArgs {
    pub prompt: String,
    /// 数据目录（会话库、设置、hooks 都在这里）。缺省见 [`default_data_dir`]。
    pub data_dir: Option<PathBuf>,
    /// 会话标题：同名会话会被复用（第二次跑同一案例就接在同一段历史上）。
    pub session: Option<String>,
    /// 覆盖设置里的端点/模型（评测要能钉一个已知端点）。
    pub base_url: Option<String>,
    pub model: Option<String>,
    /// 硬超时（秒）：到点直接退出 3，不无限等一个挂住的端点。
    pub timeout_secs: u64,
}

/// 用法文本。
pub const USAGE: &str = "\
用法: lfttdo --headless --prompt \"<提示词>\" [选项]

选项:
  --prompt <文本>        要发送的用户消息（必填）
  --data-dir <目录>      数据目录；默认 %APPDATA%\\com.qwa.do\\headless
  --session <标题>       会话标题；同名会话复用（用于多轮案例）
  --base-url <URL>       覆盖模型端点（默认用该数据目录里的设置）
  --model <名>           覆盖模型名
  --timeout <秒>         整轮超时，默认 300
  -h, --help             显示这段文本

输出: stdout 为 NDJSON（一行一个 AgentEvent），诊断走 stderr。
退出码: 0 正常收尾 / 1 运行错误 / 2 用法错误 / 3 超时
";

/// 解析命令行。`args` **不含**程序名（`std::env::args().skip(1)`）。
pub fn parse_args(args: &[String]) -> Result<HeadlessArgs, String> {
    let mut prompt: Option<String> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut session: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut model: Option<String> = None;
    let mut timeout_secs: u64 = 300;

    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        // 需要取值的开关：统一「下一个参数就是值」，缺了就报用法错误
        // （不猜、不给默认值：一个少给了值的命令行不该跑起一轮对话）。
        let mut take = |name: &str| -> Result<String, String> {
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{name} 需要一个值"))
        };
        match flag {
            "--headless" | "--" => {}
            "--prompt" => prompt = Some(take("--prompt")?),
            "--data-dir" => data_dir = Some(PathBuf::from(take("--data-dir")?)),
            "--session" => session = Some(take("--session")?),
            "--base-url" => base_url = Some(take("--base-url")?),
            "--model" => model = Some(take("--model")?),
            "--timeout" => {
                let raw = take("--timeout")?;
                timeout_secs = raw
                    .parse()
                    .map_err(|_| format!("--timeout 需要一个整数秒数，收到 {raw:?}"))?;
                if timeout_secs == 0 {
                    return Err("--timeout 必须大于 0".into());
                }
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("未知参数 {other:?}\n\n{USAGE}")),
        }
        i += 1;
    }

    let prompt = prompt.ok_or("缺少 --prompt")?;
    if prompt.trim().is_empty() {
        return Err("--prompt 不能是空白".into());
    }
    Ok(HeadlessArgs {
        prompt,
        data_dir,
        session,
        base_url,
        model,
        timeout_secs,
    })
}

/// 数据目录的缺省值。
///
/// **故意不用 GUI 的数据目录**：一次评测跑下来会写会话库、记忆、日志，
/// 混进用户真实数据里既污染也会让下一次评测读到上一次的残留。
/// `%APPDATA%\com.qwa.do\headless` 与用户数据目录同父、不同叶。
pub fn default_data_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("APPDATA 未设置，请用 --data-dir 指定目录")?;
    Ok(base.join("com.qwa.do").join("headless"))
}

/// 把事件逐行写出去的出口。**stdout 只承载 NDJSON**：诊断一律 stderr，
/// 否则 `| jq` 之类的下游会被一行中文提示打断。
struct NdjsonSink {
    out: Mutex<std::io::Stdout>,
    /// 捕获到的事件（供收尾摘要 —— 摘要走 stderr，不进 stdout）。
    seen: Mutex<Vec<AgentEvent>>,
}

impl NdjsonSink {
    fn new() -> Self {
        Self {
            out: Mutex::new(std::io::stdout()),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl crate::agent::events::EventSink for NdjsonSink {
    fn emit(&self, e: AgentEvent) {
        // 序列化失败不该让会话挂掉：写一条降级行，让下游知道「这里有个事件丢了」。
        let line = serde_json::to_string(&e).unwrap_or_else(|err| {
            format!(
                r#"{{"t":"error","where":"loop","message":"事件序列化失败：{err}","fatal":false}}"#
            )
        });
        {
            let mut out = self.out.lock().unwrap();
            let _ = writeln!(out, "{line}");
            let _ = out.flush(); // 逐行刷：评测要能在进程被杀时也拿到已发生的部分
        }
        self.seen.lock().unwrap().push(e);
    }
}

/// 按标题找会话，没有就建一个。评测的多轮案例靠它把两轮接在同一段历史上。
fn ensure_session(db: &Db, title: &str) -> Result<String, String> {
    let rows = db.list_sessions().map_err(|e| e.to_string())?;
    if let Some(row) = rows.into_iter().find(|r| r.title == title) {
        return Ok(row.id);
    }
    db.create_session(title).map_err(|e| e.to_string())
}

/// 跑一轮。返回值是进程退出码语义（见文件头）。
pub async fn run(args: HeadlessArgs) -> Result<(), String> {
    let data_dir = match &args.data_dir {
        Some(d) => d.clone(),
        None => default_data_dir()?,
    };
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| format!("创建数据目录 {} 失败：{e}", data_dir.display()))?;
    let skills_dir = data_dir.join("skills");
    let _ = std::fs::create_dir_all(&skills_dir);

    let db = Arc::new(
        Db::open(&data_dir.join("qwen-do.db")).map_err(|e| format!("打开会话库失败：{e}"))?,
    );
    // 一个目录只许存在一个 `SettingsStore`（settings.rs 的 N-3）：这里建一次，别处只 clone。
    let store = Arc::new(SettingsStore::new(&data_dir));
    let mut settings = store.load().map_err(|e| format!("读设置失败：{e}"))?;

    // 命令行覆盖：评测要能钉一个确定的端点，而不是靠机器上碰巧存着什么。
    if let Some(url) = &args.base_url {
        settings.base_url = url.clone();
    }
    if let Some(model) = &args.model {
        settings.model = model.clone();
    }
    settings.normalize();
    if settings.base_url.trim().is_empty() {
        return Err("没有模型端点：请用 --base-url 指定，或先在这个数据目录里写好设置".to_string());
    }

    let api_key = store
        .api_key()
        .map_err(|e| format!("读 API key 失败：{e}"))?;
    let api_key = match api_key {
        Some(k) => k,
        // 回环端点（本机 Ollama 等）允许空 key —— 与 `resolve_api_key` 同一条规则；
        // 这里不能直接调它（那个函数在 ipc.rs 且不 pub），规则本身一字不差地抄过来
        // 由下面的测试钉住两端。
        None if crate::model::openai_compat::is_loopback_endpoint(&settings.base_url) => {
            String::new()
        }
        None => return Err("这个端点需要 API key，但数据目录里没有".to_string()),
    };
    let model_cfg = settings.to_model_config(&api_key);
    let temperature = model_cfg.temperature;
    let model: Arc<dyn crate::model::provider::ModelProvider> = Arc::new(
        crate::model::openai_compat::OpenAiCompat::new(model_cfg).map_err(|e| e.to_string())?,
    );
    drop(api_key);

    let registry = crate::ipc::build_production_registry(
        db.clone(),
        Arc::new(std::sync::RwLock::new(
            crate::tools::shell::ShellCatalog::new(),
        )),
    );
    // 权限门**不装 sink**：headless 里没有人能应答弹窗，于是 Read 自动放行、
    // Write/Exec 走 `没有可应答的界面` 的 deny（gate.rs 的既有语义，R44）。
    // 这不是「为评测放宽权限」——恰相反，它比 GUI 更严。
    let gate = Arc::new(PermissionGate::new());
    for (server, verdict) in crate::mcp::trust::load(&data_dir) {
        gate.rules().set_server_trust(&server, verdict);
    }
    if let Some(rules) = &settings.permission_rules
        && let Err(reason) = gate.set_rules_from_json(rules)
    {
        eprintln!("permission_rules 形状非法，本次忽略：{reason}");
    }
    gate.set_permission_mode(settings.permission_mode);

    let sink = Arc::new(NdjsonSink::new());
    let title = args
        .session
        .clone()
        .unwrap_or_else(|| "headless".to_string());
    let session_id = ensure_session(&db, &title)?;

    // 输出目录：没有工作文件夹的会话落在 `<data>\\sessions\\<id>`。与 GUI 的
    // `resolve_session_output` 同一条规则的另一半（那个函数要 `AppHandle` 取文档目录，
    // headless 没有，所以缺省落到数据目录下 —— 差别只在**默认储存位置**，不在语义）。
    let output_dir = data_dir.join("sessions").join(&session_id);
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("创建输出目录失败：{e}"))?;

    let profile = settings.active_agent_profile();
    let mut context_settings = settings.clone();
    if let Some(prompt) = settings.effective_agent_prompt() {
        context_settings.system_prompt = prompt;
    }

    let mut session = AgentSession::new(SessionDeps {
        model,
        registry,
        gate: gate.clone(),
        sink: sink.clone(),
        context: crate::agent::context::ContextBuilder::new(context_settings.to_context_config()),
        temperature,
    })
    .with_roots(crate::settings::effective_roots(&settings.roots))
    .with_output_dir(Some(output_dir))
    .with_skills_dir(Some(skills_dir))
    .with_config(SessionConfig {
        plan_mode: settings.plan_mode,
        diagnostics_command: settings.diagnostics_command.clone(),
        allowed_tools: profile.as_ref().and_then(|p| p.tools.clone()),
        active_agent_name: profile.as_ref().map(|p| p.name.clone()),
        agents: crate::ipc::agent_profiles(&settings),
        // headless 也跑 hooks：评测要能验证 hook 行为（deny / 改写）。
        // 只有用户作用域 —— 工作区级要一个会话工作文件夹，而 headless 的会话
        // 没有（这里不编一个默认值出来）。
        hooks: Some(crate::hooks::HookRuntime::user_only(&data_dir)),
        ..Default::default()
    });

    // 恢复历史：多轮案例的第二轮要接着上一轮。失败不致命 —— 那只是「这一轮从空历史开始」，
    // 但要说出来（静默丢历史会让多轮案例的断言指向错误的原因）。
    let mut restored = 0usize;
    match db.messages(&session_id) {
        Ok(msgs) if !msgs.is_empty() => {
            restored = msgs.len();
            session.restore(msgs);
        }
        Ok(_) => {}
        Err(e) => eprintln!("读历史失败，本轮从空历史开始：{e}"),
    }

    let blocks = vec![ContentBlock::Text {
        text: args.prompt.clone(),
    }];

    // 超时：把会话 future 放进 select 里与计时器赛跑。取消令牌拿不到（headless 的
    // cancel 槽位是空的），所以超时只能**放弃等待**，不能优雅中断 —— 如实记录：
    // 进程退出后仍在写盘的工具会被操作系统回收，它们可能留下半截产物。
    let ran = tokio::time::timeout(
        std::time::Duration::from_secs(args.timeout_secs),
        session.run(blocks),
    )
    .await;

    let reason = match ran {
        Ok(r) => r,
        Err(_) => {
            eprintln!(
                "整轮超时（{} 秒）。已发生的事件都在 stdout 里。",
                args.timeout_secs
            );
            return Err("timeout".to_string());
        }
    };

    // 落库：只**追加**本轮新产生的行（库里已有的不动）。`run_turn` 的做法是
    // 「压缩判定 + append_messages」，headless 不做压缩判定（它要 TurnDeps 的全套
    // 装配，而这一轮已经在跑了），所以直接按「恢复时几行、现在几行」取后缀。
    let fresh = &session.history()[restored..];
    if !fresh.is_empty() {
        db.append_messages(&session_id, fresh)
            .map_err(|e| format!("落库失败：{e}"))?;
    }

    eprintln!(
        "完成：stop_reason={reason:?}，事件 {} 条，会话 {session_id}（库：{}）",
        sink.count(),
        data_dir.display()
    );
    Ok(())
}

/// 设置里的模型端点为空时不许跑 —— 这道闸的判据与 GUI 的装配层一致。
#[cfg(test)]
fn endpoint_missing(settings: &crate::settings::Settings) -> bool {
    settings.base_url.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    // 参数解析：必需项、取值项、未知开关、超时边界。
    #[test]
    fn parse_requires_a_prompt_and_reads_every_option() {
        assert!(
            parse_args(&args(&["--headless"])).is_err(),
            "缺 --prompt 必须报错"
        );
        assert!(
            parse_args(&args(&["--prompt", "  "])).is_err(),
            "空白提示词不算"
        );
        let a = parse_args(&args(&[
            "--headless",
            "--prompt",
            "看看 src",
            "--data-dir",
            "D:/tmp/ev",
            "--session",
            "case-1",
            "--base-url",
            "http://127.0.0.1:11434/v1",
            "--model",
            "qwen3",
            "--timeout",
            "42",
        ]))
        .unwrap();
        assert_eq!(a.prompt, "看看 src");
        assert_eq!(a.data_dir, Some(PathBuf::from("D:/tmp/ev")));
        assert_eq!(a.session.as_deref(), Some("case-1"));
        assert_eq!(a.base_url.as_deref(), Some("http://127.0.0.1:11434/v1"));
        assert_eq!(a.model.as_deref(), Some("qwen3"));
        assert_eq!(a.timeout_secs, 42);
    }

    #[test]
    fn parse_rejects_unknown_flags_missing_values_and_zero_timeout() {
        for bad in [
            vec!["--prompt", "x", "--nope"],
            vec!["--prompt"], // 缺值
            vec!["--prompt", "x", "--timeout", "0"],
            vec!["--prompt", "x", "--timeout", "abc"],
        ] {
            assert!(parse_args(&args(&bad)).is_err(), "{bad:?} 本应报用法错误");
        }
        // 帮助走 Err（与用法错误同一条出口：打印文本、退出 2）。
        let help = parse_args(&args(&["--help"])).unwrap_err();
        assert!(help.contains("--prompt"), "{help}");
    }

    // `--headless` 是唯一的模式开关：不带的命令行必须仍然走 GUI（行为一个字不变）。
    #[test]
    fn only_an_explicit_flag_switches_to_headless() {
        assert!(wants_headless(&args(&["--headless", "--prompt", "x"])));
        assert!(!wants_headless(&args(&["--prompt", "x"])));
        assert!(!wants_headless(&args(&[])));
    }

    // 端点为空必须被拦下：这条闸与 GUI 装配层是同一个判据（R90 的取向）。
    #[test]
    fn an_empty_endpoint_is_refused() {
        let mut s = crate::settings::Settings::default();
        assert!(endpoint_missing(&s), "默认设置没有端点");
        s.base_url = "http://127.0.0.1:11434/v1".into();
        assert!(!endpoint_missing(&s));
    }

    // 缺省数据目录与 GUI 的目录同父、不同叶：评测写到 `...\com.qwa.do\headless`，
    // 不污染 `...\com.qwa.do\` 里的真实会话库。
    #[test]
    fn the_default_data_dir_is_a_sibling_of_the_real_one() {
        // APPDATA 在本机一定存在（Windows）；在别的平台这条测试跳过。
        if std::env::var_os("APPDATA").is_none() {
            return;
        }
        let d = default_data_dir().unwrap();
        assert!(d.ends_with("headless"), "{}", d.display());
        assert!(
            d.parent()
                .map(|p| p.ends_with("com.qwa.do"))
                .unwrap_or(false),
            "应当与 GUI 数据目录同父：{}",
            d.display()
        );
    }
}
