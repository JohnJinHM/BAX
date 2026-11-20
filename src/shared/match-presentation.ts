import type { MatchView } from "./bindings/MatchView";
import type { MatchReportView } from "./bindings/MatchReportView";
import type { RosterRow } from "./roster";

export const matchModeNames = { unknown: "对局", ranked: "排位", custom: "自定义", pve: "PvE" };

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
  if (!match?.rosterKnown) return match?.mode === "pve" ? "PvE · OCR 已暂停" : "";
  const count = (team: string) => match.players.filter(p => p.team === team).length;
  const spectators = count("spectators");
  const mode = report && report.analysis.matchId === match.matchId ? report.mode : match.mode;
  return `${matchModeNames[mode]} · ${count("alpha")} v ${count("bravo")}${mode === "pve" ? " 真人 · OCR 已暂停" : ""}${spectators ? ` · 旁观 ${spectators} 人` : ""}`;
}
