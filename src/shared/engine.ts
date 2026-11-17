// 与 Rust 一侧通信。业务状态只在 Rust 里维护，这里只订阅快照、转发命令（ADR-0004）。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { shallowRef, type ShallowRef } from "vue";

import type { ReminderRule } from "./bindings/ReminderRule";
import type { JsonValue } from "./bindings/serde_json/JsonValue";
import type { Snapshot } from "./bindings/Snapshot";

const SNAPSHOT_EVENT = "bax://snapshot";
const APP_STATUS_EVENT = "bax://app";

/** 外壳自己的状态：悬浮窗与快捷键。对应 src-tauri/src/app.rs 的 AppStatus。 */
export interface AppStatus {
  overlayVisible: boolean;
  overlayLocked: boolean;
  /** 注册失败的快捷键及原因 */
  hotkeyErrors: string[];
  dataDir: string;
}

const snapshot = shallowRef<Snapshot | null>(null);
const appStatus = shallowRef<AppStatus | null>(null);
let subscribed: Promise<void> | null = null;

function acceptSnapshot(next: Snapshot) {
  // 事件与主动读取可能乱序到达，以修订号为准
  const current = snapshot.value;
  if (!current || next.revision >= current.revision) {
    snapshot.value = next;
  }
}

function subscribe(): Promise<void> {
  subscribed ??= (async () => {
    await listen<Snapshot>(SNAPSHOT_EVENT, (event) => acceptSnapshot(event.payload));
    await listen<AppStatus>(APP_STATUS_EVENT, (event) => {
      appStatus.value = event.payload;
    });
    const [first, status] = await Promise.all([
      invoke<Snapshot>("get_snapshot"),
      invoke<AppStatus>("get_app_status"),
    ]);
    acceptSnapshot(first);
    appStatus.value ??= status;
  })();
  return subscribed;
}

/** 引擎的最新快照。窗口里所有组件共用同一份订阅。 */
export function useSnapshot(): ShallowRef<Snapshot | null> {
  void subscribe();
  return snapshot;
}

/** 外壳的最新状态。 */
export function useAppStatus(): ShallowRef<AppStatus | null> {
  void subscribe();
  return appStatus;
}

export const commands = {
  refreshPlayers: () => invoke<void>("refresh_players"),
  setLogDir: (dir: string | null) => invoke<void>("set_log_dir", { dir }),
  setReminders: (rules: ReminderRule[]) => invoke<void>("set_reminders", { rules }),
  /** 此刻处于正式开局后多少毫秒，准备阶段里为负数。 */
  calibrate: (elapsedMs: number) => invoke<void>("calibrate", { elapsedMs }),
  setUiSetting: (key: string, value: JsonValue) => invoke<void>("set_ui_setting", { key, value }),
  setOverlayLocked: (locked: boolean) => invoke<void>("set_overlay_locked", { locked }),
  setOverlayVisible: (visible: boolean) => invoke<void>("set_overlay_visible", { visible }),
  openMainWindow: () => invoke<void>("open_main_window"),
};

/** 把命令的失败原因转成一句话。 */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
}
