// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { JsonValue } from "../shared/bindings/serde_json/JsonValue";

import { fixtureDetail, fixtureHistory, fixtureHits, fixtureReport, fixtureSnapshot } from "./fixture";

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
      case "player_detail":
        return fixtureDetail(Number((args as { id?: number } | undefined)?.id));
      case "match_history":
        return structuredClone(fixtureHistory);
      case "open_batrace_player":
        window.open(`https://dash.batrace.top/player/${Number((args as { id: number }).id)}`, "_blank", "noopener,noreferrer");
        return null;
      case "match_report":
        return fixtureReport(Number((args as { id?: number } | undefined)?.id));
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
