// 界面设置。引擎只负责保存和分发（键以 ui. 开头），内容由界面自己定义。
// ui.hotkeys、ui.alerts、ui.diag 与 ui.overlay 的 snapEdges 也由 Rust 外壳读取，默认值与 Rust 一侧的保持一致。

import { computed, type ComputedRef, type ShallowRef } from "vue";

import type { JsonValue } from "./bindings/serde_json/JsonValue";
import type { Snapshot } from "./bindings/Snapshot";
import { commands } from "./engine";
import type { SkinId } from "./skins";

export interface OverlayAppearance {
  /** 背景不透明度，0.3～1 */
  opacity: number;
  /** 整体缩放，0.8～1.5 */
  scale: number;
  /** 展开玩家列表；收起时只显示计时与两队均分 */
  expanded: boolean;
  autoExpand: boolean;
  collapseBeforeBattleSecs: number;
  animations: boolean;
  /** 拖动松手后靠近屏幕边缘时贴边，改变大小时保持贴边。外壳读取，默认值与 src-tauri/src/overlay.rs 一致 */
  snapEdges: boolean;
}

export interface AlertSettings {
  /** 提醒时响铃 */
  sound: boolean;
  /** 提醒时如果悬浮窗被隐藏，把它显示出来 */
  showOverlay: boolean;
}

export interface Hotkeys {
  toggleOverlay: string;
  toggleLock: string;
}

export interface DiagSettings {
  /** 诊断档位：standard 记录生命周期与状态变化，verbose 再加上每次请求与每个事件 */
  level: "standard" | "verbose";
}

export interface SkinSettings {
  /** 皮肤 ID，见 skins.ts；不认识的值按默认皮肤处理 */
  id: SkinId;
}

interface Settings {
  "ui.skin": SkinSettings;
  "ui.sortie": { height: number };
  "ui.ocr": { enabled: boolean };
  "ui.overlay": OverlayAppearance;
  "ui.alerts": AlertSettings;
  "ui.hotkeys": Hotkeys;
  "ui.diag": DiagSettings;
}

export const defaults: Settings = {
  "ui.skin": { id: "default" },
  "ui.sortie": { height: 600 },
  "ui.ocr": { enabled: true },
  "ui.overlay": { opacity: 0.88, scale: 1, expanded: true, autoExpand: true, collapseBeforeBattleSecs: 60, animations: true, snapEdges: true },
  "ui.alerts": { sound: true, showOverlay: true },
  "ui.hotkeys": { toggleOverlay: "Ctrl+Alt+B", toggleLock: "Ctrl+Alt+L" },
  "ui.diag": { level: "standard" },
};

type Key = keyof typeof defaults;

/** 读一项设置，缺的字段用默认值补上。 */
export function useSetting<K extends Key>(
  snapshot: ShallowRef<Snapshot | null>,
  key: K,
): ComputedRef<(typeof defaults)[K]> {
  return computed(() => {
    const saved = snapshot.value?.ui[key];
    const base = defaults[key];
    if (saved && typeof saved === "object" && !Array.isArray(saved)) {
      return { ...base, ...(saved as object) } as (typeof defaults)[K];
    }
    return base;
  });
}

/** 改一项设置的部分字段。 */
export function saveSetting<K extends Key>(
  current: (typeof defaults)[K],
  key: K,
  patch: Partial<(typeof defaults)[K]>,
): Promise<void> {
  return commands.setUiSetting(key, { ...current, ...patch } as unknown as JsonValue);
}
