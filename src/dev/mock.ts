// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { JsonValue } from "../shared/bindings/serde_json/JsonValue";

import { currentRelations } from "../shared/roster";
import { fixtureDetail, fixtureHistory, fixtureHits, fixtureMatches, fixtureReport, fixtureSnapshot } from "./fixture";

/** 预览页控制按钮使用虚构快照模拟阶段变化，不进入正式构建。 */
export function previewOverlayStage(stage: "waitingRoom" | "loading" | "deployment" | "battle" | "ended" | "exited") {
  const match = fixtureSnapshot.current;
  if (!match) return;
  match.matchId = 8999927;
  match.phase = stage === "battle" ? "inBattle" : stage === "exited" ? "ended" : stage;
  match.connection = stage === "exited" ? "disconnected" : "connected";
  match.endedAt = match.phase === "ended" ? Date.now() : null;
  match.clock = stage === "deployment" ? { startAt: Date.now() + 65000, source: "logEstimate" } : null;
  fixtureSnapshot.alert = null;
  fixtureSnapshot.revision++;
  void emit("bax://snapshot", structuredClone(fixtureSnapshot));
}

export function installMock(options: { locked?: boolean; collapsed?: boolean } = {}) {
  mockWindows("main");
  const params = new URLSearchParams(location.search);
  const reportMode = params.has("pve") ? "pve" : params.has("custom") ? "custom" : "ranked";
  const previewLogDir = fixtureSnapshot.log.dir;
  if (params.has("missingLog")) {
    fixtureSnapshot.current = null;
    fixtureSnapshot.log = { searching: false, dir: null, file: null, ready: false, error: null };
  }
  const saved = localStorage.getItem("bax.dev.ui");
  if (saved) fixtureSnapshot.ui = { ...fixtureSnapshot.ui, ...JSON.parse(saved) };
  // ?skin=broken-arrow 直接用某个皮肤预览
  const skin = params.get("skin");
  // ?appearance={"fontScale":1.2,"density":"compact"} 叠加个人调整，字段见 src/shared/appearance.ts
  const look = params.get("appearance");
  if (skin || look) {
    const saved = fixtureSnapshot.ui["ui.skin"];
    fixtureSnapshot.ui["ui.skin"] = {
      ...(saved && typeof saved === "object" && !Array.isArray(saved) ? saved : {}),
      ...(look ? JSON.parse(look) : {}),
      ...(skin ? { id: skin } : {}),
    };
  }
  if (options.collapsed) {
    const overlay = fixtureSnapshot.ui["ui.overlay"];
    fixtureSnapshot.ui["ui.overlay"] = {
      ...(overlay && typeof overlay === "object" && !Array.isArray(overlay) ? overlay : {}),
      expanded: false, autoExpand: false,
    };
  }
  if (params.has("ended") && fixtureSnapshot.current) {
    fixtureSnapshot.current.matchId = 8999927;
    fixtureSnapshot.current.phase = "ended";
    fixtureSnapshot.current.endedAt = Date.now();
    fixtureSnapshot.alert = null;
  }
  if (params.has("deployment") && fixtureSnapshot.current) {
    fixtureSnapshot.current.phase = "deployment";
    fixtureSnapshot.current.clock = { startAt: Date.now() + 65000, source: "logEstimate" };
    fixtureSnapshot.alert = null;
  }
  if (params.has("custom") && fixtureSnapshot.current) {
    const m = fixtureSnapshot.current;
    m.mode = "custom";
    m.players = m.players.filter(p => [101, 201, 202].includes(p.id));
    m.players.find(p => p.id === 201)!.team = "spectators";
    m.teams = m.teams.map(t => ({ ...t, players: 1, rated: 1 }));
  }
  if (params.has("pve") && fixtureSnapshot.current) {
    const m = fixtureSnapshot.current;
    m.mode = "pve"; m.scenario = "Example_Skirmish";
    m.players = m.players.filter(p => p.team === "bravo").slice(0, 2);
    m.teams = m.teams.map(t => ({ ...t, players: t.team === "bravo" ? 2 : 0 }));
  }
  if (new URLSearchParams(location.search).has("pregame") && fixtureSnapshot.current) {
    const m = fixtureSnapshot.current;
    fixtureSnapshot.alert = null;
    m.phase = "waitingRoom"; m.rosterKnown = false; m.clock = null;
    m.pregame = { status: "候选待日志确认", players: m.players.map((p, i) => ({
      observed: { country: 1, side: i < 5 ? "left" : "right", row: i % 5, friendly: i < 5, name: p.name ?? "Raven", level: p.profile?.level ?? 50, alternatives: [] },
      candidates: i === 4 ? [] : [{ id: p.id, name: p.name ?? "Raven", level: p.profile?.level ?? 50, local: i % 2 === 0, profile: p.profile }],
      ambiguous: i === 3,
      traceCount: i === 3 || i === 4 ? null : p.traceCount,
    })) }; m.players = []; m.teams = [];
  }
  let lastReminderId = Math.max(0, ...fixtureSnapshot.reminders.map(r => r.id));
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "pick_log_dir":
        return previewLogDir;
      case "set_log_dir": {
        const { dir } = args as { dir: string | null };
        const pending = { searching: dir == null, dir, file: null, ready: false, error: null };
        fixtureSnapshot.log = pending;
        fixtureSnapshot.revision++;
        void emit("bax://snapshot", structuredClone(fixtureSnapshot));
        window.setTimeout(() => {
          if (fixtureSnapshot.log !== pending) return;
          const found = dir ?? previewLogDir;
          fixtureSnapshot.log = { searching: false, dir: found, file: found ? "Gamelog__preview.log" : null, ready: !!found, error: null };
          fixtureSnapshot.revision++;
          void emit("bax://snapshot", structuredClone(fixtureSnapshot));
        }, 300);
        return null;
      }
      case "reserve_reminder_id":
        return ++lastReminderId;
      case "check_data_sources":
        return new Promise(resolve => setTimeout(() => resolve([
          { name: "主源 · 玩家资料", available: true, message: "预览样本：查询成功", latencyMs: 180, checkedAt: Math.floor(Date.now() / 1000) },
          { name: "主源 · 战报样本", available: true, message: "预览样本：查询成功", latencyMs: 240, checkedAt: Math.floor(Date.now() / 1000) },
          ...["社区标记", "昵称搜索", "单位目录"].map(name => ({ name: `BATrace · ${name}`, available: false, message: "预览样本：HTTP 404", latencyMs: 120, checkedAt: Math.floor(Date.now() / 1000) })),
        ]), 900));
      case "get_snapshot":
        return structuredClone(fixtureSnapshot);
      case "set_ui_setting": {
        const { key, value } = args as { key: string; value: JsonValue };
        fixtureSnapshot.ui[key] = value;
        fixtureSnapshot.revision++;
        localStorage.setItem("bax.dev.ui", JSON.stringify(fixtureSnapshot.ui));
        void emit("bax://snapshot", structuredClone(fixtureSnapshot));
        return null;
      }
      case "get_app_status":
        return {
          update: { currentVersion: "0.2.0", edition: "Full", canInstall: true, state: "available", version: "0.2.1", notes: "预览数据：常用专精与自动更新。", downloaded: 0, total: null, message: "发现新版本 0.2.1" },
          ocrSupported: !new URLSearchParams(location.search).has("lite"),
          overlayVisible: true,
          overlayLocked: options.locked ?? false,
          hotkeyErrors: [],
          dataDir: "C:\\Users\\example\\AppData\\Local\\BAX",
          diagDir: "C:\\Users\\example\\AppData\\Local\\BAX\\diag",
          diagLockedBy: null,
          diagError: null,
        };
      case "export_diag":
        return "C:\\Users\\example\\AppData\\Local\\BAX\\diag\\export\\bax-diag-20261117-203015.txt";
      case "check_update":
      case "install_update":
      case "open_releases":
        return null;
      case "player_fights": {
        const id = Number((args as {id:number}).id);
        const player = fixtureDetail(id);
        const at = localStorage.getItem(`bax.dev.fightQuery.${id}`);
        const fights = fixtureMatches.flatMap(m => {
          const report = fixtureReport(m.matchId);
          const p = report?.analysis?.players.find(row => row.id === id);
          if (!p || !report?.analysis) return [];
          const winner = report.analysis.winner;
          const deployedUnits = p.units.map(u => ({id:u.unitId, name:u.unitName, class:u.class}));
          return [{
            hasLocalLog: fixtureHistory.some(local => local.matchId === m.matchId),
            relation: player?.matches.find(row => row.matchId === m.matchId)?.relation ?? null,
            playedTogether: id !== fixtureSnapshot.selfId && !!player?.matches.some(row => row.matchId === m.matchId),
            mode:reportMode, matchId:m.matchId, start:m.at, durationSecs:report.analysis.durationSecs, map:m.map,
            team:p.team, won:winner == null ? null : p.team === winner, winner,
            ratingDelta:reportMode !== "ranked" || p.newRating == null || p.oldRating == null ? null : p.newRating-p.oldRating,
            roster:report.analysis.players.map(({id,name,team}) => ({id,name,team})),
            performance:reportMode === "ranked" ? {kills:p.destruction ?? 0, losses:p.losses ?? 0, objectives:p.objectives ?? 0, suppliedAllies:p.supplyByAllies ?? 0,
              spawnedScore:p.units.reduce((sum,u) => sum+u.deployedScore,0)} : null,
            composition:reportMode === "ranked" ? p.composition : [], deployedUnits:reportMode === "ranked" ? deployedUnits : [],
            opening:reportMode === "ranked" ? deployedUnits.filter((_,i) => (p.units[i].firstSpawnSecs ?? Infinity) < 180) : null,
            specs:reportMode === "ranked" ? report.specs[id] ?? [] : [],
          }];
        });
        return { lastQueriedAt: at == null ? null : Number(at), fights };
      }
      case "stop_player_fights": return null;
      case "sync_player_fights":
        localStorage.setItem(`bax.dev.fightQuery.${(args as {id:number}).id}`, String(Date.now()));
        return "查询完成 · 预览战报已收录";
      case "player_detail":
        return fixtureDetail(Number((args as { id?: number } | undefined)?.id));
      case "match_history": {
        const { query = "", player, limit = 100, offset = 0 } = args as { query?: string; player?: number; limit?: number; offset?: number };
        const q = query.trim().toLowerCase();
        const members = fixtureHits("").map(hit => ({
          ...hit, matches: new Set(fixtureDetail(hit.id)?.matches.map(row => row.matchId)),
        }));
        return fixtureHistory.filter(row => {
          const roster = members.filter(p => p.matches.has(row.matchId));
          return (!player || roster.some(p => p.id === player)) && (!q || row.matchId === Number(q)
            || row.map?.toLowerCase().includes(q) || roster.some(p => p.id === Number(q)
              || p.name?.toLowerCase().includes(q) || p.logName?.toLowerCase().includes(q)));
        }).slice(offset, offset + limit);
      }
      case "open_batrace_player":
        window.open(`https://dash.batrace.top/player/${Number((args as { id: number }).id)}`, "_blank", "noopener,noreferrer");
        return null;
      case "match_report": {
        const report = fixtureReport(Number((args as { id?: number } | undefined)?.id));
        if (report && reportMode !== "ranked") {
          report.mode = reportMode;
            if (reportMode === "pve") {
              report.analysis = null;
              report.supply = [];
            }
          report.specs = {};
        }
        return report;
      }
      case "set_player_favorite": {
        const { id, favorite } = args as { id: number; favorite: boolean };
        const key = `bax.dev.favorite.${id}`;
        if (favorite) localStorage.setItem(key, "1");
        else localStorage.removeItem(key);
        return null;
      }
      case "search_players": {
        const { query = "", sort = "recent", filter = "all", limit = 80, offset = 0 } = args as { query?: string; sort?: string; filter?: string; limit?: number; offset?: number };
        const hits = fixtureHits(query).filter(p => filter === "noted" ? p.hasNote : filter === "rated" ? p.rating != null : filter === "encountered" ? p.encounters > 0 : true);
        const relations = currentRelations(fixtureSnapshot.current);
        const tier = (p: (typeof hits)[number]) => {
          if (p.id === fixtureSnapshot.selfId) return 0;
          if (relations.has(p.id)) return 1;
          return p.isFavorite ? 2 : 3;
        };
        hits.sort((a, b) => {
          const exact = Number(b.id === Number(query.trim())) - Number(a.id === Number(query.trim()));
          if (exact) return exact;
          if (sort === "recent" || sort === "default") {
            return tier(a) - tier(b) || (b.lastSeenAt ?? 0) - (a.lastSeenAt ?? 0) || a.id - b.id;
          }
          const priority = Number(relations.has(b.id)) - Number(relations.has(a.id));
          if (priority) return priority;
          if (sort === "name") return (a.name ?? a.logName ?? "").localeCompare(b.name ?? b.logName ?? "");
          if (sort === "ratingDesc" || sort === "ratingAsc") return Number(a.rating == null) - Number(b.rating == null) || ((a.rating ?? 0) - (b.rating ?? 0)) * (sort === "ratingDesc" ? -1 : 1);
          return Number(b.id === fixtureSnapshot.selfId) - Number(a.id === fixtureSnapshot.selfId) || b.encounters - a.encounters;
        });
        return hits.slice(offset, offset + limit);
      }
      default:
        // 事件订阅、窗口大小等调用一律当作成功
        return null;
    }
  }, { shouldMockEvents: true });
}
