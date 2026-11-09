# G44 Hooks 机制设计（2026-11-09）

## 目标

让用户在自己的机器上把**自定义脚本**挂进 agent 的事件流：工具调用前拦截、调用后补充上下文、会话开始/结束时通知。典型用途：审计日志、把「每次写文件都跑一次 lint」变成确定性行为、按团队规范拦截危险命令、把外部信息注入提示词。

## 为什么用「digest 信任」而不是「开关」

hook 是**任意代码执行**。开关式信任（一个全局"启用 hooks"）挡不住「文件被改过」——用户在 A 时刻审过一份脚本，B 时刻磁盘上的内容已经换了。ZCode 的做法值得照抄：**信任绑定声明内容的哈希**。

- digest = SHA-256(事件名 + matcher + command + args 的规范化拼接)
- 用户「审阅并信任」= 把 digest 记进信任表
- 声明改一个字符 → digest 变 → 该 hook 回到「未信任」状态，**不执行**
- 信任表损坏/读不出 → 视为空表（fail closed），不是"全部信任"
- **yolo / Trusted 权限模式不自动信任 hooks**：那是工具权限的档位，不是代码执行的信任

## 配置

数据目录 `hooks.json`（用户作用域；工作区级 hook 本轮不做——克隆来的仓库自带 hooks 是最高风险面，留后续）：

```json
{
  "enabled": true,
  "events": {
    "PreToolUse": [
      { "matcher": "run_command|write_file", "command": "node", "args": ["D:/hooks/audit.js"], "timeoutMs": 5000 }
    ],
    "PostToolUse": [],
    "SessionStart": [],
    "Stop": []
  }
}
```

- `matcher`：工具名匹配，支持 `*`/`?` 通配与 `|` 多选；空/缺省 = 全部工具。`SessionStart`/`Stop` 忽略 matcher。
- `command` + `args`：**分参数组**，不经 shell（与 `run_command` 的 argv 纪律一致）。
- `timeoutMs`：默认 5000，夹在 100..=60000。
- `enabled: false` 或缺文件 = 全部不跑。

## 协议

stdin 收一个 JSON 对象，stdout 回一个 JSON 对象（**stdout 只承载协议**，日志走 stderr）。

输入：

```json
{
  "event": "PreToolUse",
  "session_id": "…",
  "cwd": "D:/work/proj",
  "tool": { "name": "run_command", "input": { "command": "git status" } }
}
```

输出（全部可选）：

```json
{ "decision": "deny", "reason": "团队规范：禁止直接 git push" }
{ "additional_context": "本仓库用 pnpm，不要用 npm" }
```

- `decision: "deny"` 只在 `PreToolUse` 有意义：工具**不执行**，理由作为工具结果回给模型（与权限门 deny 同一条呈现路径，模型能读到为什么）。
- `additional_context` 在四个事件里都有意义：`PreToolUse` 追加进本次工具结果、`PostToolUse` 追加进工具输出尾部、`SessionStart` 落成一条系统备注、`Stop` 忽略（无人可注入）。
- 退出码非 0 / 超时 / stdout 不是合法 JSON / 字段类型不对 → **不拦截、只记诊断**（进通知条）。理由：hook 的故障不该让对话整体失败；但「本该拦的没拦」的事实必须可见，不能静默。

## 安全边界（写进代码注释）

1. 只跑**已信任**的 hook；未信任的跳过并发一条可见提示（一次性，按 digest 去重）。
2. 分参数执行，不拼 shell；超时 kill（含子进程树回收用既有 KillOnCloseJob 手法）。
3. `async: true` 不做——同步语义才能拦。`Stop` 事件允许 fire-and-forget（不等人）。
4. 输出长度上限：`additional_context` 截 8000 字符、`reason` 截 2000。
5. 环境变量不额外注入；继承父进程（用户脚本要按自己的环境跑）。
6. **hook 不是安全策略**：它自己也可能有 bug。权限门与 SSRF/路径校验仍然是防线，hook 只是用户层的延伸。

## 落点

| 文件 | 内容 |
|---|---|
| `src-tauri/src/hooks.rs`（新） | 声明解析、digest、信任表、子进程执行、输出解析（纯逻辑，不 `use tauri`） |
| `src-tauri/src/agent/session.rs` | `SessionDeps.hooks`；`PreToolUse` 在权限门**之后**、执行**之前**；`PostToolUse` 在结果产生后 |
| `src-tauri/src/ipc.rs` | 装配（读 hooks.json + 信任表）、4 条命令：`hooks_list` / `hooks_trust` / `hooks_revoke` / `hooks_open_folder`；`SessionStart`/`Stop` 发在 `run_turn` 两端 |
| `src/components/HooksPanel.tsx`（新） | 列表 + 每条的状态（已信任/未信任）+ 信任/撤销按钮 + 打开文件夹 |

## 验收（测试钉住的项）

- digest 对内容敏感（改一个字符就不同）、声明规范化稳定（键序/空白不敏感）
- 信任表损坏 → 空表（fail closed）
- 未信任的 hook 不执行
- 输出解析：合法 deny 生效、坏 JSON/超时/非零退出 = 不拦 + 诊断
- matcher 通配与多选
- `additional_context` 截断上限
- 端到端：PreToolUse deny 时工具没有被调用（用假注册表证明）

## 不做（本轮）

- 异步 hooks（同步语义才能拦；`Stop` 的 fire-and-forget 由 hook 自己的超时兜住）

## 后续轮次已补上（2026-11-09 上午）

- **工作区级 hooks 已做**（`<workspace>/.lfttdo/hooks.json`）：**两把锁** ——
  `TrustStore.workspace_enabled` 默认关（克隆仓库自带 hooks 是最高风险输入面），
  打开后仍要逐条 digest 信任；信任键带 `ws:<工作区路径摘要>:` 前缀，
  所以**信任按路径绑定**（仓库 A 信任过的同内容声明不会让仓库 B 也变已信任）。
  用户作用域的前缀是空串 —— v0.2.0 的旧信任表继续有效。
- **`updatedInput` 已做**：`HookOutput.updated_input` 只收 JSON 对象；折叠规则是
  「声明顺序里最后一条说了算」（串行叠加的语义是「谁先谁赢一半」，用户无从预期）。
  **改写之后权限门看的是改写后的入参** —— 拿旧 view 判新入参等于给 hook 一张
  绕过权限门的通行证。改写同时进通知条（不静默）。
