//! G44：hooks 机制 —— 用户脚本挂进 agent 事件流。
//!
//! 设计文档：`docs/superpowers/specs/2026-11-09-hooks-design.md`。
//!
//! 三层，全部与 Tauri 无关（Plan A 约束）：
//! 1. **声明**（[`Declaration`]）：`hooks.json` 的解析与规范化；
//! 2. **信任**（[`TrustStore`]）：声明内容的 SHA-256 digest 绑定信任 —— 改一个
//!    字符就回到未信任，这是本机制唯一的防线（hook 是任意代码执行）；
//! 3. **执行**（[`run_hook`]）：stdin 喂 JSON、stdout 收 JSON、超时杀进程。
//!
//! 失败取向：**hook 的故障不让对话失败**。超时/坏 JSON/非零退出一律「不拦截 +
//! 诊断文本」；但「本该拦的没拦」必须可见，所以诊断会进通知条而不是只写日志。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// 配置文件（数据目录下）。
pub const HOOKS_FILE: &str = "hooks.json";
/// 信任表（数据目录下）。损坏按空表处理（fail closed）。
pub const TRUST_FILE: &str = "hooks-trust.json";
/// 默认超时。
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
/// 超时的夹取范围。
pub const MIN_TIMEOUT_MS: u64 = 100;
pub const MAX_TIMEOUT_MS: u64 = 60_000;
/// `additional_context` 与 `reason` 的输出上限（字符）。
pub const MAX_CONTEXT_CHARS: usize = 8_000;
pub const MAX_REASON_CHARS: usize = 2_000;
/// 工作区级 hooks 住哪：`<workspace>/.lfttdo/hooks.json`（与 todos/undo 同一个应用态目录）。
pub const WORKSPACE_STATE_DIR: &str = ".lfttdo";

/// hook 声明的来源作用域。
///
/// 两个作用域**不共享信任**：工作区级的 hooks.json 可能是克隆仓库自带的，
/// 它是一份**不可信文件** —— 用户为某个仓库里同名同内容的 hook 授予的信任，
/// 不该自动延伸到另一个仓库。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookScope {
    /// 数据目录（用户自己写的）。
    User,
    /// 会话工作区里的 `.lfttdo/`（可能是克隆来的）。
    Workspace,
}

impl HookScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookScope::User => "user",
            HookScope::Workspace => "workspace",
        }
    }
}

/// 一个 hook 来源：配置目录 + 它在信任表里的键前缀。
///
/// 信任表始终只有**一份**（数据目录里的 `hooks-trust.json`），作用域靠键前缀区分：
/// 用户作用域的前缀是空串（键 = digest 本身，与 v0.2.0 的旧信任表兼容），
/// 工作区作用域是 `ws:<工作区路径摘要>:`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookSource {
    /// hooks.json 所在目录。
    pub config_dir: std::path::PathBuf,
    pub scope: HookScope,
    /// 信任键前缀（见上）。
    pub trust_prefix: String,
}

/// 工作区路径的信任前缀。**按路径绑定**：同一个 hook 在仓库 A 被信任，
/// 不会让仓库 B 里的同内容声明也变成已信任 —— 换了目录就是换了来源。
pub fn workspace_trust_prefix(root: &Path) -> String {
    // canonicalize 失败（目录已消失）时用原样的路径文本：宁可换一个前缀让信任失效，
    // 也不要退化成「所有解析不出的路径共用一个前缀」。
    let key = root
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| root.display().to_string());
    let digest = sha256_hex(key.as_bytes());
    format!("ws:{}:", &digest[..16])
}

/// 一轮要跑的 hook 全集：信任表位置 + 有序来源表。
///
/// 顺序有意义：先用户作用域、后工作区作用域 —— 折叠规则是「`updated_input` 后者
/// 覆盖前者」，所以工作区规则能修正（但不能取消）用户的改写；`deny` 谁都能下。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRuntime {
    /// 信任表所在目录（永远是应用的数据目录）。
    pub trust_dir: std::path::PathBuf,
    pub sources: Vec<HookSource>,
}

impl HookRuntime {
    /// 只跑用户作用域（没有工作区的会话）。
    pub fn user_only(data_dir: &Path) -> Self {
        Self {
            trust_dir: data_dir.to_path_buf(),
            sources: vec![HookSource {
                config_dir: data_dir.to_path_buf(),
                scope: HookScope::User,
                trust_prefix: String::new(),
            }],
        }
    }

    /// 追加工作区作用域（`<workspace>/.lfttdo`）。
    ///
    /// **只登记来源，不等于启用**：工作区来源还必须过信任表里的
    /// `workspace_enabled` 全局开关（默认关）。开关与逐条 digest 信任是两把锁：
    /// 克隆一个仓库不该让它的 hook 因为「文件里写着 enabled」就武装起来。
    pub fn with_workspace(mut self, workspace: &Path) -> Self {
        let dir = workspace.join(WORKSPACE_STATE_DIR);
        self.sources.push(HookSource {
            config_dir: dir,
            scope: HookScope::Workspace,
            trust_prefix: workspace_trust_prefix(workspace),
        });
        self
    }

    pub fn has_workspace(&self) -> bool {
        self.sources.iter().any(|s| s.scope == HookScope::Workspace)
    }
}

/// 四个事件。线上名字是 CamelCase（与 ZCode 同一套叫法，用户可迁移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookEvent {
    SessionStart,
    PreToolUse,
    PostToolUse,
    Stop,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::SessionStart => "SessionStart",
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::Stop => "Stop",
        }
    }
    /// 需要 matcher 的事件只有工具事件；其余忽略 matcher（写了也不报错）。
    fn is_tool_event(&self) -> bool {
        matches!(self, HookEvent::PreToolUse | HookEvent::PostToolUse)
    }
}

/// 一条 hook 声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookDecl {
    /// 工具名匹配：`*`/`?` 通配，`|` 多选；空 = 全部。
    #[serde(default)]
    pub matcher: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl HookDecl {
    fn timeout(&self) -> Duration {
        let ms = self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        Duration::from_millis(ms.clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS))
    }

    /// 规范化后的声明文本 —— digest 的输入。**只含**影响执行语义的字段，
    /// 按固定顺序拼接：键序与空白差异不该改变 digest（用户重新格式化 JSON
    /// 不该让信任失效），但任何一个字段变化都必须改变它。
    fn canonical(&self, event: HookEvent) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            event.as_str(),
            self.matcher.trim(),
            self.command.trim(),
            self.args.join("\u{1f}"),
            self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
        )
    }

    /// 信任键：规范化声明的 SHA-256（十六进制）。
    pub fn digest(&self, event: HookEvent) -> String {
        sha256_hex(self.canonical(event).as_bytes())
    }

    /// matcher 判定。空 matcher = 全部命中；`|` 分隔多选，逐项通配。
    pub fn matches_tool(&self, event: HookEvent, tool: &str) -> bool {
        if !event.is_tool_event() {
            return true;
        }
        let m = self.matcher.trim();
        if m.is_empty() || m == "*" {
            return true;
        }
        m.split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .any(|p| crate::tools::grep::wildcard_match(tool, p))
    }
}

/// `hooks.json` 的顶层形状。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookConfig {
    #[serde(default)]
    pub enabled: bool,
    /// 事件名 ⇒ 声明列表。
    #[serde(default)]
    pub events: std::collections::BTreeMap<String, Vec<HookDecl>>,
}

impl HookConfig {
    /// 取某个事件的声明（事件名大小写不敏感，容错用户手写）。
    pub fn decls(&self, event: HookEvent) -> Vec<HookDecl> {
        let want = event.as_str().to_ascii_lowercase();
        self.events
            .iter()
            .find(|(k, _)| k.trim().to_ascii_lowercase() == want)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

/// 读配置。缺文件/坏 JSON 都按「关闭」处理 —— 一份写坏的 hooks.json 不该
/// 让应用起不来，也不该让「没配过」与「配错了」表现不同（都没有 hook 跑）。
pub fn load_config(data_dir: &Path) -> HookConfig {
    let Ok(raw) = std::fs::read_to_string(data_dir.join(HOOKS_FILE)) else {
        return HookConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// 信任表：digest 的集合。损坏/缺失按空表（fail closed）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default)]
    pub trusted: Vec<String>,
    /// 工作区级 hooks 的总开关。**默认关**：克隆来的仓库自带 hooks 是本机制里
    /// 最高风险的输入面，必须由用户在面板里显式打开才可能生效；打开后仍然要求
    /// 逐条 digest 信任（两把锁，缺一不可）。
    #[serde(default)]
    pub workspace_enabled: bool,
}

impl TrustStore {
    pub fn load(data_dir: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(data_dir.join(TRUST_FILE)) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(data_dir.join(TRUST_FILE), text).map_err(|e| e.to_string())
    }

    pub fn is_trusted(&self, digest: &str) -> bool {
        self.trusted.iter().any(|d| d == digest)
    }

    pub fn grant(&mut self, digest: &str) {
        if !self.is_trusted(digest) {
            self.trusted.push(digest.to_string());
            self.trusted.sort();
        }
    }

    pub fn revoke(&mut self, digest: &str) {
        self.trusted.retain(|d| d != digest);
    }
}

/// hook 的输入（喂给脚本 stdin 的 JSON）。
#[derive(Debug, Clone, Serialize)]
pub struct HookInput<'a> {
    pub event: &'a str,
    pub session_id: &'a str,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolPayload<'a>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolPayload<'a> {
    pub name: &'a str,
    pub input: &'a Value,
}

/// 脚本的输出（宽容解析：字段缺失都是 None，类型不对整条当解析失败）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HookOutput {
    pub deny: bool,
    pub reason: Option<String>,
    pub additional_context: Option<String>,
    /// G44+ `updated_input`：hook 改写的工具入参（只对 `PreToolUse` 有意义）。
    ///
    /// **改写之后权限门看的是改写后的入参**（`session.rs` 重建 `ToolCallView`）：
    /// 若门还按原入参判定，一条 hook 就能把「批准过的 `ls`」换成任意命令执行。
    /// 这是本字段唯一不可让步的语义。
    ///
    /// 只收 JSON **对象**（工具入参的形状）；数组/字符串等一律按协议错误处理
    /// （诊断，不拦）—— 「改写入参」的形状错了却照跑，比不改更危险。
    pub updated_input: Option<Value>,
}

/// 执行结果。
#[derive(Debug, Clone, PartialEq)]
pub enum HookOutcome {
    /// 正常返回（可能带 deny / 上下文）。
    Done(HookOutput),
    /// 不拦截，但要说一句为什么（超时/坏 JSON/非零退出）。文本面向用户。
    Diagnosed(String),
}

/// 跑一条 hook：分参数执行（不经 shell）、stdin 喂 JSON、stdout 收 JSON、
/// 超时杀进程。**同步**语义 —— 能拦截的前提就是等它。
pub fn run_hook(decl: &HookDecl, _event: HookEvent, input: &HookInput) -> HookOutcome {
    let mut child = match Command::new(&decl.command)
        .args(&decl.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return HookOutcome::Diagnosed(format!("hook 启动失败（{}）：{e}", decl.command));
        }
    };
    // 超时回收整棵进程树（与 experiment 的债①同一手法）。
    let job = crate::mcp::process::create_kill_job().ok();
    if let Some(job) = &job {
        let _ = job.assign(child.id());
    }
    if let Some(stdin) = child.stdin.as_mut() {
        let payload = serde_json::to_vec(input).unwrap_or_else(|_| b"{}".to_vec());
        let _ = stdin.write_all(&payload);
    }
    drop(child.stdin.take());

    // 读输出与等待都有超时：用轮询而不是平台 select，保持这份实现可移植
    // （Windows 上没有 SIGALRM）。
    let deadline = std::time::Instant::now() + decl.timeout();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(job);
                    return HookOutcome::Diagnosed(format!(
                        "hook 超时（{}），已跳过（{}）",
                        decl.timeout().as_millis(),
                        decl.command
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return HookOutcome::Diagnosed(format!("等待 hook 退出失败：{e}"));
            }
        }
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return HookOutcome::Diagnosed(format!("读取 hook 输出失败：{e}")),
    };
    drop(job);
    if !out.status.success() {
        let tail = String::from_utf8_lossy(&out.stderr);
        let tail = crate::text::truncate_display(tail.trim(), 300);
        return HookOutcome::Diagnosed(format!(
            "hook 退出码非 0（{}）{}{}",
            out.status.code().unwrap_or(-1),
            decl.command,
            if tail.is_empty() {
                String::new()
            } else {
                format!("：{tail}")
            }
        ));
    }
    match parse_output(&String::from_utf8_lossy(&out.stdout)) {
        Ok(o) => HookOutcome::Done(o),
        Err(reason) => HookOutcome::Diagnosed(format!("hook 输出不是合法协议：{reason}")),
    }
}

/// 解析 stdout。**空输出 = 无动作**（脚本只想记日志是常见用法）；
/// 非空但不是对象/类型不对 = 解析失败（诊断，不拦）。
pub fn parse_output(stdout: &str) -> Result<HookOutput, String> {
    let text = stdout.trim();
    if text.is_empty() {
        return Ok(HookOutput::default());
    }
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("顶层必须是对象")?;
    let decision = match obj.get("decision") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.trim().to_ascii_lowercase()),
        Some(_) => return Err("decision 必须是字符串".into()),
    };
    let deny = match decision.as_deref() {
        Some("deny") => true,
        Some("allow") | None => false,
        Some(other) => return Err(format!("未知 decision：{other}")),
    };
    let reason = match obj.get("reason") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.trim().is_empty() => {
            Some(crate::text::truncate_display(s.trim(), MAX_REASON_CHARS))
        }
        Some(Value::String(_)) => None,
        Some(_) => return Err("reason 必须是字符串".into()),
    };
    let additional_context = match obj.get("additional_context") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.trim().is_empty() => {
            Some(crate::text::truncate_display(s.trim(), MAX_CONTEXT_CHARS))
        }
        Some(Value::String(_)) => None,
        Some(_) => return Err("additional_context 必须是字符串".into()),
    };
    // G44+ updatedInput：只接受对象（工具入参的形状）。`null` = 明确不改写。
    let updated_input = match obj.get("updated_input") {
        None | Some(Value::Null) => None,
        Some(v) if v.is_object() => Some(v.clone()),
        Some(_) => return Err("updated_input 必须是 JSON 对象".into()),
    };
    Ok(HookOutput {
        deny,
        reason,
        additional_context,
        updated_input,
    })
}

/// 跑某个事件下**所有**已信任且 matcher 命中的 hook，折叠成一次结果。
///
/// 折叠规则：
/// * 任一 deny ⇒ 整体 deny（理由按声明顺序拼接）—— deny 是终局，别的 hook 改写
///   入参也不会让这次调用跑起来；
/// * 上下文按声明顺序拼接；
/// * `updated_input` **后来者覆盖前者**（来源顺序 + 声明顺序里最后一条说了算）。
///   折叠成一份而不是逐条应用：多条 hook 各自基于「原始入参」改写时，串行叠加的
///   语义是「谁先谁赢一半」，用户无从预期；取最后一条至少是确定且可解释的。
/// * 诊断文本累积（用户要能看到哪些 hook 坏了）。
///
/// 未信任的 hook 不进结果 —— 信任状态由调用方在面板里展示，这里不重复提示。
///
/// **工作区来源的两把锁**（见 [`HookRuntime`]）：`workspace_enabled` 未打开时，
/// 工作区来源整段跳过（连声明都不读）；打开后仍逐条按 `trust_prefix + digest` 查表。
pub fn run_event(
    runtime: &HookRuntime,
    event: HookEvent,
    tool: Option<&str>,
    input: &HookInput,
) -> (HookOutput, Vec<String>) {
    // 一次性读信任表：多个来源共享同一份，中途改文件不该让同一次调用看到两套信任。
    let trust = TrustStore::load(&runtime.trust_dir);
    let mut acc = HookOutput::default();
    let mut diags = Vec::new();
    let mut contexts: Vec<String> = Vec::new();
    for source in &runtime.sources {
        if source.scope == HookScope::Workspace && !trust.workspace_enabled {
            continue; // 总开关关闭：工作区来源完全不可见
        }
        let cfg = load_config(&source.config_dir);
        if !cfg.enabled {
            continue;
        }
        for decl in cfg.decls(event) {
            if let Some(tool) = tool
                && !decl.matches_tool(event, tool)
            {
                continue;
            }
            let key = format!("{}{}", source.trust_prefix, decl.digest(event));
            if !trust.is_trusted(&key) {
                continue; // 未信任：不执行（信任与否由面板展示）
            }
            match run_hook(&decl, event, input) {
                HookOutcome::Done(o) => {
                    if o.deny {
                        acc.deny = true;
                        if let Some(r) = o.reason {
                            acc.reason = Some(match acc.reason.take() {
                                Some(prev) => format!("{prev}；{r}"),
                                None => r,
                            });
                        }
                    }
                    if let Some(c) = o.additional_context {
                        contexts.push(c);
                    }
                    // 只有 PreToolUse 的改写有意义（别的时机工具已经跑完/还没被调用）。
                    // deny 的调用根本不会执行，改写值留着也无害（调用方在 deny 分支直接返回）。
                    if event == HookEvent::PreToolUse
                        && let Some(v) = o.updated_input
                    {
                        acc.updated_input = Some(v);
                    }
                }
                HookOutcome::Diagnosed(d) => diags.push(d),
            }
        }
    }
    if !contexts.is_empty() {
        acc.additional_context = Some(crate::text::truncate_display(
            &contexts.join("\n"),
            MAX_CONTEXT_CHARS,
        ));
    }
    (acc, diags)
}

/// 会话工作目录（hook 的 `cwd` 字段）。取不到就用进程当前目录。
pub fn cwd_string() -> String {
    std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

// ─────────────────────────── SHA-256（内联，零依赖） ───────────────────────────

/// SHA-256 的内联实现。**为什么不用 crates.io 的 sha2**：本仓库把安装包体积
/// 与依赖树当验收线（见 Cargo.toml 头注释），而这里只需要一个固定算法、
/// 一份输入、一个摘要 —— 几十行的标准实现比新增一棵依赖树划算。
/// 用 FIPS 180-4 的标准测试向量钉住（下面 `sha256_known_vectors`）。
///
/// **不能用弱哈希**：信任 digest 的威胁模型是「用户审过的脚本后来被换了」，
/// 可构造碰撞的哈希（FNV 等）等于信任可伪造。
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // `chunks` 而不是 `chunks_exact`：clippy 对「常量块长」建议后者，但这里的填充
    // 已保证长度是 64 的倍数（上面那个 while 循环），`chunks` 语义等价且不触发
    // 「可能丢尾部」的误报。窗口内按定长下标取，不会越界。
    for chunk in msg.chunks(64) {
        if chunk.len() != 64 {
            continue; // 不可达（填充保证）；防御性跳过而不是 panic
        }
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn decl(command: &str) -> HookDecl {
        HookDecl {
            matcher: "run_command".into(),
            command: command.into(),
            args: vec![],
            timeout_ms: None,
        }
    }

    // FIPS 180-4 的标准向量：内联 SHA-256 必须与公开摘要逐字一致。
    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    // digest 对语义字段敏感、对键序/空白不敏感。
    #[test]
    fn digest_tracks_content_but_ignores_formatting() {
        let a = HookDecl {
            matcher: " run_command ".into(),
            command: " node ".into(),
            args: vec!["a.js".into()],
            timeout_ms: Some(1000),
        };
        let b = HookDecl {
            matcher: "run_command".into(),
            command: "node".into(),
            args: vec!["a.js".into()],
            timeout_ms: Some(1000),
        };
        assert_eq!(
            a.digest(HookEvent::PreToolUse),
            b.digest(HookEvent::PreToolUse),
            "空白差异不该让信任失效"
        );
        let mut c = b.clone();
        c.args = vec!["b.js".into()];
        assert_ne!(
            b.digest(HookEvent::PreToolUse),
            c.digest(HookEvent::PreToolUse),
            "参数变了 digest 必须变"
        );
        // 同一份声明挂在不同事件下是不同的信任单元。
        assert_ne!(
            b.digest(HookEvent::PreToolUse),
            b.digest(HookEvent::PostToolUse)
        );
    }

    // 信任表：损坏 = 空表（fail closed）；grant/revoke 幂等。
    #[test]
    fn a_corrupt_trust_store_reads_as_empty() {
        let d = tmp();
        std::fs::write(d.path().join(TRUST_FILE), "{not json").unwrap();
        let store = TrustStore::load(d.path());
        assert!(store.trusted.is_empty(), "损坏的信任表必须 fail closed");
        assert!(!store.is_trusted("anything"));
    }

    #[test]
    fn trust_grant_and_revoke_round_trip() {
        let d = tmp();
        let mut s = TrustStore::default();
        s.grant("aaa");
        s.grant("aaa");
        assert_eq!(s.trusted, vec!["aaa".to_string()], "重复授予不重复记");
        s.save(d.path()).unwrap();
        assert!(TrustStore::load(d.path()).is_trusted("aaa"));
        s.revoke("aaa");
        assert!(!s.is_trusted("aaa"));
    }

    // matcher：通配、多选、空 = 全部；非工具事件忽略 matcher。
    #[test]
    fn matcher_covers_wildcards_alternation_and_empty() {
        let mut d = decl("x");
        assert!(d.matches_tool(HookEvent::PreToolUse, "run_command"));
        assert!(!d.matches_tool(HookEvent::PreToolUse, "write_file"));
        d.matcher = "run_*|write_file".into();
        assert!(d.matches_tool(HookEvent::PreToolUse, "run_command"));
        assert!(d.matches_tool(HookEvent::PreToolUse, "write_file"));
        assert!(!d.matches_tool(HookEvent::PreToolUse, "read_file"));
        d.matcher = String::new();
        assert!(d.matches_tool(HookEvent::PreToolUse, "anything"));
        // SessionStart 没有工具：matcher 一律忽略。
        d.matcher = "run_command".into();
        assert!(d.matches_tool(HookEvent::SessionStart, "whatever"));
    }

    // 输出解析：空 = 无动作；合法 deny；坏 JSON/错类型 = Err。
    #[test]
    fn output_parsing_is_strict_about_shape_but_tolerates_empty() {
        assert_eq!(parse_output("").unwrap(), HookOutput::default());
        assert_eq!(parse_output("  \n ").unwrap(), HookOutput::default());
        let o = parse_output(r#"{"decision":"deny","reason":"禁止"}"#).unwrap();
        assert!(o.deny);
        assert_eq!(o.reason.as_deref(), Some("禁止"));
        let o = parse_output(r#"{"additional_context":"用 pnpm"}"#).unwrap();
        assert!(!o.deny);
        assert_eq!(o.additional_context.as_deref(), Some("用 pnpm"));
        assert!(parse_output("not json").is_err());
        assert!(parse_output(r#"["array"]"#).is_err());
        assert!(parse_output(r#"{"decision":42}"#).is_err());
        assert!(parse_output(r#"{"decision":"maybe"}"#).is_err());
        assert!(parse_output(r#"{"reason":42}"#).is_err());
    }

    // G44+ updatedInput：只收对象；`null` = 不改写；非对象是协议错误（不是静默忽略）。
    // 「形状错了却照跑」在改写入参这件事上比不改更危险 —— 用户以为限制生效了。
    #[test]
    fn updated_input_takes_objects_and_rejects_other_json_shapes() {
        let o = parse_output(r#"{"updated_input":{"command":"ls -la"}}"#).unwrap();
        assert_eq!(
            o.updated_input,
            Some(serde_json::json!({"command": "ls -la"}))
        );
        assert_eq!(
            parse_output(r#"{"updated_input":null}"#)
                .unwrap()
                .updated_input,
            None
        );
        assert!(parse_output(r#"{"updated_input":"ls"}"#).is_err());
        assert!(parse_output(r#"{"updated_input":[1,2]}"#).is_err());
        assert!(parse_output(r#"{"updated_input":42}"#).is_err());
        // 空对象是合法改写（把入参清空到工具自己报缺参），不是「没有改写」。
        assert_eq!(
            parse_output(r#"{"updated_input":{}}"#)
                .unwrap()
                .updated_input,
            Some(serde_json::json!({}))
        );
    }

    // 端到端（无网络、无外部依赖）：真跑一个平台命令当 hook。
    /// 写一份 .cmd 脚本把给定 JSON 打到 stdout —— 比 `cmd /C echo {...}` 可靠：
    /// echo 对 `"` 的处理随 shell 版本变，转义写不对时测试红的是夹具不是被测代码。
    #[cfg(windows)]
    fn script_cmd(dir: &Path, json: &str) -> HookDecl {
        let path = dir.join("hook.cmd");
        std::fs::write(&path, format!("@echo off\r\necho {json}\r\n")).unwrap();
        HookDecl {
            matcher: "run_command".into(),
            command: "cmd".into(),
            args: vec!["/C".into(), path.display().to_string()],
            timeout_ms: Some(5_000),
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_real_hook_can_deny() {
        let dir = tmp();
        let d = script_cmd(dir.path(), r#"{"decision":"deny","reason":"team-rule"}"#);
        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({"command": "git push"}),
            }),
        };
        match run_hook(&d, HookEvent::PreToolUse, &input) {
            HookOutcome::Done(o) => {
                assert!(o.deny, "合法 deny 必须生效");
                assert_eq!(o.reason.as_deref(), Some("team-rule"));
            }
            other => panic!("期望 Done，实际 {other:?}"),
        }
    }

    // 非零退出 = 诊断（不拦）；坏 JSON = 诊断（不拦）。
    #[cfg(windows)]
    #[test]
    fn a_failing_hook_diagnoses_instead_of_blocking() {
        let bad_exit = HookDecl {
            matcher: String::new(),
            command: "cmd".into(),
            args: vec!["/C".into(), "exit 3".into()],
            timeout_ms: Some(5_000),
        };
        let input = HookInput {
            event: "PostToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: None,
        };
        match run_hook(&bad_exit, HookEvent::PostToolUse, &input) {
            HookOutcome::Diagnosed(d) => assert!(d.contains("退出码"), "{d}"),
            other => panic!("期望诊断，实际 {other:?}"),
        }
        let dir2 = tmp();
        let bad_json = script_cmd(dir2.path(), "not json at all");
        match run_hook(&bad_json, HookEvent::PreToolUse, &input) {
            HookOutcome::Diagnosed(d) => assert!(d.contains("协议"), "{d}"),
            other => panic!("期望诊断，实际 {other:?}"),
        }
    }

    // 未信任的 hook 不执行：声明存在、trust 为空 ⇒ 零动作。
    #[cfg(windows)]
    #[test]
    fn an_untrusted_hook_never_runs() {
        let d = tmp();
        // 脚本真的会 deny —— 但信任表是空的，所以它绝不能被执行。
        let script = script_cmd(d.path(), r#"{"decision":"deny"}"#);
        let cfg = serde_json::json!({
            "enabled": true,
            "events": { "PreToolUse": [{
                "matcher": "*", "command": script.command, "args": script.args
            }]}
        });
        std::fs::write(d.path().join(HOOKS_FILE), cfg.to_string()).unwrap();
        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        let (out, diags) = run_event(
            &HookRuntime::user_only(d.path()),
            HookEvent::PreToolUse,
            Some("run_command"),
            &input,
        );
        assert!(!out.deny, "未信任的 hook 不许拦");
        assert!(diags.is_empty());
    }

    // 信任之后同一条 hook 生效。
    #[cfg(windows)]
    #[test]
    fn a_trusted_hook_runs_and_denies() {
        let d = tmp();
        let script = script_cmd(d.path(), r#"{"decision":"deny","reason":"x"}"#);
        let cfg = serde_json::json!({
            "enabled": true,
            "events": { "PreToolUse": [{
                "matcher": "run_command", "command": script.command, "args": script.args
            }]}
        });
        std::fs::write(d.path().join(HOOKS_FILE), cfg.to_string()).unwrap();
        let loaded = load_config(d.path());
        let decl = loaded.decls(HookEvent::PreToolUse)[0].clone();
        let mut trust = TrustStore::default();
        trust.grant(&decl.digest(HookEvent::PreToolUse));
        trust.save(d.path()).unwrap();

        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        let (out, _) = run_event(
            &HookRuntime::user_only(d.path()),
            HookEvent::PreToolUse,
            Some("run_command"),
            &input,
        );
        assert!(out.deny, "已信任的 deny 必须生效");
    }

    // enabled=false 时什么都不跑。
    #[test]
    fn a_disabled_config_runs_nothing() {
        let d = tmp();
        std::fs::write(
            d.path().join(HOOKS_FILE),
            r#"{"enabled":false,"events":{"PreToolUse":[{"command":"x"}]}}"#,
        )
        .unwrap();
        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: None,
        };
        let (out, diags) = run_event(
            &HookRuntime::user_only(d.path()),
            HookEvent::PreToolUse,
            None,
            &input,
        );
        assert_eq!(out, HookOutput::default());
        assert!(diags.is_empty());
    }

    /// 把一条声明写进配置并授予信任（`run_event` 的两道闸都过）。
    /// `event` 同时决定「写进哪个事件」与「按哪个事件算 digest」—— 两者混用会让
    /// digest 对不上（信任表里的键与运行时查的键不是同一个）。
    #[cfg(windows)]
    fn trusted_decl(dir: &Path, event: HookEvent, script: HookDecl) {
        let cfg = serde_json::json!({
            "enabled": true,
            "events": { event.as_str(): [{
                "matcher": "*", "command": script.command, "args": script.args
            }]}
        });
        std::fs::write(dir.join(HOOKS_FILE), cfg.to_string()).unwrap();
        let loaded = load_config(dir);
        let decl = loaded.decls(event)[0].clone();
        let mut trust = TrustStore::default();
        trust.grant(&decl.digest(event));
        trust.save(dir).unwrap();
    }

    // G44+ updatedInput 折叠：**声明顺序里最后一条说了算**（不是合并、不是第一条赢）。
    // 多条 hook 各自基于「原始入参」改写时，串行叠加的语义是「谁先谁赢一半」，
    // 用户无从预期；取最后一条至少是确定且可解释的。
    #[cfg(windows)]
    #[test]
    fn updated_input_is_last_writer_wins() {
        let d = tmp();
        let first = script_cmd(d.path(), r#"{"updated_input":{"n":1}}"#);
        let mut second = first.clone();
        // 同一个 .cmd 文件，但声明不同（args 不同 ⇒ digest 不同 ⇒ 是两条独立 hook）。
        // 用第二个脚本文件承载不同的输出。
        let second_path = d.path().join("hook2.cmd");
        std::fs::write(
            &second_path,
            "@echo off\r\necho {\"updated_input\":{\"n\":2}}\r\n",
        )
        .unwrap();
        second.args = vec!["/C".into(), second_path.display().to_string()];

        let cfg = serde_json::json!({
            "enabled": true,
            "events": { "PreToolUse": [
                { "matcher": "*", "command": first.command, "args": first.args },
                { "matcher": "*", "command": second.command, "args": second.args }
            ]}
        });
        std::fs::write(d.path().join(HOOKS_FILE), cfg.to_string()).unwrap();
        let loaded = load_config(d.path());
        let decls = loaded.decls(HookEvent::PreToolUse);
        assert_eq!(decls.len(), 2);
        let mut trust = TrustStore::default();
        for dl in &decls {
            trust.grant(&dl.digest(HookEvent::PreToolUse));
        }
        trust.save(d.path()).unwrap();

        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "echo",
                input: &serde_json::json!({"text": "原始"}),
            }),
        };
        let (out, diags) = run_event(
            &HookRuntime::user_only(d.path()),
            HookEvent::PreToolUse,
            Some("echo"),
            &input,
        );
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(
            out.updated_input,
            Some(serde_json::json!({"n": 2})),
            "应当取声明顺序里最后一条的改写"
        );
    }

    // G44+：改写只对 PreToolUse 有意义 —— 别的时机工具已经跑完（PostToolUse）或还没
    // 被调用（SessionStart/Stop），收下改写值会让调用方以为发生了什么，其实什么也没发生。
    #[cfg(windows)]
    #[test]
    fn updated_input_is_ignored_outside_pre_tool_use() {
        let d = tmp();
        let script = script_cmd(d.path(), r#"{"updated_input":{"n":1}}"#);
        trusted_decl(d.path(), HookEvent::PostToolUse, script);
        let input = HookInput {
            event: "PostToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        let (out, _) = run_event(
            &HookRuntime::user_only(d.path()),
            HookEvent::PostToolUse,
            Some("run_command"),
            &input,
        );
        assert_eq!(out.updated_input, None, "PostToolUse 的改写必须被丢弃");
    }

    // ───────────────────── G44+ 工作区级 hooks：两把锁 ─────────────────────

    /// 在工作区里写一份 hooks.json + 一条会 deny 的脚本。
    #[cfg(windows)]
    fn workspace_fixture(workspace: &Path) -> HookDecl {
        let state_dir = workspace.join(WORKSPACE_STATE_DIR);
        std::fs::create_dir_all(&state_dir).unwrap();
        let script = script_cmd(&state_dir, r#"{"decision":"deny","reason":"ws-rule"}"#);
        let cfg = serde_json::json!({
            "enabled": true,
            "events": { "PreToolUse": [{
                "matcher": "run_command", "command": script.command, "args": script.args
            }]}
        });
        std::fs::write(state_dir.join(HOOKS_FILE), cfg.to_string()).unwrap();
        script
    }

    /// 第一把锁：`workspace_enabled` 默认关 —— 工作区配置里写着 `"enabled": true`
    /// 也**绝不能**跑。克隆一个仓库就等于把它的脚本执行权交给了仓库作者，
    /// 这是本机制里最高风险的输入面。
    #[cfg(windows)]
    #[test]
    fn a_workspace_hook_never_runs_until_the_master_switch_is_on() {
        let data = tmp();
        let ws = tmp();
        workspace_fixture(ws.path());
        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        let rt = HookRuntime::user_only(data.path()).with_workspace(ws.path());
        // 即便把每条 digest 都信任了，总开关不开就一条都不跑 —— 两把锁缺一不可。
        let cfg = load_config(&ws.path().join(WORKSPACE_STATE_DIR));
        let decl = cfg.decls(HookEvent::PreToolUse)[0].clone();
        let mut trust = TrustStore::default();
        let prefix = workspace_trust_prefix(ws.path());
        trust.grant(&format!("{prefix}{}", decl.digest(HookEvent::PreToolUse)));
        trust.save(data.path()).unwrap();

        let (out, diags) = run_event(&rt, HookEvent::PreToolUse, Some("run_command"), &input);
        assert!(!out.deny, "总开关关着时工作区 hook 绝不许跑");
        assert!(diags.is_empty());
    }

    /// 第二把锁：总开关打开之后，仍然要逐条 digest 信任。仓库作者改一行脚本，
    /// 信任就失效（与用户作用域同一条规则）。
    #[cfg(windows)]
    #[test]
    fn a_workspace_hook_still_needs_per_declaration_trust() {
        let data = tmp();
        let ws = tmp();
        workspace_fixture(ws.path());
        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        let rt = HookRuntime::user_only(data.path()).with_workspace(ws.path());

        // 只打开总开关，不信任任何声明。
        let trust = TrustStore {
            workspace_enabled: true,
            ..Default::default()
        };
        trust.save(data.path()).unwrap();
        let (out, _) = run_event(&rt, HookEvent::PreToolUse, Some("run_command"), &input);
        assert!(!out.deny, "总开关开了但没逐条信任 ⇒ 不跑");

        // 信任之后才生效 —— 且信任键带工作区前缀。
        let cfg = load_config(&ws.path().join(WORKSPACE_STATE_DIR));
        let decl = cfg.decls(HookEvent::PreToolUse)[0].clone();
        let prefix = workspace_trust_prefix(ws.path());
        let mut trust = TrustStore::load(data.path());
        trust.grant(&format!("{prefix}{}", decl.digest(HookEvent::PreToolUse)));
        trust.save(data.path()).unwrap();
        let (out, diags) = run_event(&rt, HookEvent::PreToolUse, Some("run_command"), &input);
        assert!(out.deny, "两把锁都开了才生效：{diags:?}");
        assert_eq!(out.reason.as_deref(), Some("ws-rule"));
    }

    /// 信任按**路径**绑定：仓库 A 里信任过的同内容声明，不会让仓库 B 里那份也变已信任。
    /// 这是「克隆来的代码不可信」原则在信任键上的落点。
    #[cfg(windows)]
    #[test]
    fn workspace_trust_does_not_leak_across_directories() {
        let data = tmp();
        let a = tmp();
        let b = tmp();
        workspace_fixture(a.path());
        workspace_fixture(b.path());
        assert_ne!(
            workspace_trust_prefix(a.path()),
            workspace_trust_prefix(b.path()),
            "不同路径必须给出不同的信任前缀"
        );

        // 在 A 里信任（总开关也打开）。
        let cfg = load_config(&a.path().join(WORKSPACE_STATE_DIR));
        let decl = cfg.decls(HookEvent::PreToolUse)[0].clone();
        let trust = TrustStore {
            trusted: vec![format!(
                "{}{}",
                workspace_trust_prefix(a.path()),
                decl.digest(HookEvent::PreToolUse)
            )],
            workspace_enabled: true,
        };
        trust.save(data.path()).unwrap();

        let input = HookInput {
            event: "PreToolUse",
            session_id: "s",
            cwd: cwd_string(),
            tool: Some(ToolPayload {
                name: "run_command",
                input: &serde_json::json!({}),
            }),
        };
        // A 跑得起来。
        let rt_a = HookRuntime::user_only(data.path()).with_workspace(a.path());
        assert!(
            run_event(&rt_a, HookEvent::PreToolUse, Some("run_command"), &input)
                .0
                .deny,
            "A 里已信任"
        );
        // B 里同内容的声明**不**已信任：换个目录就是换个来源。
        let rt_b = HookRuntime::user_only(data.path()).with_workspace(b.path());
        assert!(
            !run_event(&rt_b, HookEvent::PreToolUse, Some("run_command"), &input)
                .0
                .deny,
            "B 里的同内容声明不该继承 A 的信任"
        );
    }

    /// 用户作用域的信任键**不带前缀**（与 v0.2.0 的旧信任表兼容：老用户的信任不失效）。
    #[test]
    fn the_user_scope_trust_key_is_the_bare_digest() {
        let rt = HookRuntime::user_only(std::path::Path::new("C:/data"));
        assert_eq!(rt.sources.len(), 1);
        assert_eq!(rt.sources[0].scope, HookScope::User);
        assert_eq!(
            rt.sources[0].trust_prefix, "",
            "旧信任表里的裸 digest 必须继续有效"
        );
        assert!(!rt.has_workspace());
        assert!(
            rt.with_workspace(std::path::Path::new("D:/repo"))
                .has_workspace()
        );
    }

    /// 旧版信任表（只有 `trusted` 数组、没有 `workspace_enabled`）必须照常读出来，
    /// 且新字段落到默认值 false —— 升级不该把用户已有的信任清零，也不该顺手打开工作区。
    #[test]
    fn an_old_trust_file_still_loads_with_the_workspace_switch_off() {
        let d = tmp();
        std::fs::write(d.path().join(TRUST_FILE), r#"{"trusted":["abc","def"]}"#).unwrap();
        let store = TrustStore::load(d.path());
        assert_eq!(store.trusted, vec!["abc".to_string(), "def".to_string()]);
        assert!(!store.workspace_enabled, "缺字段时工作区总开关必须是关的");
    }
}
