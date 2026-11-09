# G45/G45+ 会话级回归评测

外部启动器 + 案例集 + 证据归档。**不是 CI 的一部分**（CI 无模型、无网络），
是「要发版 / 要大改时手动跑一遍」的回归门。

## 责任边界

照 ZCode `usage/13-automation.md` 的取向：**LFTTDO 只当执行器**。案例集、
环境隔离、资源预算、评分全部在这些脚本里；产品代码不为评测加开关 ——
唯一的入口是 `lfttdo --headless`，而它本身也是产品自己的命令行模式
（`--headless --prompt "..."`，不带它就照常起 GUI）。

## 两层判定

| 层 | 用什么跑 | 断言什么 |
|---|---|---|
| **启动层** | 发布版 GUI 二进制 | 冷启动起得来、保持运行、隔离数据目录、退出时进程树回收 |
| **会话层** | `lfttdo --headless` | 真实一轮的 `tool_call` 序列、`stop_reason`、产物文件、回复关键词 |

启动层是「每轮评测的第一道闸」；会话层才是真正的回归信号。

## 用法

```bash
npm run tauri build                # 评测用发布版二进制，不替你构建
node scripts/eval/run.mjs --dry              # 先看将跑哪些用例（含端点可达性）
node scripts/eval/run.mjs                    # 跑全部（需要模型端点）
node scripts/eval/run.mjs --case read-a-file # 只跑一条
node scripts/eval/run.mjs --boot-only        # 只跑启动层（不需要模型）
```

**端点不可达时会话层明确失败（exit 1），不会静默跳过。** 这一条是刻意的：
「评测全绿」如果其实一条会话断言都没跑，那是最坏的一种假信号。

端点与模型写在 `cases.json` 的 `endpoint` 段（默认本机 Ollama
`http://127.0.0.1:11434/v1`）。

## headless 入口

```bash
src-tauri/target/debug/lfttdo.exe --headless \
  --prompt "用 glob 列出所有 .txt" \
  --data-dir "$TEMP/lfttdo-hl" \
  --base-url http://127.0.0.1:11434/v1 \
  --timeout 120
```

- **stdout 是纯 NDJSON**（一行一个 `AgentEvent`，与前端收到的是同一份线上格式）；
  诊断/日志一律走 stderr，所以 `| jq -c` 之类的下游不会被中文提示打断。
- 退出码：`0` 正常收尾 / `1` 运行错误 / `2` 用法错误 / `3` 超时。
- 数据目录默认 `%APPDATA%\com.qwa.do\headless`（与 GUI 的数据目录**同父不同叶**：
  跑评测不污染用户真实会话库）。
- **不模拟点「允许」**：headless 的权限门没有 sink，所以 Write/Exec 走
  `没有可应答的界面` 的 deny（`gate.rs` 的既有语义）。这比 GUI **更严**，
  不是为评测放宽权限。
- 不跑空闲记忆 worker、不连 MCP、不开窗口。

## 证据

每次运行落 `scripts/eval/runs/<时间戳>/`：

- `fingerprint.json`：git SHA（含是否脏工作区）、版本、平台、node 版本、时间
- `profiles/<case>/`：该用例的独立数据目录（HOME/APPDATA 全指到这里）
- `<case>.ndjson`：会话层的**完整原始事件流**（出了分歧要能回看这一轮到底发生了什么）
- `<case>.stderr.txt`：诊断输出
- `results.json`：每条用例的结论（含 `layer`、`binary`、`failures`）

`runs/` 不进 git（`.gitignore`），但**结论要能对回代码**：指纹是评测报告的一部分。
`binary` 字段标明跑的是 `release` 还是 `debug` —— 本地迭代时用调试版是显式降级。

## 加一条会话层用例

```json
{
  "name": "grep-content",
  "prompt": "用 grep 找出 src 里所有 TODO",
  "why": "为什么值得回归",
  "seed": "seeds/notes",
  "expect": {
    "stop_reason": "end_turn",
    "any_of_tools": ["grep"],
    "files": ["notes.txt"],
    "text_contains": "TODO",
    "no_errors": true
  },
  "timeout_secs": 180
}
```

判定字段：

| 字段 | 含义 |
|---|---|
| `expect.tools` | **精确**的工具序列（顺序有意义 —— 工具的顺序执行是产品契约） |
| `expect.any_of_tools` | 宽松档：至少用到其中一把（模型可能选不同工具达成同一目标） |
| `expect.stop_reason` | `end_turn` / `max_turns` / `interrupted` / `error` |
| `expect.files` | profile 里的相对路径产物（`/` 分隔） |
| `expect.text_contains` | 助手回复里的子串 |
| `expect.no_errors` | `true` = 不许出现任何 `error` 事件 |
| `seed` | 指向 `scripts/eval/` 下的目录，复制进该用例的 profile |

## 已知限制（如实标注）

- 会话层断言依赖模型的**工具选择**，所以用 `any_of_tools` 这类宽松档多于精确序列；
  需要精确序列的用例应选「只有一种合理做法」的提示词。
- headless 没有工作文件夹，所以**工作区级 hooks 不在评测范围**（用户作用域的 hooks
  会跑，因为它们的目录就是 `--data-dir`）。
- 超时是「放弃等待」而不是优雅中断：进程退出后仍在写盘的工具会被操作系统回收，
  可能留下半截产物。评测的产物断言因此只在 `exit 0` 的用例上才可信
  （`judge()` 已经把非零退出记成失败）。
