import { useEffect, useRef, useState } from "react";
import {
  hooksList,
  hooksOpenFolder,
  hooksSetTrusted,
  hooksSetWorkspaceEnabled,
} from "../ipc/commands";
import type { HooksView } from "../types";

interface HooksPanelProps {
  /** 当前会话（决定「工作区是哪个」）。null = 没有选中会话。 */
  sessionId?: string | null;
  /** 测试注入：省一次 IPC；生产不传，面板自己拉。 */
  initial?: HooksView;
  onClose: () => void;
  load?: (sessionId?: string) => Promise<HooksView>;
  openFolder?: (sessionId?: string, workspace?: boolean) => Promise<void>;
  setTrusted?: (digest: string, trusted: boolean) => Promise<void>;
  setWorkspaceEnabled?: (enabled: boolean) => Promise<void>;
}

/**
 * G44/G44+ hooks 面板。列表每次打开/刷新都从磁盘现读（`hooks_list`），信任状态由
 * Rust 落盘。**这里没有"总开关"可以替代逐条信任**：hook 是任意代码执行，
 * 信任绑定的是「这条声明的当前内容」（digest）—— 改一个字符就回到未信任。
 *
 * 为什么不做行内编辑：hooks.json 的字段（args 数组、matcher 通配）用表单编辑
 * 要发明第二套校验，写坏了还可能让「以为生效的 hook 其实没跑」。这一版只做
 * 「看 + 信任/撤销 + 打开文件」，配置文件本身仍是唯一真相。
 *
 * **工作区来源（G44+）是两把锁**：总开关 + 逐条 digest 信任。克隆仓库自带的
 * `.lfttdo/hooks.json` 是本机制里最高风险的输入面，所以总开关默认关，且它在界面上
 * 是一个要自己按下去的独立动作 —— 不是一个能顺手勾上的小方框。
 */
export function HooksPanel({
  sessionId = null,
  initial,
  onClose,
  load = hooksList,
  openFolder = hooksOpenFolder,
  setTrusted = hooksSetTrusted,
  setWorkspaceEnabled = hooksSetWorkspaceEnabled,
}: HooksPanelProps) {
  const [view, setView] = useState<HooksView | null>(initial ?? null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);

  const refresh = async () => {
    setBusy(true);
    try {
      setView(await load(sessionId ?? undefined));
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

  const open = async (workspace = false) => {
    setBusy(true);
    try {
      await openFolder(sessionId ?? undefined, workspace);
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
      setView(await load(sessionId ?? undefined));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  /** 工作区总开关：单独的、需要确认语义的一步（不是顺手勾的方框）。 */
  const toggleWorkspace = async (enabled: boolean) => {
    if (busyRef.current) return;
    if (enabled) {
      const ok = window.confirm(
        "打开工作区级 hooks？\n\n当前会话的工作文件夹里 .lfttdo/hooks.json 的脚本会被执行。" +
          "克隆来的仓库可能自带这份文件 —— 请确认你信任这个工作文件夹的内容。\n\n" +
          "打开之后仍然要逐条信任每一条声明。",
      );
      if (!ok) return;
    }
    busyRef.current = true;
    setBusy(true);
    try {
      await setWorkspaceEnabled(enabled);
      setView(await load(sessionId ?? undefined));
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  const userHooks = view?.hooks.filter((h) => h.scope !== "workspace") ?? [];
  const workspaceHooks = view?.hooks.filter((h) => h.scope === "workspace") ?? [];

  const renderHook = (
    h: NonNullable<HooksView["hooks"]>[number],
    keyPrefix: string,
  ) => (
    <fieldset className="panel-group" key={`${keyPrefix}-${h.digest}`}>
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
        <label htmlFor={`hook-trust-${keyPrefix}-${h.digest}`}>信任</label>
        <input
          id={`hook-trust-${keyPrefix}-${h.digest}`}
          type="checkbox"
          aria-label={`信任 ${h.event} ${h.command}`}
          checked={h.trusted}
          disabled={busy}
          onChange={(e) => void toggle(h.digest, e.target.checked)}
        />
      </div>
    </fieldset>
  );

  return (
    <div className="panel-backdrop" onClick={onClose}>
      <section
        className="panel"
        aria-label="Hooks"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="panel-title">Hooks</h2>
        <p className="panel-note">
          hook 是挂在你机器上的脚本：工具调用前可以拦下（deny + 理由）、调用后可以补一段
          上下文、会话开始/结束也能通知。因为是任意代码执行，<strong>每条都要你亲自信任</strong>
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

            <h3 className="panel-subtitle">用户作用域</h3>
            {userHooks.length === 0 ? (
              <p className="panel-note">
                还没有 hook。在配置文件里按
                {" "}
                <code>{`{"enabled": true, "events": {"PreToolUse": [{"matcher": "run_command", "command": "node", "args": ["D:/hooks/audit.js"]}]}}`}</code>
                {" "}
                的形状加一条，回到这里点「刷新」再勾选信任。
              </p>
            ) : (
              userHooks.map((h) => renderHook(h, "user"))
            )}

            <h3 className="panel-subtitle">工作区级</h3>
            {view.workspace === null ? (
              <p className="panel-note">
                当前会话没有工作文件夹，所以没有工作区级 hooks。
              </p>
            ) : (
              <>
                <p className="panel-note">
                  来自 <code>{`${view.workspace}\\.lfttdo\\hooks.json`}</code>。
                  克隆来的仓库可能自带这份文件，所以它是一个<strong>独立开关</strong>：
                  打开后仍然要逐条信任。
                </p>
                <div className="panel-row">
                  {/* 不用 `<label htmlFor>` 指向 button：label 会覆盖 button 的
                      可访问名（按钮会变成叫「启用工作区级 hooks（当前：未启用）」），
                      屏幕阅读器与测试都会找不到「启用」这个动作。 */}
                  <p className="panel-note" id="hook-workspace-switch-note">
                    启用工作区级 hooks（当前：
                    {view.workspace_enabled ? "已启用" : "未启用"}）
                  </p>
                  <button
                    type="button"
                    disabled={busy}
                    aria-describedby="hook-workspace-switch-note"
                    onClick={() => void toggleWorkspace(!view.workspace_enabled)}
                  >
                    {view.workspace_enabled ? "关闭" : "启用"}
                  </button>
                </div>
                {view.workspace_enabled ? (
                  workspaceHooks.length === 0 ? (
                    <p className="panel-note">
                      这个工作区还没有 hooks。在 <code>.lfttdo/hooks.json</code> 里加一条，
                      回来刷新后勾选信任。
                    </p>
                  ) : (
                    workspaceHooks.map((h) => renderHook(h, "ws"))
                  )
                ) : null}
              </>
            )}
          </>
        )}
        <div className="panel-actions">
          <button type="button" disabled={busy} onClick={() => void open(false)}>
            打开数据目录
          </button>
          {view?.workspace ? (
            <button type="button" disabled={busy} onClick={() => void open(true)}>
              打开工作区目录
            </button>
          ) : null}
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
