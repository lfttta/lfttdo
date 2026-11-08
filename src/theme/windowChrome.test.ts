import { describe, expect, it } from "vitest";
import { pickChromeColors } from "./windowChrome";

// jsdom 没有 getComputedStyle 的自定义属性可给，这里的被测面只是**挑选逻辑**：
// 给一份假的 CSSStyleDeclaration（原型上没有 getPropertyValue 也没关系，
// 我们传的就是一个带该方法的普通对象）。染色本身（DWM）在 Rust 侧。
const styleOf = (props: Record<string, string>) =>
  ({
    getPropertyValue: (name: string) => props[name] ?? "",
  }) as unknown as CSSStyleDeclaration;

describe("pickChromeColors", () => {
  // 玻璃模式：--bg-surface 是半透明 rgb()（DWM 上不了色），可见实底是 --app-base。
  it("prefers the opaque glass base over the translucent surface", () => {
    const colors = pickChromeColors(
      styleOf({
        "--app-base": "#1a120c",
        "--bg-surface": "rgb(255 255 255 / 8%)",
        "--text-primary": "#f0e4d6",
      }),
    );
    expect(colors).toEqual({ caption: "#1a120c", text: "#f0e4d6" });
  });

  // 没有玻璃层的预设：回落到不透明的 --bg-surface。
  it("falls back to --bg-surface when there is no glass base", () => {
    const colors = pickChromeColors(
      styleOf({
        "--bg-surface": "#151517",
        "--text-primary": "#e8e6e3",
      }),
    );
    expect(colors).toEqual({ caption: "#151517", text: "#e8e6e3" });
  });

  // 两支底色候选全是 rgb()（或缺失）时整单放弃：染一支谁也没选过的颜色比不染更糟。
  it("gives up entirely when no candidate parses as opaque hex", () => {
    expect(
      pickChromeColors(
        styleOf({
          "--bg-surface": "rgb(255 255 255 / 8%)",
          "--text-primary": "#f2f5ff",
        }),
      ),
    ).toBeNull();
  });

  // 文字色不合法同样放弃：深底配一条读不清的标题比系统默认的灰更差。
  it("gives up when the text color is not opaque hex", () => {
    expect(
      pickChromeColors(
        styleOf({
          "--app-base": "#1a120c",
          "--text-primary": "color(srgb 0.5 0.5 0.5)",
        }),
      ),
    ).toBeNull();
  });
});
