import type { UnitClass } from "../shared/bindings/UnitClass";

/** 没有专用素材的类别用通用车辆图标加色条、文字。 */
export const unitTypes: Record<UnitClass, { file?: string; tag?: string; color?: string }> = {
  recon: { file: "recon.png" },
  infantry: { file: "infantry.png" },
  tank: { file: "tank.png" },
  airDefense: { file: "air-defense.png" },
  artillery: { file: "artillery.png" },
  helicopter: { file: "helicopter.png" },
  jet: { file: "jet.png" },
  supply: { file: "supply.png" },
  ifv: { file: "vehicle.png", tag: "步战", color: "#ed9868" },
  transport: { file: "vehicle.png", tag: "运输", color: "#b4becb" },
  other: { tag: "其他", color: "#b4becb" },
};

export const unitTypeWidth = (kind: UnitClass) => unitTypes[kind].tag ? 48 : 24;
