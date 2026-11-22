// 个人外观调整：在所选皮肤之上叠加颜色、字号与间距变量（ADR-0014）。
// 设置存在 ui.skin 里，只接受这里列出的变量，不接受任意 CSS。这个文件只引用 skins.ts，
// tests/appearance.test.mjs 直接读取它。

import { normalizeSkin, skins, type SkinId } from "./skins";

/** 可以改的颜色。一项可以对应几个皮肤变量：本方同时用于不知道本机队伍时的 Alpha。 */
export const colorSlots = [
  { key: "accent", name: "强调色", vars: ["--bax-accent"], usage: "选中项、主按钮、链接与滑块" },
  { key: "friend", name: "本方 / Alpha", vars: ["--bax-friend", "--bax-alpha"], usage: "名单、战报与图表里的本方" },
  { key: "foe", name: "对方 / Bravo", vars: ["--bax-foe", "--bax-bravo"], usage: "名单、战报与图表里的对方" },
  { key: "good", name: "胜利", vars: ["--bax-good"], usage: "胜利标签、加分与成功提示" },
  { key: "bad", name: "失败", vars: ["--bax-bad"], usage: "失败标签、减分与错误提示" },
] as const;

export type ColorKey = (typeof colorSlots)[number]["key"];
export type ColorOverrides = Partial<Record<ColorKey, string>>;

/** 间距档位：主窗口的面板内边距、面板之间与列表行的空白按系数缩放，控件本身大小不变。 */
export const densities = [
  { id: "compact", name: "紧凑", factor: 0.75 },
  { id: "standard", name: "标准", factor: 1 },
  { id: "comfortable", name: "宽松", factor: 1.25 },
] as const;

export type Density = (typeof densities)[number]["id"];

/** 主窗口字号倍数的范围与步长 */
export const fontScaleRange = { min: 0.85, max: 1.3, step: 0.05 } as const;

export interface Appearance {
  /** 皮肤 ID，见 skins.ts；不认识的值按默认皮肤处理 */
  id: SkinId;
  /** 各皮肤上改过的颜色，键是皮肤 ID；没改的颜色不存，跟随皮肤 */
  colors: Partial<Record<SkinId, ColorOverrides>>;
  /** 主窗口字号倍数 */
  fontScale: number;
  /** 主窗口间距 */
  density: Density;
}

export const defaultAppearance: Appearance = { id: "default", colors: {}, fontScale: 1, density: "standard" };

/** 由个人调整控制的变量。应用时先清掉不在本次结果里的，换皮肤或恢复默认后不留旧值。 */
export const managedVars: readonly string[] = [
  ...new Set([...colorSlots.flatMap((s) => s.vars), "--bax-on-accent", "--bax-font-scale", "--bax-density"]),
];

const isObject = (value: unknown): value is Record<string, unknown> =>
  value != null && typeof value === "object" && !Array.isArray(value);

/** #rgb 或 #rrggbb，统一成小写 #rrggbb；其他写法不接受。 */
export function normalizeHex(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value.trim());
  if (!m) return null;
  const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return `#${hex.toLowerCase()}`;
}

function normalizeColors(value: unknown): ColorOverrides {
  if (!isObject(value)) return {};
  const out: ColorOverrides = {};
  for (const slot of colorSlots) {
    const hex = normalizeHex(value[slot.key]);
    if (hex) out[slot.key] = hex;
  }
  return out;
}

export function clampFontScale(value: unknown): number {
  const n = typeof value === "number" ? value : Number.NaN;
  if (!Number.isFinite(n)) return 1;
  const { min, max } = fontScaleRange;
  return Math.round(Math.min(max, Math.max(min, n)) * 100) / 100;
}

/** 设置里存的值可能来自旧版本或者被手改过：缺的、不认识的都按默认处理。 */
export function normalizeAppearance(value: unknown): Appearance {
  if (!isObject(value)) return { ...defaultAppearance, colors: {} };
  const colors: Appearance["colors"] = {};
  if (isObject(value.colors)) {
    for (const { id } of skins) {
      const own = normalizeColors(value.colors[id]);
      if (Object.keys(own).length) colors[id] = own;
    }
  }
  return {
    id: normalizeSkin(value.id),
    colors,
    fontScale: clampFontScale(value.fontScale),
    density: densities.some((d) => d.id === value.density) ? (value.density as Density) : "standard",
  };
}

export function densityFactor(density: Density): number {
  return densities.find((d) => d.id === density)?.factor ?? 1;
}

/** 一个皮肤上改过的颜色对应的变量。改了强调色时，主按钮文字按新颜色选黑或白。 */
export function colorVars(colors: ColorOverrides | undefined): Record<string, string> {
  const vars: Record<string, string> = {};
  if (!colors) return vars;
  for (const slot of colorSlots) {
    const hex = colors[slot.key];
    if (!hex) continue;
    for (const name of slot.vars) vars[name] = hex;
  }
  if (colors.accent) vars["--bax-on-accent"] = onAccent(colors.accent);
  return vars;
}

/** 当前皮肤要叠加的全部变量。 */
export function appearanceVars(appearance: Appearance): Record<string, string> {
  return {
    ...colorVars(appearance.colors[appearance.id]),
    "--bax-font-scale": String(appearance.fontScale),
    "--bax-density": String(densityFactor(appearance.density)),
  };
}

export function isDefaultAppearance(appearance: Appearance): boolean {
  return (
    appearance.fontScale === 1 &&
    appearance.density === "standard" &&
    Object.keys(appearance.colors[appearance.id] ?? {}).length === 0
  );
}

// ---------------------------------------------------------------- 颜色计算

type Rgba = [number, number, number, number];

/** 皮肤变量里出现的颜色写法：#hex、rgb()/rgba()、以及 --bax-bg-rgb 那样的 RGB 三元组。 */
export function parseCssColor(value: string): Rgba | null {
  const text = value.trim();
  let m = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(text);
  if (m) {
    const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
    const [r, g, b, a = 255] = hex.match(/../g)!.map((h) => parseInt(h, 16));
    return [r, g, b, a / 255];
  }
  m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/i.exec(text);
  if (m) {
    const alpha = m[4] == null ? 1 : m[4].endsWith("%") ? Number(m[4].slice(0, -1)) / 100 : Number(m[4]);
    return [Number(m[1]), Number(m[2]), Number(m[3]), alpha];
  }
  m = /^([\d.]+)\s+([\d.]+)\s+([\d.]+)$/.exec(text);
  if (m) return [Number(m[1]), Number(m[2]), Number(m[3]), 1];
  return null;
}

export function toHex(color: Rgba): string {
  return `#${color.slice(0, 3).map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0")).join("")}`;
}

const over = (top: Rgba, bottom: Rgba): Rgba => [
  top[0] * top[3] + bottom[0] * (1 - top[3]),
  top[1] * top[3] + bottom[1] * (1 - top[3]),
  top[2] * top[3] + bottom[2] * (1 - top[3]),
  1,
];

function luminance([r, g, b]: Rgba): number {
  const channel = (v: number) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG 对比度。 */
export function contrastRatio(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** 主按钮文字：黑白里对比度高的一个。任何底色都至少有 4.58:1。 */
export function onAccent(accent: string): string {
  const color = parseCssColor(accent);
  if (!color) return "#000000";
  return contrastRatio(color, [0, 0, 0, 1]) >= contrastRatio(color, [255, 255, 255, 1]) ? "#000000" : "#ffffff";
}

/**
 * 颜色作为文字放在底色、面板、面板 2 上的最低对比度。面板可以半透明，按底色 → 面板 → 面板 2 叠起来算。
 * 任何一个颜色读不出来时返回 null。
 */
export function worstContrast(color: string, grounds: { bg: string; surface: string; surface2: string }): number | null {
  const fg = parseCssColor(color);
  const bgRaw = parseCssColor(grounds.bg);
  const surfaceRaw = parseCssColor(grounds.surface);
  const surface2Raw = parseCssColor(grounds.surface2);
  if (!fg || !bgRaw || !surfaceRaw || !surface2Raw) return null;
  const bg = over(bgRaw, [0, 0, 0, 1]);
  const surface = over(surfaceRaw, bg);
  const surface2 = over(surface2Raw, surface);
  return Math.min(...[bg, surface, surface2].map((ground) => contrastRatio(over(fg, ground), ground)));
}
