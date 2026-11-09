import { useEffect, useRef, useState } from "react";
import { MessageBubble } from "./MessageBubble";
import { QuestionCard } from "./QuestionCard";
import type { Decision, ErrorWhere, UiState } from "../types";

/** 距底多少像素以内算「还想跟着看」：太高会误粘、太低会在一次小增量后脱落。 */
const NEAR_BOTTOM_PX = 40;

/**
 * G42：重放/长会话的 turns 可达上百条，`state.turns.map` 全量渲染会把 DOM 撑到
 * 卡顿（MessageList.test 里「重放一整段历史可以上百条」正是现状）。只渲染最近
 * `VISIBLE_TURNS` 条，更早的收进顶部一个「展开」按钮 —— reducer 仍持有全部，
 * 这**纯是渲染层裁剪**，不碰状态层。40 与滑窗默认值（context.rs `window_messages`
 * 默认 40 轮）同量级纯属巧合：一个是上下文预算、一个是渲染预算，互不依赖。
 * 展开后不提供「收起」：历史只会越聊越多，收起没有第二回价值。
 */
const VISIBLE_TURNS = 40;

/** `ErrorWhere`（types.rs 镜像 events.rs:103-111）只用来把「哪儿炸了」说成人话。 */
const WHERE_LABEL: Record<ErrorWhere, string> = {
  model: "模型端点",
  tool: "工具",
  mcp: "MCP",
  loop: "对话循环",
  store: "本地存储",
};

/**
 * 要求 11 = T14 评审 F5 的交接：`notices` **只增不减**，重放一整段历史可以上百条
 * （每个失败工具、每次压缩、每条非致命 error 都 push 一条），reducer 那边保持一份真相，
 * 折叠/截断全归渲染层。所以这里：
 *   ① 连续重复的合并成一条并显示 `×n`；
 *   ② 折叠后仍超过 `COLLAPSED_MAX` 条时只露最近的那几条，其余收进「展开」；
 *   ③ 不许每条都渲染成独立横幅。
 */
// export：App 侧的 appNotices 渲染共用同一份「合并 ×n + 超上限折叠」逻辑 ——
// 两条通知通道若是各写一份，「×n」的口径迟早漂成两个。
export const COLLAPSED_MAX = 5;

interface Group {
  text: string;
  count: number;
}

export function groupRuns(notices: string[]): Group[] {
  const out: Group[] = [];
  for (const n of notices) {
    const last = out[out.length - 1];
    if (last && last.text === n) last.count += 1;
    else out.push({ text: n, count: 1 });
  }
  return out;
}

function NoticeStrip({ notices }: { notices: string[] }) {
  const [open, setOpen] = useState(false);
  if (notices.length === 0) return null;
  const groups = groupRuns(notices);
  const collapsed = !open && groups.length > COLLAPSED_MAX;
  const shown = collapsed ? groups.slice(groups.length - COLLAPSED_MAX) : groups;
  const hidden = groups.length - shown.length;
  return (
    <ul className="notices" data-testid="notice-strip">
      {collapsed ? (
        <li className="notice-more">
          <button type="button" onClick={() => setOpen(true)}>
            展开（另有 {hidden} 条）
          </button>
        </li>
      ) : null}
      {open && groups.length > COLLAPSED_MAX ? (
        <li className="notice-more">
          <button type="button" onClick={() => setOpen(false)}>
            收起
          </button>
        </li>
      ) : null}
      {shown.map((g, i) => (
        <li className="notice" data-testid="notice" key={`${g.text}-${i}`}>
          {g.text}
          {g.count > 1 ? <span className="notice-count"> ×{g.count}</span> : null}
        </li>
      ))}
    </ul>
  );
}

/**
 * 要求 1：只收 `{ state, onRespond }`，**没有任何判断风险的 prop**。
 * 风险徽章归 `MessageBubble` 画，判定权在 Rust 的 PermissionGate（R114）。
 */
export function MessageList({
  state,
  onRespond,
  onOpenLink,
  onRevealPath,
  onAnswerQuestion,
  onRetryLast,
  onFollowUp,
}: {
  state: UiState;
  onRespond: (id: string, d: Decision) => void;
  /** Markdown 链接的出口（App 接 `open_url`）；测试与纯展示场景可以不传。 */
  onOpenLink?: (url: string) => void;
  onRevealPath?: (path: string) => void;
  /**
   * G20 提问面板的应答出口（App 接 `question_respond`）；测试与纯展示场景可以不传。
   * 面板排在消息流末尾：提问发生在一次工具执行的**中途**，模型此刻就停在这道题上，
   * 它是时间线上最新的一件事。
   */
  onAnswerQuestion?: (id: string, choice: string) => void;
  /** G39：「重试最后一轮」的出口（App 接截断→重放→重发链）；缺省 = 按钮不渲染。 */
  onRetryLast?: () => void;
  /** G40：render_html 页面回流的出口（App 接 send）；缺省 = 确认条不渲染发送钮。 */
  onFollowUp?: (prompt: string) => void;
}) {
  const lastTurn = state.turns.length - 1;

  /**
   * G42：折叠态是本组件的**渲染偏好**，不进 UiState —— reducer 是事件折叠的唯一
   * 地方，渲染裁剪混进去就会跟着事件流一起被重放（重放一个折叠偏好没有意义）。
   * 切换会话时的重置由 `App` 给本组件挂 `key={activeId}` 完成（重放本来就换整套
   * turns，重挂的代价是零）。
   */
  const [expanded, setExpanded] = useState(false);
  const hiddenTurns = expanded ? 0 : Math.max(0, state.turns.length - VISIBLE_TURNS);
  const visibleTurns =
    hiddenTurns > 0 ? state.turns.slice(state.turns.length - VISIBLE_TURNS) : state.turns;

  /**
   * 自动滚底：贴底时随新内容走，用户上翻之后**不再抢**滚动位置；
   * 回到底部附近（`NEAR_BOTTOM_PX` 内）重新黏住。
   * 判据用「距底距离」而不是「scrollTop 等于某值」：流式时内容高度一直在变。
   */
  const listRef = useRef<HTMLDivElement>(null);
  const stickRef = useRef(true);
  const onScroll = () => {
    const el = listRef.current;
    if (!el) return;
    stickRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX;
  };
  // 无依赖：每次渲染（含流式增量）之后都检查一次；不贴底就什么都不做。
  useEffect(() => {
    const el = listRef.current;
    if (el && stickRef.current) el.scrollTop = el.scrollHeight;
  });

  return (
    <div
      className="message-list"
      data-testid="message-list"
      ref={listRef}
      onScroll={onScroll}
    >
      {/* 致命横幅排在所有东西之前：用户先看见「这一轮已经废了」，再读气泡。 */}
      {state.fatal ? (
        <div className="fatal" role="alert">
          <strong>致命错误（{WHERE_LABEL[state.fatal.where]}）</strong>
          <span>{state.fatal.message}</span>
        </div>
      ) : null}
      <NoticeStrip notices={state.notices} />
      {hiddenTurns > 0 ? (
        <button
          type="button"
          className="turns-expand"
          data-testid="turns-expand"
          onClick={() => setExpanded(true)}
        >
          展开更早的 {hiddenTurns} 条
        </button>
      ) : null}
      <div className="turns" data-testid="bubble-list">
        {visibleTurns.map((t, i) => (
          <MessageBubble
            key={t.id}
            turn={t}
            // slice 之后 i 是局部索引，「是不是最后一轮」要按全量下标算：
            // 差了这个偏移，折叠状态下最后一条回答拿不到 isLastTurn（复制 chip
            // 那一族行为跟着失灵）。
            isLastTurn={i + hiddenTurns === lastTurn}
            streaming={state.streaming}
            onRespond={onRespond}
            onOpenLink={onOpenLink}
            onRevealPath={onRevealPath}
            onRetry={onRetryLast}
            onFollowUp={onFollowUp}
          />
        ))}
      </div>
      {/* G20：`key` 用题的 id —— 单槽被新问覆盖时强制重挂载，用户给旧题打的草稿
          不许残留到一道不同的题上。 */}
      {state.pendingQuestion && onAnswerQuestion ? (
        <div className="question-region">
          <QuestionCard
            key={state.pendingQuestion.id}
            pending={state.pendingQuestion}
            onAnswer={onAnswerQuestion}
          />
        </div>
      ) : null}
    </div>
  );
}
