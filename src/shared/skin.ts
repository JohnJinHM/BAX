// 切换皮肤：在 <html data-skin> 上换一组 CSS 变量（ADR-0014）。
// 所选皮肤保存在界面设置 ui.skin，两个窗口随快照一起切换。这里另在 localStorage 记一份皮肤 ID，
// 只用于快照到达之前的第一帧，避免先闪一下默认皮肤；快照到了以后以快照为准（ADR-0004）。

import { shallowRef, watch, type ShallowRef } from "vue";

import type { Snapshot } from "./bindings/Snapshot";
import { useSetting } from "./settings";
import { normalizeSkin, type SkinId } from "./skins";

const CACHE_KEY = "bax.skin";

/** 当前生效的皮肤。图表这类从 CSS 变量取色的地方依赖它重新取色。 */
export const activeSkin = shallowRef<SkinId>("default");

export function applySkin(value: unknown): SkinId {
  const id = normalizeSkin(value);
  // 先改属性再改 activeSkin：依赖它的计算属性重新取色时，新变量已经生效
  document.documentElement.dataset.skin = id;
  activeSkin.value = id;
  return id;
}

/** 页面挂载前调用，套用上次记下的皮肤。 */
export function restoreSkin(): void {
  let cached: string | null = null;
  try {
    cached = localStorage.getItem(CACHE_KEY);
  } catch {
    // 存储不可用时先用默认皮肤，快照到了再换
  }
  applySkin(cached);
}

/** 跟随设置切换皮肤。快照到达前不动，保留 restoreSkin 套用的皮肤。 */
export function useSkin(snapshot: ShallowRef<Snapshot | null>): void {
  const setting = useSetting(snapshot, "ui.skin");
  watch(
    () => (snapshot.value ? setting.value.id : null),
    (id) => {
      if (id == null) return;
      const applied = applySkin(id);
      try {
        localStorage.setItem(CACHE_KEY, applied);
      } catch {
        // 只影响下次启动的第一帧
      }
    },
    { immediate: true },
  );
}
