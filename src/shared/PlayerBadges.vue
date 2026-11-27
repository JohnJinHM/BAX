<script setup lang="ts">
import { computed } from "vue";
import HoverTip from "./HoverTip.vue";
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

const tips = computed(() => badges.value.map(b => ({
  key:b.symbol, title:b.symbol+" · "+names[b.symbol], detail:b.detail,
})));
</script>
<template>
  <HoverTip v-slot="{ trigger }" :items="tips">
    <span v-for="b in badges" :key="b.symbol" class="badge" :data-symbol="b.symbol"
      :aria-label="names[b.symbol]" v-bind="trigger(b.symbol)">{{ b.symbol }}</span>
  </HoverTip>
</template>
<style scoped>
.badge { display:inline-grid; place-items:center; flex:none; width:1.45em; height:1.45em; border:1px solid currentColor; border-radius:3px; font-size:.86em; line-height:1; color:#f2c14e; background:var(--bax-surface); cursor:help; }
.badge[data-symbol="作"] { color: #2b2005; background: #edc462; border-color: #ffdf8a; font-weight: 700; }
.badge[data-symbol="挂"] { color:var(--bax-danger,#ff7575); }
.badge[data-symbol="车"] { color:var(--bax-info,#77baff); }
</style>
