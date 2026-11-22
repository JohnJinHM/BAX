// 切换皮肤与个人调整：在 <html data-skin> 上换一组 CSS 变量，再在行内样式上叠加个人改过的颜色、
// 字号倍数与间距系数（ADR-0014）。设置保存在界面设置 ui.skin，两个窗口随快照一起切换。
// 这里另在 localStorage 记一份，只用于快照到达之前的第一帧，避免先闪一下默认外观；
// 快照到了以后以快照为准（ADR-0004）。

import { shallowRef, watch, type ShallowRef } from "vue";

import { appearanceVars, managedVars, normalizeAppearance, type Appearance } from "./appearance";
import type { Snapshot } from "./bindings/Snapshot";
import { useSetting } from "./settings";
import type { SkinId } from "./skins";

const CACHE_KEY = "bax.appearance";
/** 0.2.2 只记皮肤 ID */
const LEGACY_CACHE_KEY = "bax.skin";

/** 当前生效的皮肤。 */
export const activeSkin = shallowRef<SkinId>("default");
/** 外观每变一次加一。图表这类从 CSS 变量取色、取字号的计算依赖它重新生成。 */
export const appearanceRevision = shallowRef(0);

export function applyAppearance(value: unknown): Appearance {
  const appearance = normalizeAppearance(value);
  const root = document.documentElement;
  const vars = appearanceVars(appearance);
  // 先改属性和变量再改 activeSkin：依赖它的计算属性重新取色时，新变量已经生效
  root.dataset.skin = appearance.id;
  for (const name of managedVars) {
    const v = vars[name];
    if (v == null) root.style.removeProperty(name);
    else root.style.setProperty(name, v);
  }
  activeSkin.value = appearance.id;
  appearanceRevision.value++;
  return appearance;
}

/** 页面挂载前调用，套用上次记下的外观。 */
export function restoreSkin(): void {
  let cached: unknown = null;
  try {
    const saved = localStorage.getItem(CACHE_KEY);
    if (saved) cached = JSON.parse(saved);
    else {
      const id = localStorage.getItem(LEGACY_CACHE_KEY);
      if (id) cached = { id };
    }
  } catch {
    // 存储不可用或内容损坏时先用默认外观，快照到了再换
  }
  applyAppearance(cached);
}

/** 跟随设置切换皮肤与个人调整。快照到达前不动，保留 restoreSkin 套用的外观。 */
export function useSkin(snapshot: ShallowRef<Snapshot | null>): void {
  const setting = useSetting(snapshot, "ui.skin");
  watch(
    // 按内容比较：快照每次都是新对象，内容没变时不重新套用
    () => (snapshot.value ? JSON.stringify(normalizeAppearance(setting.value)) : null),
    (json) => {
      if (json == null) return;
      applyAppearance(JSON.parse(json));
      try {
        localStorage.setItem(CACHE_KEY, json);
        localStorage.removeItem(LEGACY_CACHE_KEY);
      } catch {
        // 只影响下次启动的第一帧
      }
    },
    { immediate: true },
  );
}
