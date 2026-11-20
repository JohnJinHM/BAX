// 预览用的虚构快照：一场进行中的 5v5，玩家状态覆盖各种情况。只在 dev/ 预览页里使用。

import type { EncounterRow } from "../shared/bindings/EncounterRow";
import type { HistoryRow } from "../shared/bindings/HistoryRow";
import type { MatchReportView } from "../shared/bindings/MatchReportView";
import type { PlayerReport } from "../shared/bindings/PlayerReport";
import type { PlayerDetail } from "../shared/bindings/PlayerDetail";
import type { PlayerHit } from "../shared/bindings/PlayerHit";
import type { PlayerView } from "../shared/bindings/PlayerView";
import type { RatingPoint } from "../shared/bindings/RatingPoint";
import type { Snapshot } from "../shared/bindings/Snapshot";
import type { Sortie } from "../shared/bindings/Sortie";
import type { UnitClass } from "../shared/bindings/UnitClass";

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
            favoriteSpecs: [{ ids: [5], name: "美国空降部队", experience: 240000, experienceShare: 0.67 }, { ids: [3], name: "美国海军陆战队", experience: 81000, experienceShare: 0.23 }],
            favoriteSpecPairs: [{ ids: [3, 5], name: "美国海军陆战队 + 美国空降部队", experience: 120000, experienceShare: 0.65 }],
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
      pregame: { players: [], status: "" },
    players,
    teams: [average("alpha"), average("bravo")],
  },
  archive: { pending: 1, lastArchived: 8999963, seq: 3 },
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
// ------------------------------------------------------------ 比赛记录与战后分析

const maps = ["Airport", "Central Village", "Suwalki", "Baltiisk", "Coast"];

export const fixtureHistory: HistoryRow[] = Array.from({ length: 12 }, (_, i) => {
  const matchId = 9000001 - i * 37;
  const report: HistoryRow["report"] =
    i === 0
      ? { state: "none" }
      : i === 1
        ? { state: "pending", attempts: 2, nextAt: now + 240_000 }
        : i === 3
          ? { state: "cancelled" }
          : i === 5
          ? { state: "gaveUp", attempts: 13, lastError: "站点没有收录这条数据" }
          : { state: "ready", fetchedAt: now - i * 3_600_000 };
  return {
    matchId,
    at: now - i * 3_000_000 - 520_000,
    map: maps[i % maps.length],
    phase: i === 0 ? "inBattle" : "ended",
    selfTeam: "bravo",
    won: report.state === "ready" ? i % 3 !== 1 : null,
    players: 10,
    durationMs: i === 0 ? null : report.state === "cancelled" ? 49 : (19 + ((i * 7) % 20)) * 60_000 + 13_000,
    report,
  };
});

/** 固定种子的伪随机数，预览每次一样。 */
function seeded(seed: number) {
  let x = seed;
  return () => {
    x = (x * 1103515245 + 12345) % 2147483648;
    return x / 2147483648;
  };
}

const fixtureUnits: { id: number; name: string; cls: UnitClass; cost: number }[] = [
  { id: 191, name: "M1A2 SEP v3", cls: "tank", cost: 260 },
  { id: 15, name: "M2A2 Bradley", cls: "ifv", cost: 140 },
  { id: 59, name: "Marines", cls: "infantry", cost: 45 },
  { id: 4, name: "侦察车 D", cls: "recon", cost: 70 },
  { id: 11, name: "M142 HIMARS", cls: "artillery", cost: 210 },
  { id: 10, name: "AN/TWQ-1 Avenger", cls: "airDefense", cost: 150 },
  { id: 8, name: "AH-64D Longbow", cls: "helicopter", cost: 230 },
  { id: 169, name: "Su-35S", cls: "jet", cost: 290 },
  { id: 242, name: "HEMTT", cls: "supply", cost: 40 },
  { id: 900010, name: "运输车 J", cls: "transport", cost: 30 },
];

export function fixtureReport(id: number): MatchReportView | null {
  const row = fixtureHistory.find((r) => r.matchId === id);
  if (!row || row.report.state !== "ready") return null;
  const rand = seeded(id);
  const minutes = Math.ceil((row.durationMs ?? 1_500_000) / 60_000);
  const durationSecs = Math.floor((row.durationMs ?? 1_500_000) / 1000);
  const won = row.won ?? false;
  const sorties: Sortie[] = [];
  const reports: PlayerReport[] = players.map((p, index) => {
    const own: Sortie[] = [];
    const total = 18 + Math.floor(rand() * 30);
    for (let k = 0; k < total; k++) {
      const u = fixtureUnits[Math.floor(rand() * fixtureUnits.length)];
      const spawnSecs = k < 6 ? 0 : Math.floor(rand() * durationSecs);
      const dies = rand() < 0.55;
      own.push({
        player: p.id,
        unitId: u.id,
        variantKey: String(u.id), unitName: u.name, thumbnail: null,
        class: u.cls,
        spawnSecs,
        deathSecs: dies ? Math.min(durationSecs, spawnSecs + Math.floor(rand() * 600)) : null,
        cost: Math.round(u.cost * (1.1 + rand() * 0.4)),
        refunded: u.cls === "jet" && rand() < 0.4,
        airdropped: k % 9 === 0,
        transferredTo: index === 3 && k === 7 ? players[2].id : null,
        kills: dies ? Math.floor(rand() * 3) : Math.floor(rand() * 6),
      });
    }
    sorties.push(...own);
    const valueByMinute = Array.from({ length: minutes }, () => 0);
    const comp = new Map<UnitClass, { count: number; value: number }>();
    const units = new Map<number, PlayerReport["units"][number]>();
    for (const s of own) {
      const v = s.refunded ? 0 : s.cost;
      valueByMinute[Math.min(minutes - 1, Math.floor(s.spawnSecs / 60))] += v;
      const c = comp.get(s.class) ?? { count: 0, value: 0 };
      c.count++;
      c.value += v;
      comp.set(s.class, c);
      const r = units.get(s.unitId) ?? {
        unitId: s.unitId,
        variantKey: s.variantKey, unitName: s.unitName, thumbnail: s.thumbnail,
        class: s.class,
        count: 0,
        refunded: 0,
        dead: 0,
        kills: 0,
        damage: 0,
        value: 0,
        firstSpawnSecs: s.spawnSecs,
      };
      r.count++;
      r.refunded += s.refunded ? 1 : 0;
      r.dead += s.deathSecs != null ? 1 : 0;
      r.kills += s.kills;
      r.damage += s.kills * 180 + 40;
      r.value += v;
      r.firstSpawnSecs = Math.min(r.firstSpawnSecs ?? s.spawnSecs, s.spawnSecs);
      units.set(s.unitId, r);
    }
    const deployed = valueByMinute.reduce((a, b) => a + b, 0);
    const old = p.profile?.rating ?? null;
    const playerWon = (p.team === "bravo") === won;
    return {
      id: p.id,
      name: p.name,
      team: p.team,
      oldRating: old,
      newRating: old == null ? null : old + (playerWon ? 9 + Math.round(rand() * 8) : -(8 + Math.round(rand() * 8))),
      deployedValue: deployed,
      destructionScore: Math.round(deployed * (0.6 + rand() * 0.9)),
      lossesScore: Math.round(deployed * (0.4 + rand() * 0.5)),
      destruction: Math.floor(10 + rand() * 40),
      losses: Math.floor(8 + rand() * 30),
      damageDealt: Math.round(rand() * 40000),
      damageReceived: Math.round(rand() * 30000),
      objectives: Math.floor(rand() * 6),
      supplyConsumed: Math.floor(rand() * 3000),
      supplyFromAllies: Math.floor(rand() * 500),
      supplyByAllies: Math.floor(rand() * 500),
      supplyCaptured: Math.floor(rand() * 400),
      supplyAirdropped: null,
      deserter: p.id === 103,
      sorties: own.length,
      refunded: own.filter((s) => s.refunded).length,
      dead: own.filter((s) => s.deathSecs != null).length,
      transferredOut: own.filter((s) => s.transferredTo != null).length,
      lastSpawnSecs: Math.max(...own.map((s) => s.spawnSecs)),
      valueByMinute,
      composition: [...comp.entries()].map(([cls, c]) => ({ class: cls, ...c })).sort((a, b) => b.value - a.value),
      units: [...units.values()].sort((a, b) => b.value - a.value),
    };
  });
  sorties.sort((a, b) => a.spawnSecs - b.spawnSecs);
  const team = (t: "alpha" | "bravo") => {
    const ps = reports.filter((p) => p.team === t);
    const sum = (f: (p: PlayerReport) => number | null) => ps.reduce((a, p) => a + (f(p) ?? 0), 0);
    const ratings = ps.map((p) => p.oldRating).filter((r): r is number => r != null);
    return {
      team: t,
      players: ps.length,
      won: (t === "bravo") === won,
      points: t === "bravo" ? (won ? 2000 : 1340) : won ? 1410 : 2000,
      avgRating: ratings.length ? ratings.reduce((a, b) => a + b, 0) / ratings.length : null,
      deployedValue: sum((p) => p.deployedValue),
      destructionScore: sum((p) => p.destructionScore),
      lossesScore: sum((p) => p.lossesScore),
      destruction: sum((p) => p.destruction),
      losses: sum((p) => p.losses),
      objectives: sum((p) => p.objectives),
      supplyConsumed: sum((p) => p.supplyConsumed),
      valueByMinute: Array.from({ length: minutes }, (_, i) => ps.reduce((a, p) => a + p.valueByMinute[i], 0)),
    };
  };
  return {
    analysis: {
      matchId: id,
      startTime: row.at + 90_000,
      durationSecs: Math.round((row.durationMs ?? 0) / 1000),
      mapId: 3,
      winner: won ? "bravo" : "alpha",
      alphaPoints: won ? 1410 : 2000,
      bravoPoints: won ? 2000 : 1340,
      minutes,
      catalogComplete: true,
      teams: [team("alpha"), team("bravo")],
      players: reports,
      sorties,
      unitNames: Object.fromEntries(fixtureUnits.map((u) => [u.id, u.name])),
    },
    map: row.map,
    selfId: fixtureSnapshot.selfId,
    selfTeam: "bravo",
    fetchedAt: now - 3_600_000,
    catalogFetchedAt: now - 86_400_000,
  };
}
