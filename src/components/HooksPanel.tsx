import { useEffect, useRef, useState } from "react";
import { hooksList, hooksOpenFolder, hooksSetTrusted } from "../ipc/commands";
import type { HooksView } from "../types";

interface HooksPanelProps {
  /** 测试注入：省一次 IPC；生产不传，面板自己拉。 */
  initial?: HooksView;
  onClose: () => void;
  load?: () => Promise<HooksView>;
  openFolder?: () => Promise<void>;
  setTrusted?: (digest: string, trusted: boolean) => Promise<void>;
}

/**
 * G44 hooks 面板。列表每次打开/刷新都从磁盘现读（`hooks_list`），信任状态由
 * Rust 落盘。**这里没有"总开关"可以替代逐条信任**：hook 是任意代码执行，
 * 信任绑定的是「这条声明的当前内容」（digest）—— 改一个字符就回到未信任。
 *
 * 为什么不做行内编辑：hooks.json 的字段（args 数组、matcher 通配）用表单编辑
 * 要发明第二套校验，写坏了还可能让「以为生效的 hook 其实没跑」。这一版只做
 * 「看 + 信任/撤销 + 打开文件」，配置文件本身仍是唯一真相。
 */
export function HooksPanel({
  initial,
  onClose,
  load = hooksList,
  openFolder = hooksOpenFolder,
  setTrusted = hooksSetTrusted,
}: HooksPanelProps) {
  const [view, setView] = useState<HooksView | null>(initial ?? null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  const refresh = async () => {
    setBusy(true);
    try {
      setView(await load());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (initial === undefined) void refresh();
    // 只在挂载时拉一次；之后的刷新由按钮触发。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const open = async () => {
    setBusy(true);
    try {
      await openFolder();
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (digest: string, trusted: boolean) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    try {
      await setTrusted(digest, trusted);
      setView(await load());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  return (
    <div className="panel-backdrop" onClick={onClose}>
      <section
        className="panel"
        aria-label="Hooks"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="panel-title">Hooks</h2>
        <p className="panel-note">
          hook 是挂在你机器上的脚本：工具调用前可以拦下（deny + 理由），调用后可以补一段
          上下文，会话开始/结束也能通知。因为是任意代码执行，<strong>每条都要你亲自信任</strong>
          —— 信任绑定的是这条声明的当前内容（内容一改就回到未信任），权限模式里的
          「信任模式」不会自动信任 hook。
        </p>
        <p className="panel-note">
          hook 不是安全策略：它自己也可能有 bug，权限门与路径校验仍然是防线。
        </p>
        {error !== null ? (
          <p className="panel-note" role="alert">
            {error}
          </p>
        ) : null}
        {view === null ? (
          error === null ? <p className="panel-note">正在读取 hooks 配置…</p> : null
        ) : (
          <>
            <p className="panel-note">
              配置文件：<code>{view.config_path}</code>
              {view.enabled ? "" : "（当前未启用：hooks.json 里 enabled 不是 true）"}
            </p>
            {view.config_ok ? null : (
              <p className="panel-note" role="alert">
                这份 hooks.json 解析不了，当前一条 hook 都不会执行。改好文件后点「刷新」。
              </p>
            )}
            {view.hooks.length === 0 ? (
              <p className="panel-note">
                还没有 hook。在配置文件里按
                {" "}
                <code>{`{"enabled": true, "events": {"PreToolUse": [{"matcher": "run_command", "command": "node", "args": ["D:/hooks/audit.js"]}]}}`}</code>
                {" "}
                的形状加一条，回到这里点「刷新」再勾选信任。
              </p>
            ) : (
              view.hooks.map((h) => (
                <fieldset className="panel-group" key={h.digest}>
                  <legend>
                    {h.event}
                    {h.trusted ? "" : "（未信任）"}
                  </legend>
                  <p className="panel-row panel-readonly">
                    {h.command}
                    {h.args.length > 0 ? ` ${h.args.join(" ")}` : ""}
                  </p>
                  <p className="panel-note">
                    匹配：{h.matcher || "全部工具"} · 超时 {h.timeout_ms}ms
                  </p>
                  <p className="panel-note">摘要：{h.digest.slice(0, 16)}…</p>
                  <div className="panel-row">
                    <label htmlFor={`hook-trust-${h.digest}`}>信任</label>
                    <input
                      id={`hook-trust-${h.digest}`}
                      type="checkbox"
                      aria-label={`信任 ${h.event} ${h.command}`}
                      checked={h.trusted}
                      disabled={busy}
                      onChange={(e) => void toggle(h.digest, e.target.checked)}
                    />
                  </div>
                </fieldset>
              ))
            )}
          </>
        )}
        <div className="panel-actions">
          <button type="button" disabled={busy} onClick={() => void open()}>
            打开数据目录
          </button>
          <button type="button" disabled={busy} onClick={() => void refresh()}>
            刷新
          </button>
          <button type="button" onClick={onClose}>
            关闭
          </button>
        </div>
      </section>
    </div>
  );
}
