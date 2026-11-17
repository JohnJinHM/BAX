// 预览用的虚构快照：一场进行中的 5v5，玩家状态覆盖各种情况。只在 dev/ 预览页里使用。

import type { EncounterRow } from "../shared/bindings/EncounterRow";
import type { PlayerDetail } from "../shared/bindings/PlayerDetail";
import type { PlayerHit } from "../shared/bindings/PlayerHit";
import type { PlayerView } from "../shared/bindings/PlayerView";
import type { RatingPoint } from "../shared/bindings/RatingPoint";
import type { Snapshot } from "../shared/bindings/Snapshot";

const now = Date.now();

function player(
  id: number,
  name: string | null,
  team: "alpha" | "bravo",
  rating: number | null,
  extra: Partial<PlayerView> = {},
): PlayerView {
  return {
    id,
    name,
    logName: name,
    team,
    isSelf: false,
    profile:
      rating == null
        ? null
        : {
            name,
            level: 20 + (id % 70),
            rating,
            rank: 100 + id,
            ratingGames: 50 + (id % 400),
            ranked: {
              fights: 60 + (id % 400),
              wins: 30,
              losses: 28,
              leaves: id % 4,
              winRate: 0.42 + (id % 17) / 100,
              killLossRatio: 0.8 + (id % 9) / 10,
              costRatio: 0.9 + (id % 7) / 10,
              averageMinutes: 24 + (id % 11),
            },
            statsUpdatedAt: null,
          },
    profileFetchedAt: rating == null ? null : now - 30_000,
    profileCurrent: rating != null,
    fetch: { state: "idle" },
    encounters: { total: 0, asTeammate: 0, asOpponent: 0, firstAt: null, lastAt: null },
    note: null,
    ...extra,
  };
}

const players: PlayerView[] = [
  player(101, "Ash", "alpha", 2143),
  player(102, "Birch_the_Longest_Name_In_Lobby", "alpha", 1987, {
    encounters: { total: 3, asTeammate: 0, asOpponent: 3, firstAt: now - 9e8, lastAt: now - 4e7 },
  }),
  player(103, null, "alpha", null, { fetch: { state: "fetching" } }),
  player(104, "Cedar", "alpha", 1720, { profileCurrent: false, profileFetchedAt: now - 86_400_000 * 3 }),
  player(105, "示例 なまえ", "alpha", 2310),
  player(201, "Dune", "bravo", 2055, {
    encounters: { total: 41, asTeammate: 38, asOpponent: 3, firstAt: now - 9e9, lastAt: now - 9e5 },
    note: "常一起排，喜欢直升机开局",
  }),
  player(202, "Starling", "bravo", 2201, { isSelf: true }),
  player(203, "Elm", "bravo", 1899, { fetch: { state: "failed", kind: "timeout", retryAt: now + 12_000 } }),
  player(204, "Fern", "bravo", 2002),
  player(205, "Grove", "bravo", null, { fetch: { state: "queued" } }),
];

function average(team: "alpha" | "bravo") {
  const rated = players.filter((p) => p.team === team && p.profile?.rating != null);
  const sum = rated.reduce((s, p) => s + (p.profile?.rating ?? 0), 0);
  return {
    team,
    players: players.filter((p) => p.team === team).length,
    rated: rated.length,
    averageRating: rated.length ? sum / rated.length : null,
  };
}

export const fixtureSnapshot: Snapshot = {
  revision: 42,
  log: { searching: false, dir: "C:\\Games\\broken_arrow\\GameLogs", file: "Gamelog__2026_11_16__18_51.log", ready: true, error: null },
  persona: "Starling",
  gameVersion: "1.2.2.1",
  selfId: 202,
  api: { inFlight: 1, queued: 1, pausedUntil: null, lastError: null },
  reminders: [
    { id: 1, label: "第 10 分钟", atSecs: 600, enabled: true, fireAt: now - 401_000 + 600_000, outcome: null, outcomeAt: null },
    { id: 2, label: "第 15 分钟", atSecs: 900, enabled: true, fireAt: now - 401_000 + 900_000, outcome: null, outcomeAt: null },
  ],
  alert: { seq: 1, matchId: 9000001, ruleId: 1, label: "第 10 分钟", at: now - 2000 },
  ui: {},
  current: {
    matchId: 9000001,
    phase: "inBattle",
    connection: "connected",
    rejoined: false,
    map: "Central Village",
    scenario: "Central Village_network",
    deck: "my deck",
    clock: { startAt: now - 401_000, source: "logEstimate" },
    discoveredAt: now - 520_000,
    endedAt: null,
    selfTeam: "bravo",
    rosterKnown: true,
    players,
    teams: [average("alpha"), average("bravo")],
  },
};

/** 玩家档案：用快照里的玩家拼一份，加上虚构的历史与同场记录。 */
export function fixtureDetail(id: number): PlayerDetail | null {
  const p = players.find((x) => x.id === id);
  if (!p) return null;
  const day = 86_400_000;
  const history: RatingPoint[] = Array.from({ length: 14 }, (_, i) => ({
    at: now - (14 - i) * day,
    rating: (p.profile?.rating ?? 1800) - 120 + Math.round(Math.sin(i / 2) * 60 + i * 9),
    level: p.profile?.level ?? null,
    rank: null,
    ratingGames: null,
  }));
  const maps = ["Airport", "Central Village", "Suwalki", "Baltiisk", "Coast"];
  const matches: EncounterRow[] = Array.from({ length: Math.min(8, p.encounters.total || 3) }, (_, i) => ({
    matchId: 9000000 - i * 37,
    at: now - (i + 1) * day * 0.7,
    map: maps[i % maps.length],
    relation: i % 3 === 2 ? "opponent" : "teammate",
    team: i % 2 ? "alpha" : "bravo",
    phase: "ended",
    logName: p.logName,
    durationMs: (18 + ((i * 7) % 25)) * 60_000,
  }));
  return {
    id: p.id,
    name: p.name,
    logName: p.logName,
    steamId: "76561190000000201",
    profile: p.profile,
    fetchedAt: p.profileFetchedAt,
    firstSeenAt: now - 120 * day,
    lastSeenAt: now - 600_000,
    note: p.note ? { body: p.note, updatedAt: now - 3 * day } : null,
    encounters: p.encounters,
    history,
    matches,
    isSelf: p.isSelf,
  };
}

export function fixtureHits(query: string): PlayerHit[] {
  const q = query.trim().toLowerCase();
  return players
    .filter((p) => !q || String(p.id) === q || (p.name ?? "").toLowerCase().includes(q))
    .map((p) => ({
      id: p.id,
      name: p.name,
      logName: p.logName,
      rating: p.profile?.rating ?? null,
      level: p.profile?.level ?? null,
      lastSeenAt: now - 600_000,
      encounters: p.encounters.total,
      hasNote: p.note != null,
    }));
}