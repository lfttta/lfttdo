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

**待办**：真机验收五项（任务栏闪烁 / 导出对话框 / 占用条 / 重试按钮 / 折叠展开）；提交推送（工作区混着两会话的活且 ipc.rs 无法按文件分离，只能一个 commit）；推送后 gh 查 CI。

## 后续批次（按序，每批先写 spec）

| 编号 | 内容 | 关键落点 | 量级 | 备注 |
|---|---|---|---|---|
| G40 | render_html 交互闭环：widgetState 双分区（modelContent ≤16KiB 进上下文，privateContent 不进）+ sendFollowUpMessage | render_html.rs + iframe postMessage 桥 + ipc | M | 抄 ZCode UI_PLUGIN.md 的 widgetState 设计 |
| G41 | slash 模板命令：SKILL.md frontmatter 加 argument-hint，正文 $ARGUMENTS/$1 展开 | skills.rs + Composer | M | 红线：模板内 shell 片段解析期绝不执行，只作为文本进消息 |
| G43 | @文件提及：Composer @ 浮层防抖搜索，复用 glob 枚举（需新 ipc list_files） | Composer.tsx + tools/glob.rs | M | 给小模型精确路径锚点 |
| G44 | hooks 机制：PreToolUse/PostToolUse/SessionStart/Stop 四事件 + digest 信任模型（内容哈希绑定信任，yolo 不自动信任，fail closed） | 新 hooks.rs + AgentEvent 挂点 + 设置 UI | L | 先写 spec；信任模型逐字参照 ZCode hooks.md |
| G45 | 会话级回归评测：外部启动器跑案例集（prompt→期望工具序列/产物断言），证据归档（git SHA/版本/失败现场） | 仓库外脚本 + headless 入口 | M | 责任边界参照 ZCode usage/13：LFTTDO 只当执行器，隔离（HOME/数据目录）与评分归外部 |
| — | 安全加固债 5 项（JobObject 孙进程回收 / experiment_run 接命令段规则 / websearch 复用 fetch_guard SSRF client / permission_rules 非法形状 UI 提示 / 非 Exec 工具 scope 补全） | 见 lfttdo-project-status | S×5 | 不阻塞，穿插做 |
| — | ipc.rs 拆分（~1.9 万行单文件） | 按 G31/G32/G33 式分模块 | L | 做 G44 前顺手拆 |

## 已明确不做 / 暂不做

- 事件溯源改造（messages 表为真相 + 投影已满足需求， rewind 用 DELETE 尾部行即可）
- btw 旁路提问、会话分叉 fork、定时自动化、bot 渠道集成
- 任务栏徽标 overlay icon（Windows 需 ITaskbarList3 额外验证，toast 先行）
