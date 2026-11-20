// 与 Rust 一侧通信。业务状态只在 Rust 里维护，这里只订阅快照、转发命令（ADR-0004）。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { shallowRef, type ShallowRef } from "vue";

import type { HistoryRow } from "./bindings/HistoryRow";
import type { ImportSummary } from "./bindings/ImportSummary";
import type { MatchReportView } from "./bindings/MatchReportView";
import type { PlayerDetail } from "./bindings/PlayerDetail";
import type { PlayerHit } from "./bindings/PlayerHit";
import type { ReminderRule } from "./bindings/ReminderRule";
import type { JsonValue } from "./bindings/serde_json/JsonValue";
import type { Snapshot } from "./bindings/Snapshot";

const SNAPSHOT_EVENT = "bax://snapshot";
const APP_STATUS_EVENT = "bax://app";

/** 外壳自己的状态：悬浮窗与快捷键。对应 src-tauri/src/app.rs 的 AppStatus。 */
export interface AppStatus {
  ocrSupported: boolean;
  overlayVisible: boolean;
  overlayLocked: boolean;
  /** 注册失败的快捷键及原因 */
  hotkeyErrors: string[];
  dataDir: string;
  /** 诊断日志目录；不写诊断日志时为空 */
  diagDir: string;
  /** 诊断档位被什么锁定（环境变量或启动参数）；没有锁定时为 null */
  diagLockedBy: string | null;
  /** 诊断日志写不了时的原因 */
  diagError: string | null;
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
  /** 锁按钮的位置，物理像素，相对窗口左上角；锁定后鼠标在这里时仍可点击。 */
  setOverlayHotspot: (hotspot: { x: number; y: number; width: number; height: number } | null) =>
    invoke<void>("set_overlay_hotspot", { hotspot }),
  setOverlayVisible: (visible: boolean) => invoke<void>("set_overlay_visible", { visible }),
  openMainWindow: () => invoke<void>("open_main_window"),
  openDiagDir: () => invoke<void>("open_diag_dir"),
  /** 导出诊断包，返回文件路径。 */
  exportDiag: () => invoke<string>("export_diag"),
  /** 前端错误交给 Rust 写进诊断日志（ADR-0007）。 */
  reportUiError: (kind: string, message: string, stack: string | null) =>
    invoke<void>("report_ui_error", { kind, message, stack }),
  /** 长期库里没有这名玩家时为 null。 */
  playerDetail: (id: number) => invoke<PlayerDetail | null>("player_detail", { id }),
  /** 默认本机优先，然后按同场次数排序；筛选在分页前执行。 */
  searchPlayers: (query: string, limit = 50, sort = "default", filter = "all", offset = 0) => invoke<PlayerHit[]>("search_players", { query, limit, sort, filter, offset }),
  /** 内容为空时删除备注。 */
  setNote: (id: number, body: string) => invoke<void>("set_note", { id, body }),
  refreshPlayer: (id: number) => invoke<void>("refresh_player", { id }),
  /** 比赛记录，新的在前，带战报状态。 */
  matchHistory: (limit = 100, offset = 0) => invoke<HistoryRow[]>("match_history", { limit, offset }),
  /** 一场比赛的战后分析；还没有战报时为 null。 */
  matchReport: (id: number) => invoke<MatchReportView | null>("match_report", { id }),
  /** 立即去取战报，取不到时按时间表继续等。 */
  fetchReport: (id: number) => invoke<void>("fetch_report", { id }),
  /** 导入日志目录里的全部历史日志；重复导入不会重复计数。 */
  importHistory: () => invoke<ImportSummary>("import_history"),
};

/** 把命令的失败原因转成一句话。 */
export function errorText(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : String(error);
}
