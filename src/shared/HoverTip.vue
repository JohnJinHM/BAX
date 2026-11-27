<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch, type CSSProperties } from "vue";

const props = defineProps<{items: {key:string; title:string; detail:string}[]}>();
const tooltipId = useId();
const key = ref("");
const active = computed(() => props.items.find(item => item.key === key.value));
const position = ref<CSSProperties>({});
const tooltip = ref<HTMLElement>();
let anchor: Element | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

function place() {
  if (!anchor?.isConnected) return close();
  const rect = anchor.getBoundingClientRect();
  const margin = 6;
  const width = Math.min(390, window.innerWidth - 2 * margin);
  const below = window.innerHeight - rect.bottom - 2 * margin;
  const above = rect.top - 2 * margin;
  const useAbove = below < 220 && above > below;
  position.value = {
    width: `${width}px`,
    left: `${Math.max(margin, Math.min(rect.left, window.innerWidth - width - margin))}px`,
    ...(useAbove ? { bottom: `${window.innerHeight - rect.top + margin}px` } : { top: `${rect.bottom + margin}px` }),
    maxHeight: `${Math.max(1, useAbove ? above : below)}px`,
  };
}
function open(event: Event, value: string) {
  clearTimeout(timer);
  anchor = event.currentTarget as Element;
  key.value = value;
  place();
}
function close() {
  clearTimeout(timer);
  key.value = "";
  anchor = null;
}
function leave() {
  clearTimeout(timer);
  timer = setTimeout(close, 140);
}
function keepOpen() {
  clearTimeout(timer);
}
watch(active, (badge, _, cleanup) => {
  if (!badge) return;
  if (tooltip.value) tooltip.value.scrollTop = 0;
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
  cleanup(() => {
    window.removeEventListener("resize", place);
    window.removeEventListener("scroll", place, true);
  });
});
onBeforeUnmount(() => clearTimeout(timer));
function trigger(value: string) {
  return {
    tabindex: 0,
    "aria-describedby": active.value?.key === value ? tooltipId : undefined,
    onMouseenter: (event: MouseEvent) => open(event,value),
    onMouseleave: leave,
    onFocus: (event: FocusEvent) => open(event,value),
    onBlur: close,
    onKeydown(event: KeyboardEvent) {
      if (event.key === "Escape" && active.value) { event.stopPropagation(); close(); }
      if (active.value && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        event.preventDefault();
        tooltip.value?.scrollBy(0,event.key === "ArrowDown" ? 40 : -40);
      }
    },
  };
}
</script>
<template>
  <slot :trigger="trigger" />
  <Teleport to="body">
    <div v-if="active" :id="tooltipId" ref="tooltip" role="tooltip" class="label-tip" :style="position"
      @mouseenter="keepOpen" @mouseleave="leave">
      <strong>{{ active.title }}</strong><div>{{ active.detail }}</div>
    </div>
  </Teleport>
</template>
<style scoped>
.label-tip { position:fixed; z-index:1000; box-sizing:border-box; overflow:auto; overscroll-behavior:contain; padding:8px 10px; border:1px solid var(--bax-border); border-radius:var(--bax-radius-sm); background:var(--bax-surface,#18212a); color:var(--bax-fg); text-align:left; font-size:calc(12px * var(--bax-font-scale)); line-height:1.5; }
.label-tip strong { display:block; margin-bottom:3px; }
.label-tip div { white-space:pre-line; overflow-wrap:anywhere; font-variant-numeric:tabular-nums; }
</style>
