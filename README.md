# LFTTDO

Windows 桌面 AI 助理：聊天、看图、联网抓取、MCP 工具调用，全部运行在本机。
界面是深色玻璃拟态，侧栏可收缩为 56px 窄栏；配置、会话、人设都保存在本机。

> 状态：个人项目，已在 Windows 10/11 x64 上实测可用。采用 **MIT 许可证**（详见文末）。
>
> 2026-10-30：移除 U 盘便携版（启动器与 `portable.txt` 模式）；本仓库只提供常规 Windows 安装版。

## 功能

- **聊天**：流式输出、多会话、长对话自动摘要压缩、上下文预算可调。会话标题由模型按首轮对话自动起（用户手动改过就不覆盖，生成失败降级为截取首条消息）；输入区上方有**上下文占用条**（分母 = 预算 − 输出预留，与压缩触发线同口径，越过 85% 红线点名「下次发送将自动压缩」）；最后一条回答下方可「**重试这一轮**」（截断该轮重新生成，用户原话自动重发——注意：工具写过的文件不回滚，被压缩掉的历史不可恢复）；历史重放超过 40 条只渲染最近 40 条，更早的收进「展开更早的 N 条」。
- **生成式界面**：模型可用内置 `render_html` 把结果渲染成交互页面（图表、表单、可视化演示），在**沙箱 iframe** 里运行（脚本可跑但源是 opaque，读不到父页面与本地存储）。页面可以通过 `postMessage` 请求**把一句话送回对话**——请求先落在卡片内的确认条，由你点「发送到对话」才进上下文（模型生成的页面不可信，回流必须人审）。页面还能保存**双分区状态**：`modelContent`（希望模型知道的选择/进度，随回流一起进对话）与 `privateContent`（滚动位置等纯 UI 状态，**永不进上下文**），重挂后自动灌回页面。
- **看图**：粘贴 / 拖拽 / 选文件三种入口，统一走同一条图片归一化（缩放、转码、体积闸门）；视觉模型可描述图片。
- **联网抓取**：内置 `fetch_url`，按内容抽取正文；每跳重定向都过 SSRF 判定（拒绝非 http(s)、本机、内网与元数据地址）。
- **本地文件**：设置页「文件访问」列出的目录作为**只读**范围；生成/修改文件写到当前输出目录（工作文件夹或默认储存位置），写入走权限弹窗、可放行本会话。路径逐次校验（绝对路径、junction/越界一律拒绝），非 UTF-8 文件明确拒绝而不是静默转码。
- **命令执行**：内置 `run_command`，命令交给选定的 shell 后端执行。shell 可选：设置页「Shell」区块内置 8 个预设 profile（bash、sh、zsh、fish、git-bash、pwsh、powershell、cmd），也支持自定义 profile（任意可执行文件路径，可校验「程序存在且可执行」）；默认 shell 存在设置里，单次调用还可临时指定；每次调用都弹 Exec 确认。`background: true` 可后台执行：立即返回 `task_id`，随后凭它查询运行状态与输出（运行中不回显命令原文）。
- **工作文件夹**：每个会话可选择一个工作文件夹；没选时用设置里的「默认储存位置」（默认是文档目录下的 `LFTTDO`，按会话建子目录）。生成的图片和文件落在输出目录：图片在 `images/`、文件在根目录；写入只允许发生在该输出目录，全局 roots 只读。工具卡会显示文件和图片的绝对路径，点击可在资源管理器中选中。
- **内置工具集**：`grep`（正则内容搜索，结果带起始行号；目录递归有界，自动跳过 `.git`/`node_modules`/`target` 与二进制/非 UTF-8 文件）、`glob`（按 `**/*.rs` 这类模式列出文件，字典序返回绝对路径）、`todo_write`（结构化任务清单，持久化到工作目录的 `.lfttdo/todos.json`，状态机 pending/in_progress/completed）、`undo`（撤销最近一次 write/edit 的内容变更；新建文件不参与撤销——undo 永不删除用户文件）、`task`（派子代理：起一个独立历史的子会话自主完成任务，最终文本回注父对话；可选 `agent` 参数按设置里的 agent 档案派受限子代理——子会话继承该档案的 tools 白名单与非空提示词（如只读调研员），不指定则用全工具集的默认子代理；调用需一次权限确认）、`websearch`（联网搜索：DuckDuckGo 的 HTML 端点，返回「N. 标题 — URL（摘要）」编号清单，最多 10 条；结果与网络可达性受环境限制，反爬页会如实报零命中；只读不弹窗）、`question`（向用户提多选题：2..=6 个候选，答案以「用户选择了：…」回注对话，前端经 `question_respond` 命令回填，提问面板也可自由输入回答；只读不弹窗）、`workflow`（多步编排：工作目录的 `workflows/<name>.json` 定义步骤，每步可选指定 `agent` 档案派受限子代理，`{{上一步名}}` 把上一步结果注入下一步提示词（单次注入上限 8000 字符，超出截断注明）；`list` / `run` 两种动作，某步失败立即停止并在错误里注明停在第几步；run 期间每步开始/结束都向前端发进度事件（工作流名、步骤名、序号 i/N、进行中/完成/失败，界面以通知条显示）；Write 档弹窗）、`experiment`（自优化实验循环：`experiment_run` 在工作目录用平台 shell 跑基准命令并从输出解析 `METRIC 名=值` 行（可 `repeat` 1..=5 复跑取中位数；每次复跑完成后向前端发进度事件——复跑序号 i/N 与当次解析到的指标）、`experiment_keep` 把当前变更 git commit 留存（`experiment:` 前缀提交）、`experiment_discard` 回滚到上次提交（未跟踪新产物一并清理）；账本在 `.lfttdo/experiments.jsonl`；Exec 档每次弹窗，`.lfttdo` 不进 git）。
- **MCP 客户端**：stdio 与 streamable-http 两种传输，启动后台连接、工具命名走 `mcp__{server}__{tool}`、每台 server 可设信任档（全部允许 / 每次询问 / 禁止），信任设置持久化；侧栏 MCP 面板可在应用内直接编辑 `mcp.json`，可配置多个 stdio / streamable-http server（严格校验、重启生效）。工具返回的图片会直接显示在工具卡片里（最多 4 张、合计 base64 ≤ 8 MB；超出的在预览里注明），模型侧按既有的最近 3 张保留上限接收图片。
- **长期记忆**：跨会话记住事实、偏好、画像与待办；在上下文压缩、空闲、切换会话时自动抽取，每轮按相关度召回；设置页可查看、编辑、新增、删除、清空。
- **工具权限门**：三档模式（设置里下拉）：`Rules`（默认：规则引擎 + Read 自动放行 + 其余弹窗）、`AskAll`（allow 规则与会话放行全部失效，Write/Exec 逐次弹窗，Read 照常自动）、`Trusted`（走到权限门的调用一律自动放行——用户明示自己兜底的档）。默认档下 Read 自动放行；Write/Exec 弹窗确认；MCP 工具默认每次询问；未知工具按最严一档处理。设置里可写 `permission_rules`（JSON 规则：allow/ask/deny + `*`/`?` 通配；`run_command` 按 `&&`/`&`/`||`/`;`/`|`/换行分段匹配——`git status && rm -rf /x` 不会被 git 前缀的 allow 带过；单个 `&` 同样切段：bash 里 `A & B` 是 A 后台执行、cmd 里 `A & B` 是顺序连接，两种语义下 B 都会跑，判定必须逐段。已知边界均为安全侧：powershell 5.1 不支持 `&&`（整条语法失败、不会执行）、PowerShell 反引号续行不识别（整段落回默认路径弹窗）；规则 deny 压过会话记住的放行），改完设置下一条消息生效。
- **Plan 模式**：设置开关打开后模型只能看和读——写入/命令类调用被直接拒绝（提示先完成调研给出方案、退出 plan 模式再执行修改），Read 类照常；同样改设置下一条消息生效。
- **长任务护栏**：上下文超限自动收缩重试一次；步数触顶收回工具声明、强制模型文字总结收尾；工具输出超长自动截断并把全文落盘到 `.lfttdo/outputs/`（返回里给出可再读的路径）；roots 下的 `AGENTS.md` 每轮现读注入系统提示（单份 8 KiB 截断）。长对话压缩的摘要是结构化交接：按客观/已完成/决定/阻塞/下一步/涉及文件六节组织，上限 600 字。
- **自动诊断**：设置里配 `diagnostics_command`（如 `cargo check --message-format short`）后，每次 `write_file` / `edit_file` 落盘成功都在工作目录自动运行一次，输出以【自动诊断】追加进对话（合并 stdout/stderr、截 4000 字符）；空输出、命令跑不起来、超时（15 秒）不追加，退出码不参与判定（编译器报错走非零退出，照样追加）。慢命令会拖慢每次写文件之后的节奏，建议配短平快的检查。
- **人设**：设置页可直接编辑系统提示词，支持从 `.txt`（UTF-8）导入、导出 `.txt`、恢复内置；留空使用内置提示词，内容只保存在本机 `settings.json`，保存后下一条消息生效。
- **自定义 Agent**：设置里可维护 `agents` 档案（name / description / prompt / tools 白名单）并用 `active_agent` 指定生效档案；激活后非空 prompt **整段替换**系统提示词（优先于用户人设），tools 白名单双层生效——声明表只留名单内工具、白名单外的调用在执行前被拒（不执行、不弹窗）；未激活的档案只是存档。档案还与 `task` / `workflow` 联动：`task` 按档案名派出的子代理继承该档案的提示词与工具白名单，`workflow` 的步骤同样可用 `agent` 指定档案（受限子代理）。改设置下一条消息生效。
- **技能包**：`skills/<名字>/SKILL.md` 放一份说明，索引自动进系统提示；模型需要时调用
  `use_skill` 读取正文，用户这句话里带到技能名或 `keywords` 时正文直接自动加载。
  只读本地文本，不执行脚本；加/改技能不用重启（下一条消息生效）。
  侧栏「技能」面板可查看当前技能、逐个启用/停用、刷新并打开技能文件夹；输入框敲 `/` 直接弹出技能快捷菜单，↑↓ 选中即填入引导语；SKILL.md 里写 `argument-hint: 要翻译的句子` 会在菜单里显示参数提示，且 `/技能名 参数` 发送时自动展开成标准句式。
- **@文件提及**：输入框敲 `@` 弹出文件搜索浮层（文件名/相对路径子串匹配、浅层优先、最多 20 条），选中把绝对路径插入消息——给模型精确锚点，省它自己猜文件在哪。搜索范围是设置里的文件访问目录 + 当前会话输出目录；`.git`/`node_modules`/`target` 永远不进结果。
- **Hooks（自定脚本挂钩）**：数据目录的 `hooks.json` 里声明脚本，挂到四个事件上——`PreToolUse`（工具执行**前**可 deny + 理由，工具不会执行）、`PostToolUse`（执行后可追加一段上下文给模型）、`SessionStart`（每轮开始注入备注）、`Stop`（回合收尾，适合写审计/通知）。stdin 收 JSON、stdout 回 JSON；**分参数执行不经 shell**；超时（默认 5s）整棵进程树回收。安全模型：**信任绑定声明内容的 SHA-256**（改一个字符就回到未信任，`Trusted` 权限模式也不会自动信任），信任表损坏按空表（fail closed）。侧栏「Hooks」面板可查看/逐条信任/撤销、打开配置文件。hook 失败只进通知条、不拦对话。
- **会话管理**：侧栏新建/重命名/删除（两段式确认）；**导出会话为 Markdown**（保存对话框，摘要保留为一节、工具调用与结果折成一行引用，Windows 非法文件名字符自动清洗）；切走窗口后模型提问、等待授权或回合收尾时**任务栏闪烁提醒**（用户自己按的停止不提醒）。
- **界面**：深色玻璃、居中聊天列、Enter 发送 / Shift+Enter 换行、自动滚底（上翻时不抢）、整窗拖图、思考过程可折叠、通知条自动合并折叠、标题栏随主题精确着色（明暗两档 + 跟随强调色）。

## 技术栈

| 层 | 选型 |
|---|---|
| 桌面壳 | Tauri v2 |
| 内核 | Rust（会话循环、工具注册表、权限门、MCP 桥、SQLite 持久化、图片处理） |
| 前端 | React + TypeScript + Vite |
| 模型 | 任意 OpenAI 兼容端点（推荐本机 Ollama） |
| MCP | `rmcp` 3.5.1（client + stdio 传输） |

## 快速开始

前置：Node.js、Rust（MSVC 工具链）、Windows 10/11。

```bash
npm install
npm run tauri dev          # 开发
npm run tauri build        # 打包（NSIS 安装器）
```

首次打开后进入「设置」，填写：

- 接口地址：`http://127.0.0.1:11434/v1`（本机 Ollama）或任意兼容端点
- 模型名：你已下载的模型
- 云端端点需要填自己的 API Key（存入 Windows 凭据管理器，不写配置文件）

## 数据位置

常规安装的数据都在 `%APPDATA%\com.qwa.do\`：

- `settings.json`：模型地址与模型名、上下文预算、主题、长期记忆开关、`system_prompt`（人设）、自定义 shell profile 与默认 shell、权限规则（`permission_rules`）、plan 模式开关（`plan_mode`）、自动诊断命令（`diagnostics_command`）、自定义 agent 档案与生效档案（`agents` / `active_agent`）
- `qwen-do.db`：会话、消息与长期记忆
- `skills/`：技能包；`skills.json` 记录每个技能的启停
- `mcp.json`：MCP 服务器配置（可选）；`mcp_trust.json` 记录每台服务器的信任档
- `window.json`、`qwen-do.log`：窗口位置与日志
- 工作目录侧（会话选定的工作文件夹，不在 `%APPDATA%`）：`workflows/`（workflow 的多步编排定义，`workflows/<name>.json`）、`.lfttdo/experiments.jsonl`（experiment 的实验账本；整个 `.lfttdo/` 不进 git——experiment 提交时排除、回滚时保全）

## 技能包（skills）

每个技能是一个文件夹，里面放一份 `SKILL.md`（UTF-8）。开头可选 frontmatter：

```markdown
---
name: 翻译助手
description: 中英互译，保留术语表
---
先判断原文语言，再翻译；术语表里的词必须按表译。
```

- `name` / `description` 缺省时依次取第一个一级标题、文件夹名、正文第一句。
- 技能名和文件夹名都能作为 `use_skill` 的参数（ASCII 大小写不敏感）。
- 系统提示里只放「名字 + 一句话描述」；正文（单份上限 64 KiB）在模型调用
  `use_skill` 时才读入上下文。
- 每个技能可在侧栏「技能」里单独启用/停用；新技能默认启用。停用的技能既不出现在
  系统提示里，`use_skill` 也会拒绝。选择表 `skills.json` 放在数据目录（与
  `mcp.json` 同层），不写回技能文件夹。
- **本地小模型的触发稳定性**：`keywords`（触发词）命中时，正文由软件直接放进本轮系统提示，
  不赌模型自己想起来调工具；单轮最多自动加载 2 份、每份 16 KiB，超出部分提示模型自己去读。
- **「工作方式」段**（`context.rs::TOOL_POLICY`）固定追加在人设之后：先调工具拿依据再回答、
  技能先读正文、MCP 工具与内置工具同等对待。人设是整段替换内置提示词的，这段规矩不能被
  人设一起抹掉 —— 这是本地模型「有工具却凭记忆答」的首要原因。
- 面板里的「用这个技能」会把 `使用「技能名」技能：` 填进输入框并聚焦，不替用户发送。
- 技能目录是数据目录下的 `skills/`。目录不存在时 `use_skill` 不会出现在模型工具表里。
- 扫描时每个 `SKILL.md` 都会 canonicalize 并校验仍在技能根内，指向根外的
  符号链接 / junction 会被跳过；模型传入的名字从不参与拼接路径。

## 测试

```bash
cd src-tauri && cargo test --all-targets
npx vitest run
npx tsc --noEmit
npm run lint:css
```

Rust 侧需 MSVC 工具链：默认工具链不是 MSVC 的机器，用 `rustup default stable-msvc` 设为默认，
或 `cargo +stable-x86_64-pc-windows-msvc test --all-targets` 显式指定（GNU 工具链在含非 ASCII
字符的仓库路径下链接器会崩）。仓库路径含非 ASCII 字符时，aws-lc-sys 的 NASM 构建步骤也会失败，
可设 `AWS_LC_SYS_PREBUILT_NASM=1` 改用预编译汇编，或把仓库放到纯 ASCII 路径。

当前读数：Rust 957 passed / 1 ignored + 集成 28 passed（gapcheck 12 + gapcheck2 15）；前端 308 passed；clippy 无新增
（Windows 目标也已是 `-D warnings` 门禁）。权限分段、实验与诊断相关用例带 Windows 镜像：push 后 windows-latest
会实跑同一逻辑（`run_command` 的 shell 执行用例仍为 unix 专属——sh 语法）。

## 目录结构

```text
src/                前端（组件、IPC 封装、状态 reducer、主题 token）
src-tauri/src/
  agent/            会话循环、上下文组装、摘要压缩、长期记忆、事件类型
  model/            OpenAI 兼容适配器（含 reasoning 流）
  tools/            内置工具与注册表（fetch_url、read_file、write_file、edit_file、grep、glob、run_command〔可选 shell 后端〕、todo_write、undo、use_skill、memory_search、task、websearch、question、workflow、experiment）
  permission/       权限门与信任档
  mcp/              mcp.json 读取/编辑、工具命名、stdio / HTTP 桥、进程树回收
  store/            SQLite 会话与长期记忆持久化
  image_prep.rs     图片归一化
docs/               设计、计划与验证记录
release/            本地发布说明与各版本源码归档（gitignored，不参与构建）
```

## 已知限制

- **Windows x64 为一级支持**：进程树回收（JobObject）与凭据存储（凭据管理器）走 Win32。
- **Linux 目前仅到「可编译」**：keyring（secret-service / zbus）、进程组回收（libc `kill(-pgid)`）、rfd（xdg-portal）的在途移植让 `cargo test` 在 Linux 全绿；GUI 与日常使用未验证。
- **只有常规安装版**：数据固定在 `%APPDATA%\com.qwa.do\`，不提供 U 盘便携模式。
- **HTTP MCP 需要服务端支持 streamable-http**：URL 需可达；不返回 `Mcp-Session-Id` 的 server 也能连（客户端按 stateless 处理），stdio 专属的 server 仍按原方式配置。
- MCP 配置里的每个 server 必须是 `mcpServers` 的直接子项；放在根对象或另一个 server 内部会被编辑器拒绝并指出条目，不会静默丢弃。
- **常驻内存约 400 MB**：大头是 WebView2 的多进程运行时，不是泄漏。
- **人设导出不能选路径**：走 WebView 下载，默认落到系统下载目录；暂不支持“另存为”。
- 内置联网抓取只抓公开网页，需要登录的页面抓不到。
- **「重试这一轮」只删对话行，不回滚副作用**：该轮里工具写过的文件、执行过的命令不会撤销；被上下文压缩掉的历史已物理删除，重试也找不回来。
- 写文件只处理 UTF-8 文本：单次写入 ≤ 256 KiB，已有目标与编辑结果 ≤ 4 MiB；GBK 等编码、含 NUL 的内容、以及 `edit_file` 遇到 LF/CRLF 混用的文件都会被拒绝，不做转码。
- 工具图片缓存只增不减：缓存现在写在各会话的输出目录（`<输出目录>/images/`），可手工清理；重开旧会话时会按需从会话库重建。打开文件位置只允许 roots 或该会话输出目录内的已存在路径。
- **视频产物尚未处理**：当前只识别工具返回的文本和图片；ComfyUI 返回的视频（URL/文件/Blob）还不会自动落盘或播放，等拿到真实返回样例后再补。
- 思考过程的展示依赖模型把 reasoning 放进增量的 `reasoning_content`（或正文内联 `<think>`）。

## 安全说明

- API Key 只写 Windows 凭据管理器；不写 `settings.json`、不进日志、不出现在导出里。
- 工具参数逐段传 argv，不拼 shell 命令行；MCP 子进程设置 `CREATE_NO_WINDOW`。
- SSRF 判定在每一跳重定向上执行；内网与本机地址默认拒绝。
- 文件工具只接受 roots 或当前会话输出目录内的绝对路径；写盘走同目录临时文件 + 原子替换，非 UTF-8 目标不会被静默转码。写工具只允许写当前会话输出目录，想编辑其它目录请把它设为工作文件夹。
- 前端无 shell / 文件系统权限，进程执行全部在 Rust 侧。
- `run_command` 是 `Risk::Exec`：每次调用都弹窗、会话级放行的粒度是**整条命令文本**（逐字相同才放行）；
  前台命令结束整棵进程树被回收，不留后台进程。`background: true` 的后台任务不随「停止」中断，
  以超时为唯一终点，到点同样回收整棵进程树。
- 应用内配置编辑器会把 `mcp.json` 原文（含 env/headers）显示在界面里；MCP 列表与状态命令始终不返回这些值。
- Markdown 链接经 `open_url` 交给系统默认浏览器（只允许 http/https），不导航 WebView；
  WebView 顶层导航另有白名单（只放行应用自身来源与本机开发服务器）。

## 许可

本项目采用 **MIT 许可证**（见根目录 `LICENSE`）。MIT 为宽松型开源许可证：允许复制、
修改、分发与商用，仅需在衍生作品中保留版权声明与许可证全文。
