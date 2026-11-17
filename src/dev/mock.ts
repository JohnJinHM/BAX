// 在浏览器里预览界面：用 Tauri 官方的 mockIPC 冒充 Rust 一侧。只在 dev/ 预览页里使用，不进发布包。

import { mockIPC } from "@tauri-apps/api/mocks";

import { fixtureSnapshot } from "./fixture";

export function installMock(options: { locked?: boolean } = {}) {
  mockIPC((cmd) => {
    switch (cmd) {
      case "get_snapshot":
        return fixtureSnapshot;
      case "get_app_status":
        return {
          overlayVisible: true,
          overlayLocked: options.locked ?? false,
          hotkeyErrors: [],
          dataDir: "C:\\Users\\example\\AppData\\Local\\BAX",
        };
      default:
        // 事件订阅、窗口大小等调用一律当作成功
        return null;
    }
  });
}
