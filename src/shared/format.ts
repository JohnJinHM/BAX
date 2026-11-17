import type { ErrorKind } from "./bindings/ErrorKind";
import type { FetchState } from "./bindings/FetchState";
import type { Phase } from "./bindings/Phase";
import type { Team } from "./bindings/Team";

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
