// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import { fixtureDetail, fixtureHistory, fixtureHits, fixtureReport, fixtureSnapshot } from "./fixture";

export function installMock(options: { locked?: boolean; collapsed?: boolean } = {}) {
  mockWindows("main");
  if (new URLSearchParams(location.search).has("pregame") && fixtureSnapshot.current) {
    const m = fixtureSnapshot.current;
    fixtureSnapshot.alert = null;
    m.phase = "waitingRoom"; m.rosterKnown = false; m.clock = null;
    m.pregame = { status: "候选待日志确认", players: m.players.map((p, i) => ({
      observed: { side: i < 5 ? "left" : "right", row: i % 5, name: p.name ?? "Raven", level: p.profile?.level ?? 50, alternatives: [] },
      candidates: i === 4 ? [] : [{ id: p.id, name: p.name ?? "Raven", level: p.profile?.level ?? 50, local: i % 2 === 0, profile: p.profile }],
      ambiguous: i === 3,
    })) }; m.players = []; m.teams = [];
  }
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "get_snapshot":
        return options.collapsed
          ? { ...fixtureSnapshot, ui: { ...fixtureSnapshot.ui, "ui.overlay": { opacity: 0.88, scale: 1, expanded: false } } }
          : fixtureSnapshot;
      case "get_app_status":
        return {
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
        return fixtureHistory;
      case "match_report":
        return fixtureReport(Number((args as { id?: number } | undefined)?.id));
      case "search_players":
        return fixtureHits(String((args as { query?: string } | undefined)?.query ?? ""));
      default:
        // 事件订阅、窗口大小等调用一律当作成功
        return null;
    }
  });
}
