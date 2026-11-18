// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC } from "@tauri-apps/api/mocks";

import { fixtureSnapshot } from "./fixture";

export function installMock(options: { locked?: boolean; collapsed?: boolean } = {}) {
  mockIPC((cmd) => {
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
      default:
        // 事件订阅、窗口大小等调用一律当作成功
        return null;
    }
  });
}
