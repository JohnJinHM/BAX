// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { JsonValue } from "../shared/bindings/serde_json/JsonValue";

import { fixtureDetail, fixtureHistory, fixtureHits, fixtureReport, fixtureSnapshot } from "./fixture";

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
  const saved = localStorage.getItem("bax.dev.ui");
  if (saved) fixtureSnapshot.ui = { ...fixtureSnapshot.ui, ...JSON.parse(saved) };
  if (options.collapsed) fixtureSnapshot.ui["ui.overlay"] = { opacity: .88, scale: 1, expanded: false, autoExpand: false };
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
      observed: { side: i < 5 ? "left" : "right", row: i % 5, friendly: i < 5, name: p.name ?? "Raven", level: p.profile?.level ?? 50, alternatives: [] },
      candidates: i === 4 ? [] : [{ id: p.id, name: p.name ?? "Raven", level: p.profile?.level ?? 50, local: i % 2 === 0, profile: p.profile }],
      ambiguous: i === 3,
    })) }; m.players = []; m.teams = [];
  }
  mockIPC((cmd, args) => {
    switch (cmd) {
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
      case "player_detail":
        return fixtureDetail(Number((args as { id?: number } | undefined)?.id));
      case "match_history":
        return structuredClone(fixtureHistory);
      case "open_batrace_player":
        window.open(`https://dash.batrace.top/player/${Number((args as { id: number }).id)}`, "_blank", "noopener,noreferrer");
        return null;
      case "match_report": {
        const report = fixtureReport(Number((args as { id?: number } | undefined)?.id));
        if (report && params.has("custom")) {
          report.mode = "custom";
          report.spectators = [{ id: 201, name: "Dune" }];
          report.analysis.players = report.analysis.players.filter(p => [101, 202].includes(p.id));
          report.analysis.sorties = report.analysis.sorties.filter(s => [101, 202].includes(s.player));
          report.analysis.teams = report.analysis.teams.map(t => {
            const members = report.analysis.players.filter(p => p.team === t.team);
            const sum = (key: 'deployedValue' | 'destructionScore' | 'lossesScore' | 'destruction' | 'losses' | 'objectives' | 'supplyConsumed') => members.reduce((n, p) => n + (p[key] ?? 0), 0);
            return { ...t, players: members.length, avgRating: members[0]?.oldRating ?? null,
              deployedValue: sum('deployedValue'), destructionScore: sum('destructionScore'), lossesScore: sum('lossesScore'),
              destruction: sum('destruction'), losses: sum('losses'), objectives: sum('objectives'), supplyConsumed: sum('supplyConsumed'),
              valueByMinute: t.valueByMinute.map((_, i) => members.reduce((n, p) => n + p.valueByMinute[i], 0)) };
          });
        }
        return report;
      }
      case "search_players": {
        const { query = "", sort = "default", filter = "all", limit = 80, offset = 0 } = args as { query?: string; sort?: string; filter?: string; limit?: number; offset?: number };
        const hits = fixtureHits(query).filter(p => filter === "noted" ? p.hasNote : filter === "rated" ? p.rating != null : filter === "encountered" ? p.encounters > 0 : true);
        hits.sort((a, b) => {
          if (sort === "name") return (a.name ?? a.logName ?? "").localeCompare(b.name ?? b.logName ?? "");
          if (sort === "recent") return (b.lastSeenAt ?? 0) - (a.lastSeenAt ?? 0);
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
