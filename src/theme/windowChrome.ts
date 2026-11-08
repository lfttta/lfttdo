import { invoke } from "@tauri-apps/api/core";

/**
 * 标题栏的精确着色（2026-11-08，用户的「跟界面一个颜色」）。
 *
 * 背景：标题栏是**系统**画的，三层 CSS（tokens/presets/glass）都管不到它；
 * Rust 的 `.theme()` 只能给明暗两档系统灰，跟主题色永远差着一截。Windows 11
 * 的 DWM caption color 允许上任意色值，落点在 Rust 的 `set_caption_color` ——
 * 色值由这里从**计算样式**现读现递：tokens/presets 仍是唯一真相，本模块不抄
 * 第二份色表。
 *
 * 取哪一支：玻璃模式下 `--bg-surface` 是半透明 `rgb()`，DWM 上不了透明色；
 * 玻璃界面可见的实底是 html 背景里渐变下面那层 `--app-base`，所以优先取它，
 * 没有玻璃层的预设（未来若删掉玻璃）回落 `--bg-surface`。两支都解析不出
 * 六位十六进制（比如全是 rgb()）就整单放弃 —— 染一支谁也没选过的颜色，
 * 比不染更糟。
 */
export function pickChromeColors(
  style: CSSStyleDeclaration,
): { caption: string; text: string } | null {
  const hex = (value: string) => /^#[0-9a-fA-F]{6}$/.test(value.trim());
  const caption = ["--app-base", "--bg-surface"]
    .map((name) => style.getPropertyValue(name).trim())
    .find(hex);
  const text = style.getPropertyValue("--text-primary").trim();
  if (caption === undefined || !hex(text)) return null;
  return { caption, text };
}

/**
 * 把当前主题的标题栏颜色同步给系统。三个入口共用这一份：
 * - `main.tsx` 首帧播种后（缓存里的主题，标题栏从第一帧就不是系统灰）；
 * - `App.applySettingsTheme`（权威的 settings_get 回来后、以及每次保存后）。
 * `__TAURI_INTERNALS__` 不在（浏览器直开、jsdom 测试）时整体 no-op：invoke
 * 没得调，也不该调 —— 这层在测试环境里必须免费。
 * 失败静默吞是**有意的**：Win10（< 22000）没有这两个 DWM 属性，DwmSetWindowAttribute
 * 每次都会失败，为它走 notice 通道等于让那批机器永远挂着一条假报错；标题栏的
 * 明暗已经由 Rust 启动时的 `.theme()` 兜住，这里的精确色是锦上添花的一层。
 */
export function syncWindowChrome(): void {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const colors = pickChromeColors(getComputedStyle(document.documentElement));
  if (colors === null) return;
  void invoke("set_caption_color", colors).catch(() => {});
}
