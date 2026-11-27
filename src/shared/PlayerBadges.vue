<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch } from "vue";
import type { CSSProperties } from "vue";
import type { RosterRow } from "./roster";

const props = defineProps<{ row: Pick<RosterRow, "labels" | "traceCount" | "profile"> }>();
const names: Record<string, string> = {
  作: "作者", 挂: "出兵分超限", 标: "社区标记", 远: "远程火力异常", 效: "单位击杀效率",
  小: "疑似小号", 空: "固定翼补给占比", 车: "疑似组排",
};
const order = "作挂标远效小空车";
const badges = computed(() => {
  const { labels, traceCount, profile } = props.row;
  const list = [...labels];
  if (traceCount != null && traceCount > 0) {
    list.push({ symbol: "标", detail: `BATrace 社区标记次数大于 0。\n\n共 ${traceCount} 次标记` });
  }
  const level = profile?.level;
  const rating = profile?.rating;
  if (level != null && rating != null && ((level < 30 && rating > 2000) || (level < 50 && rating > 2500))) {
    list.push({
      symbol: "小",
      detail: [
        "满足以下任一项：",
        "① 等级低于 30，且 ELO 高于 2000。",
        "② 等级低于 50，且 ELO 高于 2500。",
        "",
        `当前等级 ${level}  ·  ELO ${rating}`,
      ].join("\n"),
    });
  }
  return list.sort((a, b) => order.indexOf(a.symbol) - order.indexOf(b.symbol));
});

const tooltipId = useId();
const symbol = ref("");
const active = computed(() => badges.value.find(b => b.symbol === symbol.value));
const position = ref<CSSProperties>({});
const tooltip = ref<HTMLElement>();
let anchor: HTMLElement | null = null;
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
  anchor = event.currentTarget as HTMLElement;
  symbol.value = value;
  place();
}
function close() {
  clearTimeout(timer);
  symbol.value = "";
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
</script>

<template>
  <span v-for="b in badges" :key="b.symbol" class="badge" :data-symbol="b.symbol" tabindex="0"
    :aria-label="names[b.symbol]" :aria-describedby="active?.symbol === b.symbol ? tooltipId : undefined"
    @mouseenter="open($event, b.symbol)" @mouseleave="leave" @focus="open($event, b.symbol)" @blur="close"
    @keydown.esc="close" @keydown.down.prevent="tooltip?.scrollBy(0, 40)" @keydown.up.prevent="tooltip?.scrollBy(0, -40)">
    {{ b.symbol }}
  </span>
  <Teleport to="body">
    <div v-if="active" :id="tooltipId" ref="tooltip" role="tooltip" class="label-tip" :style="position"
      @mouseenter="keepOpen" @mouseleave="leave">
      <strong>{{ active.symbol }} · {{ names[active.symbol] }}</strong>
      <div>{{ active.detail }}</div>
    </div>
  </Teleport>
</template>

<style scoped>
.badge { display:inline-grid; place-items:center; flex:none; width:1.45em; height:1.45em; border:1px solid currentColor; border-radius:3px; font-size:.86em; line-height:1; color:#f2c14e; background:var(--bax-surface); cursor:help; }
.badge[data-symbol="作"] { color: #2b2005; background: #edc462; border-color: #ffdf8a; font-weight: 700; }
.badge[data-symbol="挂"] { color:var(--bax-danger,#ff7575); }
.badge[data-symbol="车"] { color:var(--bax-info,#77baff); }
.label-tip { position:fixed; z-index:1000; box-sizing:border-box; overflow:auto; overscroll-behavior:contain; padding:8px 10px; border:1px solid var(--bax-border); border-radius:var(--bax-radius-sm); background:var(--bax-surface,#18212a); color:var(--bax-fg); text-align:left; font-size:calc(12px * var(--bax-font-scale)); line-height:1.5; }
.label-tip strong { display:block; margin-bottom:3px; }
.label-tip div { white-space:pre-line; overflow-wrap:anywhere; font-variant-numeric:tabular-nums; }
</style>
