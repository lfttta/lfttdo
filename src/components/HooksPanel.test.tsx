import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { HooksPanel } from "./HooksPanel";
import type { HooksView } from "../types";

const view = (over: Partial<HooksView> = {}): HooksView => ({
  config_path: "C:\\data\\hooks.json",
  enabled: true,
  config_ok: true,
  hooks: [
    {
      event: "PreToolUse",
      matcher: "run_command",
      command: "node",
      args: ["D:/hooks/audit.js"],
      timeout_ms: 5000,
      digest: "abcdef0123456789abcdef0123456789",
      trusted: false,
    },
  ],
  ...over,
});

describe("HooksPanel", () => {
  // G44：列表从 load 现拉；未信任的 hook 显示「（未信任）」且复选框未勾选。
  it("G44: lists hooks with their trust state", async () => {
    const load = vi.fn(async () => view());
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(await screen.findByText("PreToolUse（未信任）")).toBeInTheDocument();
    expect(screen.getByText("node D:/hooks/audit.js")).toBeInTheDocument();
    expect(screen.getByLabelText("信任 PreToolUse node")).not.toBeChecked();
  });

  // G44：勾选信任 → 调 setTrusted(digest, true) 并重拉列表（状态由 Rust 现读）。
  it("G44: trusting a hook calls the command with its digest and refreshes", async () => {
    const load = vi.fn(async () => view());
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
    const load = vi.fn(async () => view({ config_ok: false, hooks: [] }));
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(
      await screen.findByText(/这份 hooks\.json 解析不了/),
    ).toBeInTheDocument();
  });

  // G44：enabled 不是 true 时点名「当前未启用」，且空列表给出可照抄的形状。
  it("G44: a disabled config says so and shows an example shape", async () => {
    const load = vi.fn(async () => view({ enabled: false, hooks: [] }));
    render(<HooksPanel onClose={() => {}} load={load} />);
    expect(await screen.findByText(/当前未启用/)).toBeInTheDocument();
    expect(screen.getByText(/PreToolUse/)).toBeInTheDocument();
  });
});
