import { onScopeDispose, ref, type Ref } from "vue";

/** 每隔一段时间更新的当前时间（UTC 毫秒），用于界面自己计时。 */
export function useNow(intervalMs = 500): Ref<number> {
  const now = ref(Date.now());
  const timer = window.setInterval(() => {
    now.value = Date.now();
  }, intervalMs);
  onScopeDispose(() => window.clearInterval(timer));
  return now;
}
