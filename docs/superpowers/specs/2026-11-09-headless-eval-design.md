# G45+ headless 会话入口设计（2026-11-09）

## 目标

让评测（`scripts/eval`）能对**真实的会话行为**做断言：给一句 prompt，拿到这一轮完整的事件流，验证工具序列 / `stop_reason` / 产物。在此之前 G45 只能验「起得来 + 隔离」。

## 为什么是命令行入口，不是模拟点击

GUI 模拟点击的成本远高于它的信号价值：要处理窗口焦点、渲染时序、中文输入法、DPI 缩放，而它测到的仍然是同一条会话路径。一个纯命令行入口把同一轮会话跑完、把**与前端完全相同的线上事件格式**打到 stdout，信号更硬、成本更低。

## 边界

| 项 | 决定 | 理由 |
|---|---|---|
| 是否 `use tauri` | **否** | Plan A 约束。`main.rs` 在 `run()` 之前分派 |
| 装配是否另起一套 | **否** | 复用 `build_production_registry` / `memory_turn_cfg` / `agent_profiles` / `Settings::to_context_config` —— 简化版测的就不是产品 |
| stdout 内容 | **只有 NDJSON** | 诊断一律 stderr，否则 `\| jq` 会被一行中文打断 |
| 权限弹窗 | 不模拟应答 | gate 没有 sink ⇒ Write/Exec 走 `没有可应答的界面` 的 deny。**比 GUI 更严** |
| 空闲记忆 worker | 不启动 | 它是唯一没有用户手势的模型调用点，评测不该顺带触发 |
| MCP | 不连接 | 同上：评测要可复现，不引入外部进程 |
| 窗口 | 不创建 | 这就是 headless 的定义 |
| 压缩判定 | 不跑 | 需要 `TurnDeps` 全套装配，而这一轮已经在跑了。落库只**追加**本轮新增行 |

## 命令行

```
lfttdo --headless --prompt "<文本>" [选项]

  --prompt <文本>        必填
  --data-dir <目录>      默认 %APPDATA%\com.qwa.do\headless
  --session <标题>       同名会话复用（多轮案例）
  --base-url <URL>       覆盖端点
  --model <名>           覆盖模型
  --timeout <秒>         默认 300
```

退出码：`0` 正常收尾（`done` 到达）/ `1` 运行错误 / `2` 用法错误 / `3` 超时。

## 关键取舍

**1. 数据目录与 GUI 同父不同叶。** 默认 `...\com.qwa.do\headless`，不是 `...\com.qwa.do\`。一次评测会写会话库、记忆、日志，混进用户真实数据里既污染，也会让下一次评测读到上一次的残留。

**2. 缺省输出目录落在数据目录下。** GUI 的 `resolve_session_output` 要 `AppHandle` 取文档目录，headless 没有。差别只在**默认储存位置**（`<data>\sessions\<id>` vs `<文档>\LFTTDO\<id>`），不在语义 —— 有工作文件夹时两边都是它。

**3. 超时是「放弃等待」而不是优雅中断。** headless 的取消令牌槽位是空的（没有人能按停止），所以超时只能放弃等待 future。如实记录：进程退出后仍在写盘的工具会被 OS 回收，可能留下半截产物；因此评测的产物断言只在 `exit 0` 的用例上可信。

**4. 落库只追加。** `run_turn` 是「压缩判定 + `append_messages`」；headless 记住恢复时的行数，跑完取 `history()[restored..]` 追加。不压缩的理由见上表。

## 落点

| 文件 | 内容 |
|---|---|
| `src-tauri/src/headless.rs`（新） | 参数解析、NDJSON sink、装配、一轮执行、9 条单测 |
| `src-tauri/src/main.rs` | `--headless` 分派（判定在 `wants_headless`，可单测） |
| `src-tauri/src/lib.rs` | `pub mod headless;` |
| `scripts/eval/run.mjs` | 两层判定（启动层 / 会话层）、端点前置检查、NDJSON 证据归档 |
| `scripts/eval/cases.json` | `boot_only` 与 `prompt` 两类用例 + `expect` 契约 |
| `scripts/eval/seeds/notes/` | 产物/读取断言用的夹具文件 |

## 验收（测试钉住的项）

- 参数解析：缺 `--prompt` / 空白提示词 / 缺值 / 未知开关 / `--timeout 0` 全部报用法错误
- `--headless` 是唯一模式开关（不带它的命令行行为不变）
- 端点为空必须被拦下（与 GUI 装配层同一个判据）
- 默认数据目录与 GUI 同父不同叶
- 端到端：`turn_start` → `error`（端点不通）→ `done(stop_reason=error)`，stdout 纯 NDJSON
- 评测脚本：端点不可达时会话层**明确失败**（exit 1），不是静默跳过

## 不做（明确记录）

- 不驱动 GUI（界面回归属另一层）
- 不模拟权限应答（headless 的 deny 是产品的既有语义，不改成「评测全允许」）
- 不做多轮编排脚本（用例里 `session` 复用已够表达多轮；复杂流程属上层）
