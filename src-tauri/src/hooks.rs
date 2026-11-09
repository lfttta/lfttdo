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
    Ok(HookOutput {
        deny,
        reason,
        additional_context,
    })
}

/// 跑某个事件下**所有**已信任且 matcher 命中的 hook，折叠成一次结果。
///
/// 折叠规则：任一 deny ⇒ 整体 deny（理由拼接）；上下文按声明顺序拼接；
/// 诊断文本累积（用户要能看到哪些 hook 坏了）。未信任的 hook 不进结果 ——
/// 信任状态由调用方在面板里展示，这里不重复提示。
pub fn run_event(
    data_dir: &Path,
    event: HookEvent,
    tool: Option<&str>,
    input: &HookInput,
) -> (HookOutput, Vec<String>) {
    let cfg = load_config(data_dir);
    if !cfg.enabled {
        return (HookOutput::default(), Vec::new());
    }
    let trust = TrustStore::load(data_dir);
    let mut acc = HookOutput::default();
    let mut diags = Vec::new();
    let mut contexts: Vec<String> = Vec::new();
    for decl in cfg.decls(event) {
        if let Some(tool) = tool
            && !decl.matches_tool(event, tool)
        {
            continue;
        }
        if !trust.is_trusted(&decl.digest(event)) {
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
            }
            HookOutcome::Diagnosed(d) => diags.push(d),
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
        let (out, diags) = run_event(d.path(), HookEvent::PreToolUse, Some("run_command"), &input);
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
        let (out, _) = run_event(d.path(), HookEvent::PreToolUse, Some("run_command"), &input);
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
        let (out, diags) = run_event(d.path(), HookEvent::PreToolUse, None, &input);
        assert_eq!(out, HookOutput::default());
        assert!(diags.is_empty());
    }
}
