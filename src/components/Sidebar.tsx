import { useState } from "react";
import type { SessionRow } from "../types";

interface SidebarProps {
  /** 收起态：只留一条 56px 窄栏（R337）。 */
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  onOpenSettings?: () => void;
  onOpenMcp?: () => void;
  onOpenSkills?: () => void;
  /** G44：hooks 面板入口。 */
  onOpenHooks?: () => void;
  rows: SessionRow[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
  onRename: (id: string, title: string) => void;
  /** G38：导出该会话为 Markdown（Rust 侧弹保存对话框；取消由 App 静默）。 */
  onExport: (id: string) => void;
}

/**
 * 会话侧栏。
 *
 * 排序键必须是 `updated_at`（R128：`db.rs:239-246` 的 `SessionRow` 没有 `rename_all`，
 * 线上就是 snake_case）。写成 `updatedAt` 的话这里读到 `undefined`，`b.updatedAt - a.updatedAt`
 * 全是 `NaN`，比较函数恒假 ⇒ 顺序变成「碰巧」而**一声不响**，所以 `Sidebar.test.tsx` 里
 * 那两条顺序断言是这一支的唯一守卫。`[...rows].sort` 而不是 `rows.sort`：不许改调用方的数组。
 *
 * `updated_at` 是毫秒（R72），前端只拿它排序，不格式化、不显示 —— 没有消费者就不许编一份本地时间。
 */
export function Sidebar({
  collapsed = false,
  onToggleCollapsed,
  onOpenSettings,
  onOpenMcp,
  onOpenSkills,
  onOpenHooks,
  rows,
  activeId,
  onSelect,
  onCreate,
  onDelete,
  onRename,
  onExport,
}: SidebarProps) {
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  /**
   * 删除的两段式确认：`armedId` 是当前「上膛」的那一行（同一时刻最多一行）。
   * 设置面板里「清除已存的 key → 确认清除/取消」已经是这个模式；会话删除同样不可恢复，
   * 一键直删的代价是一次误点，而「点删除 → 浮出确认条 → 再点确认」只多一次点击。
   * 上膛状态在任何「换了主意」的动作上收枪：点取消、按 Esc、点别的行（选中/重命名/
   * 删除另一行都会重置）—— 收枪必须便宜，否则用户会为了逃避误删而害怕点删除。
   */
  const [armedId, setArmedId] = useState<string | null>(null);
  const sorted = [...rows].sort((a, b) => b.updated_at - a.updated_at);

  const submitRename = (id: string) => {
    const title = draft.trim();
    // 空标题不许提交：那是把侧栏那一行变成点不动的空白，而 Rust 侧只会照单收下。
    if (title !== "") onRename(id, title);
    setEditing(null);
    setDraft("");
  };

  return (
    <nav
      className="sidebar"
      aria-label="会话列表"
      data-collapsed={collapsed}
      onKeyDown={(e) => {
        if (e.key === "Escape") setArmedId(null);
      }}
    >
      <div className="sidebar-head">
        <button
          type="button"
          className="sidebar-toggle"
          aria-label={collapsed ? "展开侧栏" : "收起侧栏"}
          onClick={onToggleCollapsed}
        >
          ☰
        </button>
        {collapsed ? (
          <button
            type="button"
            className="sidebar-plus"
            aria-label="新建对话"
            onClick={onCreate}
          >
            ＋
          </button>
        ) : (
          <button type="button" className="new-session" onClick={onCreate}>
            新建对话
          </button>
        )}
      </div>
      <ul className="session-list">
        {sorted.map((r) => (
          <li
            key={r.id}
            className="session-item"
            data-testid={`session-row-${r.id}`}
            data-session-id={r.id}
            data-active={r.id === activeId}
          >
            <button
              type="button"
              className="session-title"
              onClick={() => {
                setArmedId(null);
                onSelect(r.id);
              }}
            >
              {r.title}
            </button>
            <button
              type="button"
              className="session-rename"
              onClick={() => {
                setArmedId(null);
                setEditing(r.id);
                setDraft(r.title);
              }}
            >
              重命名
            </button>
            <button
              type="button"
              className="session-export"
              onClick={() => {
                setArmedId(null);
                onExport(r.id);
              }}
            >
              导出
            </button>
            <button
              type="button"
              className="session-delete"
              onClick={() => setArmedId(r.id)}
            >
              删除
            </button>
            {editing === r.id ? (
              <div className="rename-form">
                <input
                  aria-label={`重命名 ${r.title}`}
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                />
                <button
                  type="button"
                  onClick={() => submitRename(r.id)}
                >
                  提交重命名
                </button>
              </div>
            ) : null}
            {armedId === r.id ? (
              <div
                className="delete-confirm"
                data-testid={`delete-confirm-${r.id}`}
              >
                <span className="delete-confirm-text">删除会话？</span>
                <button
                  type="button"
                  className="session-delete-armed"
                  onClick={() => {
                    setArmedId(null);
                    onDelete(r.id);
                  }}
                >
                  确认删除
                </button>
                <button
                  type="button"
                  className="session-delete-cancel"
                  onClick={() => setArmedId(null)}
                >
                  取消
                </button>
              </div>
            ) : null}
          </li>
        ))}
      </ul>
      <div className="sidebar-foot">
        <button
          type="button"
          className="sidebar-action"
          aria-label="设置"
          onClick={onOpenSettings}
        >
          ⚙{collapsed ? null : " 设置"}
        </button>
        <button
          type="button"
          className="sidebar-action"
          aria-label="技能"
          onClick={onOpenSkills}
        >
          技能
        </button>
        <button
          type="button"
          className="sidebar-action"
          aria-label="MCP"
          onClick={onOpenMcp}
        >
          MCP
        </button>
        <button
          type="button"
          className="sidebar-action"
          aria-label="Hooks"
          onClick={onOpenHooks}
        >
          Hooks
        </button>
      </div>
    </nav>
  );
}
