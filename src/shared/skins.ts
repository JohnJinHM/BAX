// 内置皮肤目录（ADR-0014）。经典皮肤就是 theme.css 本身，其余每个皮肤对应 src/shared/skins/<id>.css
// 里的一组变量。这个文件不引用其他模块，tests/skins.test.mjs 直接读取它。

export interface SkinInfo {
  id: string;
  name: string;
  description: string;
}

export const skins = [
  { id: "broken-arrow", name: "断箭", description: "炭灰底色、黄色强调与直角细线面板，参照游戏 HUD。" },
  { id: "default", name: "经典", description: "深蓝灰底色、蓝色强调与圆角面板。" },
] as const satisfies readonly SkinInfo[];

export type SkinId = (typeof skins)[number]["id"];

export const defaultSkin: SkinId = "broken-arrow";

/** 设置里存的值不认识（比如皮肤被移除）时退回默认皮肤。 */
export function normalizeSkin(value: unknown): SkinId {
  return skins.some((s) => s.id === value) ? (value as SkinId) : defaultSkin;
}
