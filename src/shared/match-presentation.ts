import type { MatchView } from "./bindings/MatchView";
import type { LogStatus } from "./bindings/LogStatus";
import type { MatchReportView } from "./bindings/MatchReportView";
import type { RosterRow } from "./roster";

export const matchModeNames = { unknown: "对局", ranked: "排位", custom: "自定义", pve: "PvE" };

export function logStatus(log?: LogStatus) {
  if (!log) return { text: "正在连接 BAX…", needsDirectory: false };
  if (log.error) return { text: `日志读取失败：${log.error}`, needsDirectory: true };
  if (log.searching) return { text: "正在自动定位游戏日志…", needsDirectory: false };
  if (!log.dir) return { text: "未找到 GameLogs 目录，BAX 会继续自动查找。", needsDirectory: true };
  if (!log.ready) return { text: "已定位目录，正在读取日志…", needsDirectory: false };
  if (!log.file) return { text: "目录中还没有游戏日志，等待游戏写入。", needsDirectory: true };
  return { text: "日志已就绪", needsDirectory: false };
}

export function rosterWaitingText(match: MatchView) {
  if (match.mode === "pve") return "PvE · 赛前识别已暂停，等待游戏日志提供名单。";
  const status = match.pregame.status.trim();
  if (match.pregame.players.length && status) return status;
  return status ? `${status}。等待游戏日志提供正式名单。` : "等待游戏日志提供正式名单。";
}

export function matchSpecs(player: RosterRow, match: MatchView | null, report: MatchReportView | null) {
  if (player.team === "spectators") return [];
  if (match?.phase === "ended") {
    return report?.analysis.matchId === match.matchId && player.id != null ? report.specs[player.id] ?? [] : [];
  }
  return player.profile?.lastSpecs ?? [];
}

export function showOverlaySpecs(match: MatchView | null) {
  return match?.phase === "ended" || (match?.phase === "waitingRoom" && match.mode !== "pve");
}

export function rosterSummary(match: MatchView | null, report: MatchReportView | null = null) {
  if (!match?.rosterKnown) return "";
  const count = (team: string) => match.players.filter(p => p.team === team).length;
  const spectators = count("spectators");
  const mode = report && report.analysis.matchId === match.matchId ? report.mode : match.mode;
  return `${matchModeNames[mode]} · ${count("alpha")} v ${count("bravo")}${mode === "pve" ? " 真人 · OCR 已暂停" : ""}${spectators ? ` · 旁观 ${spectators} 人` : ""}`;
}
