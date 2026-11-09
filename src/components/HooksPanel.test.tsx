import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HooksPanel } from "./HooksPanel";
import type { HooksView } from "../types";

const view = (over: Partial<HooksView> = {}): HooksView => ({
  config_path: "C:\\data\\hooks.json",
  enabled: true,
  config_ok: true,
  workspace_enabled: false,
  workspace: null,
  hooks: [
    {
      event: "PreToolUse",
      matcher: "run_command",
      command: "node",
      args: ["D:/hooks/audit.js"],
      timeout_ms: 5000,
      digest: "abcdef0123456789abcdef0123456789",
      trusted: false,
      scope: "user",
    },
  ],
  ...over,
});

describe("HooksPanel", () => {
  // G44：列表从 load 现拉；未信任的 hook 显示「（未信任）」且复选框未勾选。
  it("G44: lists hooks with their trust state", async () => {
    const load = vi.fn(async (_sessionId?: string) => view());
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(await screen.findByText("PreToolUse（未信任）")).toBeInTheDocument();
    expect(screen.getByText("node D:/hooks/audit.js")).toBeInTheDocument();
    expect(screen.getByLabelText("信任 PreToolUse node")).not.toBeChecked();
  });

  // G44：勾选信任 → 调 setTrusted(digest, true) 并重拉列表（状态由 Rust 现读）。
  it("G44: trusting a hook calls the command with its digest and refreshes", async () => {
    const load = vi.fn(async (_sessionId?: string) => view());
    const setTrusted = vi.fn(async () => {});
    render(
      <HooksPanel onClose={() => {}} load={load} setTrusted={setTrusted} />,
    );
    const box = await screen.findByLabelText("信任 PreToolUse node");
    fireEvent.click(box);
    await waitFor(() =>
      expect(setTrusted).toHaveBeenCalledWith(
        "abcdef0123456789abcdef0123456789",
        true,
      ),
    );
    // 一次挂载拉一次 + 信任后重拉一次。
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
  });

  // G44：配置解析失败要明确提示「一条都不会执行」（不是静默空列表）——
  // 用户以为自己配好了却没生效，是这套机制最容易踩的坑。
  it("G44: an unparsable config shows a loud warning", async () => {
    const load = vi.fn(async (_sessionId?: string) => view({ config_ok: false, hooks: [] }));
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(
      await screen.findByText(/这份 hooks\.json 解析不了/),
    ).toBeInTheDocument();
  });

  // G44：enabled 不是 true 时点名「当前未启用」，且空列表给出可照抄的形状。
  it("G44: a disabled config says so and shows an example shape", async () => {
    const load = vi.fn(async (_sessionId?: string) => view({ enabled: false, hooks: [] }));
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(await screen.findByText(/当前未启用/)).toBeInTheDocument();
    expect(screen.getByText(/PreToolUse/)).toBeInTheDocument();
  });

  // G44+：没有工作文件夹的会话不显示工作区段落 —— 不谈一个不存在的来源。
  it("G44+: a session without a workspace shows no workspace section", async () => {
    const load = vi.fn(async (_sessionId?: string) => view());
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(
      await screen.findByText(/当前会话没有工作文件夹/),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText(/启用工作区级 hooks/)).toBeNull();
  });

  // G44+ 第一把锁：总开关关着时，即便工作区里声明了 hook 也**不展示**（更不执行）。
  // 展示一堆「未启用」的条目会让人以为勾一下信任就够了 —— 它还需要这个总开关。
  it("G44+: workspace hooks are invisible until the master switch is on", async () => {
    const load = vi.fn(async () =>
      view({
        workspace: "D:\\repo",
        workspace_enabled: false,
        hooks: [
          {
            event: "PreToolUse",
            matcher: "*",
            command: "evil.bat",
            args: [],
            timeout_ms: 5000,
            digest: "ws:1234:deadbeef",
            trusted: true, // 即便"已信任"也不该出现：来源本身没启用
            scope: "workspace",
          },
        ],
      }),
    );
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(
      await screen.findByText(/启用工作区级 hooks（当前：未启用）/),
    ).toBeInTheDocument();
    expect(screen.queryByText("evil.bat")).toBeNull();
  });

  // G44+：打开总开关要过一次确认（克隆仓库自带 hooks 是本机制最高风险的输入面）。
  it("G44+: enabling the workspace switch asks for confirmation first", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const setWorkspaceEnabled = vi.fn(async () => {});
    const load = vi.fn(async () =>
      view({ workspace: "D:\\repo", workspace_enabled: false }),
    );
    render(
      <HooksPanel
        onClose={() => {}}
        load={load}
        setWorkspaceEnabled={setWorkspaceEnabled}
      />,
    );
    const btn = await screen.findByRole("button", { name: "启用" });
    fireEvent.click(btn);
    expect(confirm).toHaveBeenCalled();
    // 用户点了取消 ⇒ 一步都不走（不写盘、不重拉）。
    await waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    expect(setWorkspaceEnabled).not.toHaveBeenCalled();
    confirm.mockRestore();
  });

  // G44+：确认之后才真的落盘，并重拉（新状态由 Rust 现读）。
  it("G44+: confirming the switch persists it and refreshes", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const calls: boolean[] = [];
    let enabled = false;
    const load = vi.fn(async (_sessionId?: string) =>
      view({
        workspace: "D:\\repo",
        workspace_enabled: enabled,
        hooks: enabled
          ? [
              {
                event: "PreToolUse",
                matcher: "*",
                command: "audit.bat",
                args: [],
                timeout_ms: 5000,
                digest: "ws:1234:cafe",
                trusted: false,
                scope: "workspace",
              },
            ]
          : [],
      }),
    );
    render(
      <HooksPanel
        onClose={() => {}}
        load={load}
        setWorkspaceEnabled={async (v: boolean) => {
          enabled = v;
          calls.push(v);
        }}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "启用" }));
    await waitFor(() => expect(calls).toEqual([true]));
    // 打开之后工作区条目才出现，且仍需逐条信任。
    expect(await screen.findByText("audit.bat")).toBeInTheDocument();
    expect(screen.getByLabelText("信任 PreToolUse audit.bat")).not.toBeChecked();
    confirm.mockRestore();
  });
});
