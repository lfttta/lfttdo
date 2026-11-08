import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import type { SessionRow } from "../types";

// R128：线上键是 `updated_at`（db.rs:239-246 无 rename_all）。这一支 fixture 故意写成
// 「数组顺序 = 插入顺序、updated_at = 相反」，这样「忘了排序」和「按 updatedAt(undefined)
// 排序 ⇒ 比较函数恒假、顺序碰巧」两种坏法都会红。（写成 `updatedAt` 的话 TS 直接报错，
// 因为镜像里没有那个键 —— 这正是补镜像的意义。）
const rows: SessionRow[] = [
  { id: "old", title: "昨天那条", updated_at: 1_700_000_000_000 },
  { id: "new", title: "刚才那条", updated_at: 1_700_000_999_999 },
  { id: "mid", title: "中午那条", updated_at: 1_700_000_500_000 },
];

// 每一行的 testid 都带 id（`session-row-<id>`），排序断言只看**文档顺序**，
// 所以这里必须用正则取全量再按 `data-session-id` 还原成 id 序列。
const order = (): string[] =>
  screen
    .getAllByTestId(/session-row-/)
    .map((li) => li.getAttribute("data-session-id") as string);

function setup(over: Partial<Parameters<typeof Sidebar>[0]> = {}) {
  const props = {
    rows,
    activeId: "new",
    onSelect: vi.fn(),
    onCreate: vi.fn(),
    onDelete: vi.fn(),
    onRename: vi.fn(),
    onExport: vi.fn(),
    onOpenSkills: vi.fn(),
    ...over,
  };
  render(<Sidebar {...props} />);
  return props;
}

describe("Sidebar", () => {
  it("orders sessions by updated_at descending, not by arrival order", () => {
    setup();
    expect(order()).toEqual(["new", "mid", "old"]);
  });

  it("keeps the ordering stable when the list arrives sorted the other way", () => {
    setup({ rows: [...rows].reverse() });
    expect(order()).toEqual(["new", "mid", "old"]);
  });

  it("marks the active session and hands the id over on click", () => {
    const props = setup();
    const active = screen.getByTestId("session-row-new");
    expect(active).toHaveAttribute("data-active", "true");
    expect(screen.getByTestId("session-row-old")).toHaveAttribute(
      "data-active",
      "false",
    );
    fireEvent.click(screen.getByRole("button", { name: /昨天那条/ }));
    expect(props.onSelect).toHaveBeenCalledWith("old");
  });

  it("creates a session through the button only", () => {
    const props = setup();
    fireEvent.click(screen.getByRole("button", { name: /新建对话/ }));
    expect(props.onCreate).toHaveBeenCalled();
  });

  // 要求 3 的同一道红线在侧栏这一侧：改名必须是行内 <input>。
  it("renames through an inline input", () => {
    const props = setup();
    expect(screen.queryByRole("textbox")).toBeNull();
    const row = screen.getByTestId("session-row-old");
    fireEvent.click(within(row).getByRole("button", { name: /重命名/ }));
    const box = within(row).getByRole("textbox") as HTMLInputElement;
    expect(box.value).toBe("昨天那条");
    fireEvent.change(box, { target: { value: "改名了" } });
    fireEvent.click(within(row).getByRole("button", { name: /提交重命名/ }));
    expect(props.onRename).toHaveBeenCalledWith("old", "改名了");
  });

  it("does not rename to a blank title", () => {
    const props = setup();
    const row = screen.getByTestId("session-row-old");
    fireEvent.click(within(row).getByRole("button", { name: /重命名/ }));
    fireEvent.change(
      within(row).getByRole("textbox"),
      { target: { value: "   " } },
    );
    fireEvent.click(within(row).getByRole("button", { name: /提交重命名/ }));
    expect(props.onRename).not.toHaveBeenCalled();
  });

  // G38：导出走行内按钮，把会话 id 交给 App（对话框与写文件都在 Rust 侧）。
  // 顺手钉一条：导出会收枪 —— 上膛的删除确认不该因为点了导出还挂在那一行。
  it("exports a session through its row button and disarms the armed deletion", () => {
    const props = setup();
    const row = screen.getByTestId("session-row-old");
    fireEvent.click(within(row).getByRole("button", { name: /删除/ }));
    expect(
      within(screen.getByTestId("session-row-old")).getByTestId(
        "delete-confirm-old",
      ),
    ).toBeInTheDocument();
    fireEvent.click(within(row).getByRole("button", { name: /导出/ }));
    expect(props.onExport).toHaveBeenCalledWith("old");
    expect(
      within(screen.getByTestId("session-row-old")).queryByTestId(
        "delete-confirm-old",
      ),
    ).toBeNull();
  });

  // 两段式确认（2026-11-08）：删除不可恢复，第一击只「上膛」——浮出确认条，
  // 第二击才把 id 交给 onDelete。每条用例的名字用**全等字符串**而不是 /删除/：
  // 上膛之后行内同时存在「删除」和「确认删除」，正则会撞成两个匹配。
  it("arms deletion first and deletes only on the second click", () => {
    const props = setup();
    const target = screen.getByTestId("session-row-mid");
    fireEvent.click(within(target).getByRole("button", { name: "删除" }));
    expect(props.onDelete).not.toHaveBeenCalled();
    expect(screen.getByTestId("delete-confirm-mid")).toHaveTextContent(
      "删除会话？",
    );
    fireEvent.click(
      within(screen.getByTestId("delete-confirm-mid")).getByRole("button", {
        name: "确认删除",
      }),
    );
    expect(props.onDelete).toHaveBeenCalledWith("mid");
    // 确认之后确认条收起：一行不会永远挂着一条已经用完的确认。
    expect(screen.queryByTestId("delete-confirm-mid")).toBeNull();
  });

  it("cancel aborts the armed deletion without deleting", () => {
    const props = setup();
    const target = screen.getByTestId("session-row-mid");
    fireEvent.click(within(target).getByRole("button", { name: "删除" }));
    fireEvent.click(
      within(screen.getByTestId("delete-confirm-mid")).getByRole("button", {
        name: "取消",
      }),
    );
    expect(props.onDelete).not.toHaveBeenCalled();
    expect(screen.queryByTestId("delete-confirm-mid")).toBeNull();
  });

  it("re-arms the confirmation to the latest row when another delete is clicked", () => {
    const props = setup();
    fireEvent.click(
      within(screen.getByTestId("session-row-old")).getByRole("button", {
        name: "删除",
      }),
    );
    fireEvent.click(
      within(screen.getByTestId("session-row-new")).getByRole("button", {
        name: "删除",
      }),
    );
    expect(screen.queryByTestId("delete-confirm-old")).toBeNull();
    fireEvent.click(
      within(screen.getByTestId("delete-confirm-new")).getByRole("button", {
        name: "确认删除",
      }),
    );
    expect(props.onDelete).toHaveBeenCalledTimes(1);
    expect(props.onDelete).toHaveBeenCalledWith("new");
  });

  it("disarms the armed deletion on Escape", () => {
    const props = setup();
    fireEvent.click(
      within(screen.getByTestId("session-row-mid")).getByRole("button", {
        name: "删除",
      }),
    );
    fireEvent.keyDown(screen.getByRole("navigation"), { key: "Escape" });
    expect(props.onDelete).not.toHaveBeenCalled();
    expect(screen.queryByTestId("delete-confirm-mid")).toBeNull();
  });

  it("opens the skills panel from the footer", () => {
    const props = setup();
    fireEvent.click(screen.getByRole("button", { name: "技能" }));
    expect(props.onOpenSkills).toHaveBeenCalled();
  });
});
