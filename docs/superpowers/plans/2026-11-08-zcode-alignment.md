# ZCode 对照对齐计划（接 #70 的 G31–G33 之后）

来源：2026-11-08 对 zai-org/ZCode 仓库的机制级调研（UI_PLUGIN.md、apps/zcode-cli/docs/ 的 hooks/commands/permissions/automation/loop/skill 设计文档）+ 本仓库源码实读盘点。
本文件是长期状态表：做完一项勾一项，两个并行会话都以这里为准。

## 已完成

| 编号 | 内容 | 落点 | 状态 |
|---|---|---|---|
| G31 | 权限模式三档 | permission/gate.rs | ✅ #70 |
| G32 | render_html 沙箱渲染 | tools/render_html.rs | ✅ #70 |
| G33 | slash 技能菜单 | Composer.tsx + skills.rs | ✅ #70 |
| — | 自动上下文压缩（0.85 阈值 + 六节摘要 + 滑窗/预算双闸） | context.rs / summarize.rs / ipc.rs compact_before_turn | ✅ 更早 |
| — | 思考流自动折叠、流式末块纯文本、气泡 memo、通知 ×n 折叠 | MessageBubble / MessageList | ✅ 更早 |
| G34 | 仓库根 AGENTS.md（并行会话契约） | AGENTS.md | ✅ 2026-11-08 |

## 本轮批次（2026-11-08 会话执行）

| 编号 | 内容 | 落点 | 量级 | 状态 |
|---|---|---|---|---|
| G35 | 上下文占用条：`Usage` 事件带净预算（`budget_tokens - reserve_output`，Rust 算好）+ 状态栏占比条（>85% 红档点名「将自动压缩」，阈值与 compact_threshold 同源） | events.rs / session.rs / types.ts / reducer.ts / App.tsx(CtxMeter) / styles.css | S | ✅ 全量绿（Rust 979 / 前端 338） |
| G36 | 注意力提醒：`needs_attention()`（权限/提问/收尾，用户自按的 Interrupted 豁免）+ 失焦时 `request_user_attention` 闪任务栏。**选型变更**：不引 tauri-plugin-notification —— 违背本仓库「无外部插件」决策（Cargo.toml 头注释）+ <15MB 安装包红线；Tauri 内置 API 零依赖 | events.rs(needs_attention) / ipc.rs(TauriEmitter.with_notify) | S | ✅ 判定有单测；闪烁行为待真机验收 |
| G37 | 首轮自动命名：R104 基础版（截首条消息）升级为**模型生成标题**（summarize.rs `title_request`/`sanitize_title`/`collect_title`，与摘要同一套收流/清洗/无温度）。落库后 Done 之后异步 spawn（本地端点数秒不卡收尾）；沿用本轮 cancel，用户按停即取消；失败/清洗为空降级回 R104 首条消息截断（fallback_title）。旧 auto_title 已删除，语义融入 namer | ipc.rs(namer) / summarize.rs / db.rs(TITLE_CHARS pub) | S | ✅ 2026-11-08 另一会话完成：lib 975 全绿（新增 summarize 5 条测试 + a_default_title 重写钉模型标题优先）；夹具 ReplyModel/FakeModel 按请求分流标题支、run_memory_turn 滤起名调用（否则起名任务插队消耗脚本回复队列，6 条端到端测试集体变红的根因） |
| G38 | 会话导出 Markdown：System 摘要保留为一节、工具调用/结果折一行引用（预览复用 ToolOutput::preview，R77 单一口径）、rfd 保存对话框（**零新依赖**，弃 tauri-plugin-dialog 同一理由）、Windows 非法文件名清洗 | ipc.rs(messages_to_markdown/session_export) / Sidebar.tsx / commands.ts | S | ✅ 4 条新测试，全量绿 |

**⚠️ 并行冲突记录（2026-11-08 晚，已解决）**：本会话与另一会话同时在 ipc.rs 上干活（本会话 G35/36/38，对方 G37 namer）。对方写了一半时 crate 编译不过，经用户裁定「等对方写完再整体验收」；对方完成后全量验收通过（Rust 979 + 3 集成 / clippy 零警告 / 前端 338 全绿 / fmt 干净）。教训沉淀在仓库根 AGENTS.md 与项目记忆：验收前先 `git diff --stat` 分区、两会话开工前读本计划表。

## 第二轮批次（2026-11-08 深夜，同会话续）

| 编号 | 内容 | 落点 | 量级 | 状态 |
|---|---|---|---|---|
| G42 | 历史折叠：重放超 40 条只渲染最近 40 条 + 顶部「展开更早的 N 条」。渲染层裁剪（reducer 不动）；`isLastTurn` 按全量下标算 slice 偏移；App 给 MessageList 挂 `key={activeId}` 切会话重置折叠态；虚拟滚动（virtuoso + followOutput，注意贴底 stickRef 语义替换）等真卡再上 | MessageList.tsx / styles.css / App.tsx | S | ✅ 3 条新测试 |
| G39 | rewind 第一版「重试这一轮」：`Db::truncate_last_user_turn`（真人判别三道与 latest_user_message 同线、单事务、保留压缩备注、返回原话块）+ `session_retry_last`（running guard，跑着不许重试）+ 前端最后一条 assistant 气泡「↻ 重试这一轮」→ 截断→重放→走**同一条 send** 重发（乐观插入/回滚/自动改名免费复用）。任意点回退与工具写入核对（file-summary-safe-rewind）留后续 | db.rs / ipc.rs / MessageBubble.tsx / MessageList.tsx / App.tsx / commands.ts | M | ✅ Rust 4 条 + 前端 3 条新测试 |
| — | 欠账清偿：全仓 cargo fmt（ipc.rs ~2 万行重排系该文件首次格式化，整文件 diff 有 #70 先例）+ clippy 零警告 + 全量绿（Rust 979+3 / 前端 338） | 全仓 | — | ✅ |

**待办**：真机验收五项（任务栏闪烁 / 导出对话框 / 占用条 / 重试按钮 / 折叠展开）。

**终态（2026-11-08 深夜）**：G35-G39、G42 全部落地并推送。第一轮 CI 7/8（`c9e2931` 的 DWM 常量在 Linux clippy 下 dead_code——`cfg(windows)` 函数引用的常量也要挂 cfg），补 `#[cfg(windows)]` 后复跑 **CI 8/8 全绿**（run 42493607254）。README 功能/已知限制已同步。下一批从 G40/G41 或 G43 起。

## 第三轮批次（2026-11-09 深夜，同会话）—— G40/G41/G43 + 安全债全清

| 编号 | 内容 | 落点 | 状态 |
|---|---|---|---|
| G41 | slash 模板命令：SKILL.md `argument-hint`/`参数提示`（≤60 字符，菜单展示）+ Composer 提交时 `/技能 参数` 展开成标准引导语句式（未知名原样发，不猜） | skills.rs / ipc.rs SkillView / types.ts / Composer.tsx | ✅ Rust 1 条 + 前端 2 条新测试 |
| G43 | @文件提及：新模块 `tools/file_search.rs`（子串匹配、浅层优先、跳 .git/node_modules/target/.lfttdo、深度 8、单根 4000 条预算）+ `session_search_files` 命令（roots + 会话输出目录）+ Composer @ 浮层（防抖 180ms、↑↓/Enter/Esc、`@词` 整体替换为路径）。与 glob 工具分工：glob 给模型按模式列文件，这个给人模糊找 | tools/file_search.rs / ipc.rs / commands.ts / Composer.tsx | ✅ Rust 4 条 + 前端 2 条新测试 |
| G40 | render_html 回流闭环：沙箱 `sandbox=""` → `allow-scripts`（仍无同源）；页面 `postMessage({type:"lfttdo:followup",prompt})` → **确认条人审**（模型生成的页面不可信，回流必须过目；来源校验钉死本卡片 iframe、prompt 截 2000、流式中禁发）→ 用户点了才走同一条 send | MessageBubble.tsx(RenderedHtmlCard) / MessageList / App | ✅ 前端 2 条新测试 + sandbox 契约更新。widgetState 双分区（页面状态持久）留作后续增强 |
| 债① | experiment 基准命令接 `KillOnCloseJob`（spawn 后 assign，任何出口 Drop 回收整棵树；失败退回 kill_on_drop 保底）。workflow 不直接 spawn（走子代理→shell 工具已有 job） | tools/experiment.rs run_command | ✅ |
| 债② | gate 新增：experiment_run 的 `input.command` 按 `split_command_segments` 切段、查 **run_command 的规则表**——`rm *` → deny 不许在实验循环里被绕过；规则 allow 刻意不给 Write 档免弹窗 | permission/gate.rs | ✅ 2 条新测试 |
| 债③ | websearch 端点请求前过 `check_url` + `resolve_target`（整集复验）。**如实标注残留**：判定后 reqwest 仍自行解析（TOCTOU 窗口）——钉地址要按次重建 client，端点恒定+判定先行已把窗口压到极低 | tools/websearch.rs guarded_endpoint | ✅ 2 条新测试（字面内网 IP） |
| 债④ | permission_rules 形状非法时，除日志外**发非致命 error 事件**进前端通知条（「已忽略并清空，修正设置后下一条消息生效」）——fail-safe 语义不变，可见性补上 | ipc.rs send_message 装配段 | ✅（`#[tauri::command]` 不可单测，同既有段口径） |
| 债⑤ | 非 Exec 工具凡带 `path` 参数的，G13 匹配串用路径本身——路径型规则（`write_file: [{pattern:"*/.env*"}]`）原来恒为空串永不命中。无 path 工具维持空串，`*` 通配行为不缩窄 | permission/gate.rs check | ✅ 1 条新测试 |

**终验**：Rust 989 lib + 3 集成 / clippy 零警告 / fmt 干净 / 前端 344 / tsc / lint:css 全绿。教训：多根目录排序测试不许钉 tempdir 随机名的字典序（实测翻车）；`cargo test --all-targets` 与运行中的 dev 实例抢 `lfttdo.exe`（os error 5）——先停 dev。

## 剩余排队

| 编号 | 内容 | 量级 | 备注 |
|---|---|---|---|
| G44 | hooks 机制 | L | **先写 spec**；做前拆 ipc.rs |
| — | ipc.rs 拆分（~2 万行） | L | G44 前置 |
| G45 | 会话级回归评测 | M | 外部启动器 + 案例集 |
| G40+ | widgetState 双分区（页面状态持久化） | M | 回流闭环已通，状态持久是增强 |

## 已明确不做 / 暂不做

- 事件溯源改造（messages 表为真相 + 投影已满足需求， rewind 用 DELETE 尾部行即可）
- btw 旁路提问、会话分叉 fork、定时自动化、bot 渠道集成
- 任务栏徽标 overlay icon（Windows 需 ITaskbarList3 额外验证，toast 先行）
