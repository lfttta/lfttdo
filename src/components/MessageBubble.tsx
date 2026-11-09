import { memo, useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import Markdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import remarkGfm from "remark-gfm";
import type {
  Decision,
  PermCard,
  Risk,
  ToolCallView,
  ToolCard,
  Turn,
  UiBlock,
} from "../types";

/** `risk` 只用来画徽章，不参与任何决策（R114：判定权在 Rust 的 PermissionGate）。 */
const RISK_LABEL: Record<Risk, string> = { read: "只读", write: "写入", exec: "执行" };

const STATUS_LABEL: Record<ToolCard["status"], string> = {
  running: "执行中",
  ok: "完成",
  error: "失败",
};

/**
 * 第 9 条 = 硬性要求 8：`ms === 0` 来自 `db.rs:370` 的「库里没记这一列」，重放路径恒为 0。
 * 把它显示成「0 ms」是替用户编一次测量。reducer 原样透传（R115），归位到界面上就是这个破折号。
 */
const fmtMs = (ms: number): string => (ms === 0 ? "—" : `${ms} ms`);

/**
 * 预览里那行「未能定界出正文…」是**给模型**的护栏（防止它把样板当正文引用），
 * 不是给用户看的报错。界面只显示正文：把这一行从展示文本里摘掉；
 * 如果整段预览就只有这行提示，预览区直接不渲染。
 */
export function readablePreview(text: string): string {
  const NL = String.fromCharCode(10);
  return text
    .split(NL)
    .filter((line) => !line.trimStart().startsWith("[未能定界出正文"))
    .join(NL)
    .trim();
}

/** 复制反馈在屏上停留的毫秒数：读得清「已复制」三个字就够了，不占第二处注意力。 */
const COPY_FEEDBACK_MS = 1_500;

/**
 * 复制 chip 的共用实现（气泡右上角与代码块右上角两个落点）。
 * 成败都必须出声（R195 的口径：静默丢反馈等于界面说谎），但两者都只值一句瞬时文案：
 * 成功「已复制」、失败「复制失败」，1.5 秒后回到「复制」。计时器卸载时清掉 ——
 * 留着的 setTimeout 会在一个已经不存在的组件上 setState（App 的 memoryToastTimer
 * 同一条纪律，症状是测试之间互相串）。
 * `navigator.clipboard` 在非安全上下文里不存在：`?.` 短路后按「失败」处理并出声，
 * 不许静默 —— 用户以为复制了、粘出来是别的东西，比报错更糟。
 * `getText` 是**点击那一刻**才取的取数函数：代码块从 DOM 读 `textContent`，
 * 引用循环里造的闭包不影响 memo 化的外层（这一层是独立组件，状态自成一体）。
 */
function CopyButton({ getText, testId }: { getText: () => string; testId: string }) {
  const [state, setState] = useState<"idle" | "done" | "fail">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
        timer.current = null;
      }
    },
    [],
  );
  const copy = useCallback(() => {
    const settled = (ok: boolean) => setState(ok ? "done" : "fail");
    const promise = navigator.clipboard?.writeText(getText());
    if (promise === undefined) settled(false);
    else promise.then(() => settled(true), () => settled(false));
    // 连续第二次点击重置计时：上一次的「已复制」不该把这一次的存活时间一起带走。
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setState("idle");
      timer.current = null;
    }, COPY_FEEDBACK_MS);
  }, [getText]);
  return (
    <button type="button" className="copy-chip" data-testid={testId} onClick={copy}>
      {state === "done" ? "已复制" : state === "fail" ? "复制失败" : "复制"}
    </button>
  );
}

/**
 * 代码块的复制按钮挂在 `pre` 的包装层（`.md-code`）上，按钮从 DOM 读 `textContent`：
 * 高亮（rehype-highlight）把代码拆成一堆 span，`textContent` 是唯一「拿到的就是源码」
 * 的读法，不必去解析 markdown AST。包装层只加 `position: relative` 与这枚按钮，
 * `pre` 本体不动 —— `.md pre` 的样式与滚动行为都照旧。
 */
function CodeBlock({ children }: { children?: ReactNode }) {
  const ref = useRef<HTMLPreElement>(null);
  return (
    <div className="md-code">
      <pre ref={ref}>{children}</pre>
      <CopyButton
        testId="code-copy"
        getText={() => ref.current?.textContent ?? ""}
      />
    </div>
  );
}

function PathChip({
  path,
  onReveal,
}: {
  path: string;
  onReveal?: (path: string) => void;
}) {
  if (onReveal === undefined) {
    return (
      <span className="path-chip" title={path}>
        {path}
      </span>
    );
  }
  return (
    <button
      type="button"
      className="path-chip"
      title={`在资源管理器中显示：${path}`}
      onClick={() => onReveal(path)}
    >
      {path}
    </button>
  );
}

function toolPathOf(input: unknown): string | null {
  const o = asRecord(input);
  if (o === null || typeof o.path !== "string" || o.path.trim() === "") {
    return null;
  }
  return o.path;
}

function ToolCardView({
  card,
  streaming,
  onRevealPath,
  onFollowUp,
}: {
  card: ToolCard;
  streaming: boolean;
  onRevealPath?: (path: string) => void;
  onFollowUp?: (prompt: string) => void;
}) {
  const path = toolPathOf(card.input);
  // G32：render_html 有专属卡片 —— 原始 html 不再以 JSON 形式倾倒在 <pre> 里，
  // 改为沙箱 iframe 内嵌显示（输出文本里的落盘路径仍由下面的 preview 行展示）。
  const render = card.name === "render_html" ? renderInputOf(card.input) : null;
  return (
    <div
      className="card tool"
      data-testid={`tool-${card.callId}`}
      data-status={card.status}
    >
      <div className="card-head">
        <span className="tool-name">{card.name}</span>
        <span className={`badge risk-${card.risk}`}>{RISK_LABEL[card.risk]}</span>
        <span className="tool-status">{STATUS_LABEL[card.status]}</span>
        <span className="tool-ms">{fmtMs(card.ms)}</span>
      </div>
      {path === null ? null : (
        <PathChip path={path} onReveal={onRevealPath} />
      )}
      {render === null ? (
        <pre className="io">
          {JSON.stringify(card.input ?? {}, null, 2)}
        </pre>
      ) : (
        <RenderedHtmlCard
          html={render.html}
          title={render.title}
          streaming={streaming}
          onFollowUp={onFollowUp}
        />
      )}
      {(() => {
        const shown = card.preview ? readablePreview(card.preview) : "";
        return shown !== "" ? <pre className="io preview">{shown}</pre> : null;
      })()}
      {card.images.length > 0 ? (
        <div className="tool-images" data-testid={`tool-images-${card.callId}`}>
          {card.images.map((img, i) => (
            <figure className="tool-image" key={`${img.mime}-${i}`}>
              <img
                className="thumb"
                src={`data:image/${img.mime};base64,${img.data}`}
                alt="工具返回的图片"
              />
              {img.path === undefined ? null : (
                <PathChip path={img.path} onReveal={onRevealPath} />
              )}
            </figure>
          ))}
        </div>
      ) : null}
    </div>
  );
}

const DECISION_LABEL: Record<PermCard["state"], string> = {
  pending: "等待你的许可",
  allowed: "已放行",
  denied: "已拒绝",
};

type WriteInput = { path: string; content: string };
type EditInput = { path: string; old: string; new: string };

function asRecord(input: unknown): Record<string, unknown> | null {
  return typeof input === "object" && input !== null
    ? (input as Record<string, unknown>)
    : null;
}

function writeInputOf(input: unknown): WriteInput | null {
  const o = asRecord(input);
  if (o === null || typeof o.path !== "string" || typeof o.content !== "string") {
    return null;
  }
  return { path: o.path, content: o.content };
}

type RenderInput = { html: string; title: string | null };

/** G32：render_html 的入参守门 —— html 必须是非空字符串；title 缺席/空白归一成 null。 */
function renderInputOf(input: unknown): RenderInput | null {
  const o = asRecord(input);
  if (o === null || typeof o.html !== "string" || o.html.trim() === "") {
    return null;
  }
  const t = o.title;
  return {
    html: o.html,
    title: typeof t === "string" && t.trim() !== "" ? t : null,
  };
}

/**
 * G32/G40：render_html 的内嵌渲染。`sandbox="allow-scripts"`（无 allow-same-origin）
 * 是整个安全模型的落点：脚本可跑（交互式页面），但源是 opaque——读不到父页面、
 * localStorage，唯一出口是 `window.parent.postMessage`。data URL 让零配置的本地
 * 渲染成为可能（不启 asset 协议、不加 IPC 读回命令）；「放大」只改高度，不离开聊天流。
 *
 * G40 回流：页面可以用 `postMessage({type:"lfttdo:followup", prompt:"…"}, "*")` 请求
 * 把一句话送回对话。**不直接发**——模型生成的页面不可信（可能被诱导刷屏/注入话术），
 * 收到后先落成卡片内的确认条，由用户点了「发送」才走对话；来源校验钉死在本卡片的
 * iframe（`event.source`），别的窗口/iframe 冒充无效。超长截断到 2000 字符。
 */
const FOLLOWUP_MAX_CHARS = 2_000;

function RenderedHtmlCard({
  html,
  title,
  streaming,
  onFollowUp,
}: {
  html: string;
  title: string | null;
  streaming: boolean;
  onFollowUp?: (prompt: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const src = `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (e.source !== frameRef.current?.contentWindow) return;
      const data = e.data as { type?: unknown; prompt?: unknown } | null;
      if (
        data === null ||
        typeof data !== "object" ||
        data.type !== "lfttdo:followup" ||
        typeof data.prompt !== "string" ||
        data.prompt.trim() === ""
      ) {
        return;
      }
      setPendingPrompt(data.prompt.trim().slice(0, FOLLOWUP_MAX_CHARS));
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);
  return (
    <div className="rendered-html" data-testid="rendered-html">
      {title === null ? null : <div className="rendered-title">{title}</div>}
      <iframe
        ref={frameRef}
        className={expanded ? "rendered-frame expanded" : "rendered-frame"}
        sandbox="allow-scripts"
        src={src}
        title={title ?? "渲染结果"}
      />
      {pendingPrompt === null ? null : (
        <div className="followup-strip" data-testid="followup-strip">
          <span className="followup-text">页面请求发送：{pendingPrompt}</span>
          <button
            type="button"
            className="followup-send"
            disabled={streaming || !onFollowUp}
            data-testid="followup-send"
            onClick={() => {
              const prompt = pendingPrompt;
              setPendingPrompt(null);
              onFollowUp?.(prompt);
            }}
          >
            发送到对话
          </button>
          <button
            type="button"
            className="followup-dismiss"
            data-testid="followup-dismiss"
            onClick={() => setPendingPrompt(null)}
          >
            忽略
          </button>
        </div>
      )}
      <div className="rendered-foot">
        <button
          type="button"
          className="rendered-toggle"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? "收起" : "放大"}
        </button>
      </div>
    </div>
  );
}

function editInputOf(input: unknown): EditInput | null {
  const o = asRecord(input);
  if (
    o === null ||
    typeof o.path !== "string" ||
    typeof o.old_string !== "string" ||
    typeof o.new_string !== "string"
  ) {
    return null;
  }
  return { path: o.path, old: o.old_string, new: o.new_string };
}

function CallPreview({ call }: { call: ToolCallView }) {
  const write = call.name === "write_file" ? writeInputOf(call.input) : null;
  const edit = call.name === "edit_file" ? editInputOf(call.input) : null;

  if (write !== null) {
    return (
      <div className="perm-preview" data-testid="write-preview">
        <p className="perm-path">
          <span>路径</span>
          <code>{write.path}</code>
        </p>
        <p className="perm-note">完整内容（{write.content.length} 字符）</p>
        <pre className="io preview" tabIndex={0} aria-label="写入内容预览">
          {write.content}
        </pre>
      </div>
    );
  }
  if (edit !== null) {
    return (
      <div className="perm-preview" data-testid="edit-preview">
        <p className="perm-path">
          <span>路径</span>
          <code>{edit.path}</code>
        </p>
        <p className="perm-note">替换前</p>
        <pre className="io preview" tabIndex={0} aria-label="替换前内容">
          {edit.old}
        </pre>
        <p className="perm-note">替换后</p>
        <pre className="io preview" tabIndex={0} aria-label="替换后内容">
          {edit.new}
        </pre>
      </div>
    );
  }
  return <pre className="io">{JSON.stringify(call.input ?? {}, null, 2)}</pre>;
}

function PermCardView({
  card,
  onRespond,
}: {
  card: PermCard;
  onRespond: (id: string, d: Decision) => void;
}) {
  // 拒绝理由必须是**行内** input：第 8 条红线。`window.prompt` 在 WebView2 里返回 null，
  // 理由发不出去 ⇒ `respond` 收不到参数 ⇒ 那一笔永远 pending ⇒ 整轮静默挂住。
  // `setupTests.ts` 已经把三个弹窗函数换成一调用就抛，这里连提都不该提。
  const [denying, setDenying] = useState(false);
  const [reason, setReason] = useState("");
  const pending = card.state === "pending";

  const allow = (scope: "once" | "session") =>
    onRespond(card.id, { allow: true, scope });
  const submitDeny = () => {
    const trimmed = reason.trim();
    onRespond(card.id, { allow: false, reason: trimmed === "" ? "用户拒绝" : trimmed });
    setDenying(false);
    setReason("");
  };

  return (
    <div
      className="card perm"
      data-testid={`perm-${card.id}`}
      data-state={card.state}
    >
      <div className="card-head">
        <span className="perm-title">请求许可</span>
        <span className={`badge risk-${card.risk}`}>{RISK_LABEL[card.risk]}</span>
        <span className="perm-state">{DECISION_LABEL[card.state]}</span>
      </div>
      <div className="perm-call">
        <span className="tool-name">{card.call.name}</span>
        <CallPreview call={card.call} />
      </div>

      {card.state === "allowed" && card.scope ? (
        <p className="perm-note">
          {card.scope === "session" ? "本会话内都已放行" : "仅本次放行"}
        </p>
      ) : null}
      {card.state === "denied" ? (
        <p className="perm-note">理由：{card.reason ?? "用户拒绝"}</p>
      ) : null}

      <div className="perm-actions">
        <button type="button" disabled={!pending} onClick={() => allow("once")}>
          仅本次
        </button>
        <button
          type="button"
          disabled={!pending}
          onClick={() => allow("session")}
        >
          {card.risk === "write" ? "本会话内该目录都允许" : "本会话内都允许"}
        </button>
        <button
          type="button"
          disabled={!pending}
          onClick={() => setDenying((v) => !v)}
        >
          拒绝
        </button>
      </div>
      {denying && pending ? (
        <div className="deny-reason">
          <label htmlFor={`deny-${card.id}`}>拒绝理由（可留空）</label>
          <input
            id={`deny-${card.id}`}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
          <button type="button" onClick={submitDeny}>
            提交拒绝理由
          </button>
        </div>
      ) : null}
    </div>
  );
}

/**
 * 把 `<think>…</think>` 从正文里拆出来（R333）：Qwen3 系列把思考过程直接写在
 * content 里，原样渲染会在气泡里露出一大段 `<think>` + 推理文字，回答反而看不见。
 * 未闭合（流式进行中）时，余下全部算思考；结论先出、思考后到的模型也不会误判。
 */
export function splitThinking(text: string): { thinking: string | null; answer: string } {
  const open = text.indexOf("<think>");
  if (open < 0) return { thinking: null, answer: text };
  const rest = text.slice(open + "<think>".length);
  const close = rest.indexOf("</think>");
  const thinking = close < 0 ? rest : rest.slice(0, close);
  const after = close < 0 ? "" : rest.slice(close + "</think>".length);
  return { thinking, answer: (text.slice(0, open) + after).trim() };
}

function Thinking({ text, answer }: { text: string; answer: string }) {
  return (
    <details className="thinking" open={answer.trim() === ""}>
      <summary>思考过程</summary>
      <div className="thinking-body">{text}</div>
    </details>
  );
}

function BlockView({
  block,
  plain,
  streaming,
  onRespond,
  onOpenLink,
  onRevealPath,
  onFollowUp,
}: {
  block: UiBlock;
  plain: boolean;
  /** G40：透传给 render_html 卡片（流式中禁用「发送到对话」）。 */
  streaming: boolean;
  onRespond: (id: string, d: Decision) => void;
  onOpenLink?: (url: string) => void;
  onRevealPath?: (path: string) => void;
  onFollowUp?: (prompt: string) => void;
}) {
  switch (block.kind) {
    case "text": {
      // 要求 2：正在流式的那一块按纯文本渲染 —— 反复解析半截语法就是闪烁的来源。
      // R333：思考过程单独收进可折叠区，正文只留回答。
      const { thinking, answer } = splitThinking(block.text);
      return (
        <>
          {thinking !== null && thinking.trim() !== "" ? (
            <Thinking text={thinking} answer={answer} />
          ) : null}
          {plain ? (
            <pre className="plain">{answer}</pre>
          ) : (
            <div className="md">
              <Markdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeHighlight]}
                components={{
                  a: ({ href, children }) => (
                    <a
                      href={href}
                      rel="noreferrer noopener"
                      draggable={false}
                      onClick={(e) => {
                        // 不让 WebView 导航离开应用；能不能打开由 Rust 的
                        // `external_link::validate_external_url` 决定。
                        e.preventDefault();
                        if (href && /^https?:\/\//i.test(href)) {
                          onOpenLink?.(href);
                        }
                      }}
                      onAuxClick={(e) => {
                        // 中键/aux 点击同样不许开新窗口绕过校验。
                        e.preventDefault();
                        if (href && /^https?:\/\//i.test(href)) {
                          onOpenLink?.(href);
                        }
                      }}
                      onDragStart={(e) => e.preventDefault()}
                    >
                      {children}
                    </a>
                  ),
                  pre: CodeBlock,
                }}
              >
                {answer}
              </Markdown>
            </div>
          )}
        </>
      );
    }
    case "image":
      return (
        <img
          className="thumb"
          // 线上的 `mime` 是裸子类型（`"png"` / `"jpeg"`，见 ImageMime 的小写 serde 表示），
          // 不是完整 MIME。少写这个 `image/` 前缀产出的 `data:png;base64,…` 是非法 data URL，
          // 浏览器直接画不出来：症状是「发图那一条气泡里只有文字、重开也还是没图」，
          // 而待发缩略图（Composer 那份拼对了）一切正常，所以极易被误判成后端没存图。
          src={`data:image/${block.mime};base64,${block.data}`}
          alt="图片"
        />
      );
    case "tool":
      return (
        <ToolCardView
          card={block}
          streaming={streaming}
          onRevealPath={onRevealPath}
          onFollowUp={onFollowUp}
        />
      );
    case "perm":
      return <PermCardView card={block} onRespond={onRespond} />;
  }
}

/**
 * 这条轮次里可复制的文本：只有 text 块，且只取**回答**部分 —— 思考过程（`<think>`）
 * 在屏上收在折叠区里，复制出来的东西应该和用户读到的一致；工具卡/权限卡不是文本，
 * 不进剪贴板。多块按顺序用空行接起来。
 */
function copyTextOf(turn: Turn): string {
  return turn.blocks
    .filter((b): b is Extract<UiBlock, { kind: "text" }> => b.kind === "text")
    .map((b) => splitThinking(b.text).answer)
    .filter((s) => s.trim() !== "")
    .join("\n\n");
}

function MessageBubbleView({
  turn,
  isLastTurn,
  streaming,
  onRespond,
  onOpenLink,
  onRevealPath,
  onRetry,
  onFollowUp,
}: {
  turn: Turn;
  isLastTurn: boolean;
  streaming: boolean;
  onRespond: (id: string, d: Decision) => void;
  onOpenLink?: (url: string) => void;
  onRevealPath?: (path: string) => void;
  /**
   * G39：「重试这一轮」—— 截断最后一轮并重发原话，只在**最后一条** assistant
   * 气泡且非流式时出现（历史中间的轮次截了会把后面还没看完的内容一起删掉，
   * 那是另一个功能）。动作本身在 App：截断 → 重放 → 重发，这里只是一个按钮。
   */
  onRetry?: () => void;
  /** G40：render_html 页面的回流出口（确认条点了「发送」才到这里）。 */
  onFollowUp?: (prompt: string) => void;
}) {
  const lastIndexOf = turn.blocks.length - 1;
  // R234：空的助手气泡有两种来路 —— 等首字（该留着并看得出在等）与这一轮已结束
  // （401、中断，该收起）。样式分不出「空且还在跑」和「空且已经死」，所以在这里打标。
  const inFlight = streaming && isLastTurn && turn.role === "assistant";
  // 复制按钮只在「这一轮已经写完」的 assistant 气泡上出现：流式进行中复制到的是
  // 半截话，而且按钮悬在正在生长的气泡右上角会一直晃。等 `done` 再现身。
  // 空气泡没有可复制的东西，按钮也不渲染 —— 否则 `.bubble…:empty` 的两条
  // 隐藏/占位规则（R234）会被一个空按钮打破。
  const copyText = turn.role === "assistant" && !inFlight ? copyTextOf(turn) : "";
  return (
    <article
      className={`bubble bubble-${turn.role}`}
      data-testid={`bubble-${turn.role}`}
      data-role={turn.role}
      {...(inFlight ? { "data-streaming": "true" } : {})}
    >
      {turn.blocks.map((b, i) => (
        <BlockView
          key={i}
          block={b}
          plain={streaming && isLastTurn && i === lastIndexOf}
          streaming={streaming}
          onRespond={onRespond}
          onOpenLink={onOpenLink}
          onRevealPath={onRevealPath}
          onFollowUp={onFollowUp}
        />
      ))}
      {copyText === "" ? null : (
        <CopyButton testId="bubble-copy" getText={() => copyText} />
      )}
      {turn.role === "assistant" && !streaming && isLastTurn && onRetry ? (
        <button
          type="button"
          className="bubble-retry"
          data-testid="bubble-retry"
          onClick={onRetry}
        >
          ↻ 重试这一轮
        </button>
      ) : null}
    </article>
  );
}

/// 历史气泡不随流式帧重渲染：`turn` 对象在 reducer 里只有被改的那一轮才换身份，
/// `onRespond` 是稳定的 `useCallback`。没有这一层，每个 rAF 都会把全部历史
/// Markdown 重新解析一遍（长会话掉帧的主因）。
export const MessageBubble = memo(MessageBubbleView);
