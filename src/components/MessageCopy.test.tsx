// 复制 chip 的两条落点（2026-11-08）：assistant 气泡的「复制整条回答」与代码块的
// 「复制这段代码」。单独成文件而不是挤进 MessageList.test.tsx：那边的用例全在钉
// 「事件怎么变成界面」，这一支钉的是「剪贴板拿到的是什么」——依赖（navigator.clipboard
// 的桩）和断言口径都不同，混在一起两边都要互相迁就。
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MessageList } from "./MessageList";
import type { UiState } from "../types";

function state(partial: Partial<UiState>): UiState {
  return {
    turnSeq: 1,
    turns: [],
    streaming: false,
    usage: null,
    fatal: null,
    notices: [],
    toolIndex: {},
    permIndex: {},
    ...partial,
  };
}

const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  // jsdom 30 没有 clipboard：每次用例装一个桩，用例结束拆掉，别漏进下一份。
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
});

afterEach(() => {
  Reflect.deleteProperty(navigator, "clipboard");
});

const NL = String.fromCharCode(10);

describe("气泡复制", () => {
  it("copies the finished answer without the thinking trace", async () => {
    const s = state({
      turns: [
        {
          id: 1,
          role: "assistant",
          blocks: [
            {
              kind: "text",
              text: `<think>推理中的话${NL}不该进剪贴板</think>回答正文`,
            },
          ],
        },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    fireEvent.click(screen.getByTestId("bubble-copy"));
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith("回答正文");
    // 成功必须出声：按钮短暂换成「已复制」，不是悄悄什么都没说。
    await waitFor(() =>
      expect(screen.getByTestId("bubble-copy")).toHaveTextContent("已复制"),
    );
  });

  it("joins every text block of the turn in order", async () => {
    const s = state({
      turns: [
        {
          id: 1,
          role: "assistant",
          blocks: [
            { kind: "text", text: "第一段" },
            { kind: "text", text: "第二段" },
          ],
        },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    // writeText 的 promise 回来后还有一次 setState（「已复制」），包进 act：
    // 不然那个更新落在 act 之外，测试照过但控制台里每跑一次喊一次。
    await act(async () => {
      fireEvent.click(screen.getByTestId("bubble-copy"));
    });
    expect(writeText).toHaveBeenCalledWith(`第一段${NL}${NL}第二段`);
  });

  it("stays hidden while the answer is still streaming", () => {
    const s = state({
      streaming: true,
      turns: [
        { id: 1, role: "assistant", blocks: [{ kind: "text", text: "半截" }] },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    // 流式中的气泡复制到的是半截话，按钮等 `done` 再现身。
    expect(screen.queryByTestId("bubble-copy")).toBeNull();
  });

  it("offers no copy on a user bubble", () => {
    const s = state({
      turns: [
        { id: 1, role: "user", blocks: [{ kind: "text", text: "我自己的话" }] },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    expect(screen.queryByTestId("bubble-copy")).toBeNull();
  });

  it("says 复制失败 out loud when the clipboard rejects", async () => {
    writeText.mockRejectedValue(new Error("denied"));
    const s = state({
      turns: [
        { id: 1, role: "assistant", blocks: [{ kind: "text", text: "内容" }] },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    fireEvent.click(screen.getByTestId("bubble-copy"));
    await waitFor(() =>
      expect(screen.getByTestId("bubble-copy")).toHaveTextContent("复制失败"),
    );
  });
});

describe("代码块复制", () => {
  it("copies the raw code text out of the highlighted block", async () => {
    const s = state({
      turns: [
        {
          id: 1,
          role: "assistant",
          blocks: [
            {
              kind: "text",
              text: `看这段：${NL}${NL}\`\`\`js${NL}const a = 1;${NL}\`\`\``,
            },
          ],
        },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    // 高亮把代码拆成一堆 span，按钮从 DOM 的 textContent 读 —— 拿到的必须是源码本身
    // （含围栏块自带的那个换行），不是高亮后的富文本。
    await act(async () => {
      fireEvent.click(screen.getByTestId("code-copy"));
    });
    expect(writeText).toHaveBeenCalledWith(`const a = 1;${NL}`);
  });

  it("leaves code-free answers without a code copy chip", () => {
    const s = state({
      turns: [
        { id: 1, role: "assistant", blocks: [{ kind: "text", text: "没有代码" }] },
      ],
    });
    render(<MessageList state={s} onRespond={() => {}} />);
    expect(screen.queryByTestId("code-copy")).toBeNull();
  });
});
