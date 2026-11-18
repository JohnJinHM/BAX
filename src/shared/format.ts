import type { ErrorKind } from "./bindings/ErrorKind";
import type { FetchState } from "./bindings/FetchState";
import type { Phase } from "./bindings/Phase";
import type { Team } from "./bindings/Team";
import type { UnitClass } from "./bindings/UnitClass";

/** 毫秒 → m:ss，负数带负号。 */
export function mmss(ms: number): string {
  const total = Math.floor(Math.abs(ms) / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${ms < 0 ? "-" : ""}${minutes}:${String(seconds).padStart(2, "0")}`;
}

/** 解析 m:ss 或 mm:ss，前面可以带负号；无法解析时返回 null。 */
export function parseMmss(text: string): number | null {
  const match = /^\s*(-)?(\d{1,3}):([0-5]\d)\s*$/.exec(text);
  if (!match) return null;
  const value = (Number(match[2]) * 60 + Number(match[3])) * 1000;
  return match[1] ? -value : value;
}

export const phaseNames: Record<Phase, string> = {
  waitingRoom: "等待室",
  loading: "加载中",
  deployment: "准备阶段",
  inBattle: "对局中",
  ended: "已结束",
};

export const teamNames: Record<Team, string> = {
  alpha: "Alpha",
  bravo: "Bravo",
  spectators: "观战",
};

export const errorNames: Record<ErrorKind, string> = {
  timeout: "超时",
  network: "网络错误",
  rateLimited: "被限流",
  rejected: "被拒绝",
  notFound: "未收录",
  server: "站点出错",
  decode: "解析失败",
  config: "配置错误",
};

export function fetchText(state: FetchState): string {
  switch (state.state) {
    case "idle":
      return "";
    case "queued":
      return "排队";
    case "fetching":
      return "查询中";
    case "failed":
      return errorNames[state.kind];
  }
}

export function rating(value: number | null | undefined): string {
  return value == null ? "–" : String(Math.round(value));
}

export function percent(value: number | null | undefined): string {
  return value == null ? "–" : `${Math.round(value * 100)}%`;
}

export function ratio(value: number | null | undefined): string {
  return value == null ? "–" : value.toFixed(2);
}

export function minutes(value: number | null | undefined): string {
  return value == null ? "–" : `${Math.round(value)} 分`;
}

/** 距今多久，粗略到分钟、小时、天。 */
export function ago(at: number | null | undefined, now: number): string {
  if (at == null) return "从未";
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 60) return "刚刚";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟前`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} 小时前`;
  return `${Math.floor(seconds / 86400)} 天前`;
}

export const unitClassNames: Record<UnitClass, string> = {
  recon: "侦察",
  infantry: "步兵",
  tank: "坦克",
  ifv: "步战与反坦克车",
  transport: "运输车",
  supply: "补给",
  airDefense: "防空",
  artillery: "火炮",
  helicopter: "直升机",
  jet: "固定翼",
  other: "其他",
};

/**
 * 兵种构成图的分组与配色。八个色位按固定顺序取用（dataviz 参考色板的暗色档，
 * 已对 #161b22 底色验证相邻对），运输、补给与其他并作灰色。
 */
export const compositionGroups: { key: string; name: string; color: string; classes: UnitClass[] }[] = [
  { key: "tank", name: "坦克", color: "#3987e5", classes: ["tank"] },
  { key: "ifv", name: "步战与反坦克车", color: "#d95926", classes: ["ifv"] },
  { key: "infantry", name: "步兵", color: "#199e70", classes: ["infantry"] },
  { key: "recon", name: "侦察", color: "#c98500", classes: ["recon"] },
  { key: "artillery", name: "火炮", color: "#d55181", classes: ["artillery"] },
  { key: "airDefense", name: "防空", color: "#0ca30c", classes: ["airDefense"] },
  { key: "helicopter", name: "直升机", color: "#9085e9", classes: ["helicopter"] },
  { key: "jet", name: "固定翼", color: "#e66767", classes: ["jet"] },
  { key: "logistics", name: "运输、补给与其他", color: "#5b636d", classes: ["transport", "supply", "other"] },
];

/**
 * 出兵时间线（散点）的分组。散点里任意两组都可能相邻，参考色板只有前三个色位能两两区分，
 * 所以只分三组加灰色，并用形状作第二种编码。
 */
export const sortieGroups: { key: string; name: string; color: string; symbol: string; classes: UnitClass[] }[] = [
  { key: "ground", name: "地面作战", color: "#3987e5", symbol: "circle", classes: ["tank", "ifv", "infantry", "recon"] },
  { key: "support", name: "火炮与防空", color: "#d95926", symbol: "triangle", classes: ["artillery", "airDefense"] },
  { key: "air", name: "空中", color: "#199e70", symbol: "diamond", classes: ["helicopter", "jet"] },
  { key: "logistics", name: "运输、补给与其他", color: "#8b949e", symbol: "rect", classes: ["transport", "supply", "other"] },
];

/** 整数加千位分隔；缺失为 –。 */
export function count(value: number | null | undefined): string {
  return value == null ? "–" : Math.round(value).toLocaleString("en-US");
}
