<script setup lang="ts">
import { computed } from "vue";
import type { ConsumerActivity } from "../shared/bindings/ConsumerActivity";
import type { TreeTile } from "./treemap-layout";
import UnitIcon from "./UnitIcon.vue";

const props = defineProps<{
  name:string; scope:string; color:string; icon?:TreeTile["icon"]; activity:ConsumerActivity | null;
  extra:{ label:string; value:string }[];
}>();
const number = new Intl.NumberFormat("zh-CN",{maximumFractionDigits:2});
const metrics = computed(() => {
  const a = props.activity;
  if (!a) return props.extra;
  const m = a.metrics;
  const figures: [string, number, string?][] = [
    ["数量",a.count],["击毁单位",a.kills],["自身阵亡",a.dead],["回收",a.refunded],
    ["摧毁建筑",m.buildings],["造成伤害",a.damage],["承受伤害",m.damageReceived],["补给消耗",m.supply," 点"],
    ["造成友伤",m.friendlyDamageDealt],["受到友伤",m.friendlyDamageReceived],
    ["自身伤害",m.selfDamage],["自毁",m.selfDestroyed],["空投",m.airdropped],
  ];
  return [...figures.map(([label,value,unit = ""]) => ({label,value:number.format(value)+unit})), ...props.extra];
});
</script>
<template>
  <section class="unit-detail" :style="{'--team':color}" :aria-label="name+'单位详情'">
    <header><UnitIcon v-if="icon" v-bind="icon" /><div><h4>{{ name }}</h4><small>{{ scope }}</small></div></header>
    <dl v-if="activity" class="key-figures"><div v-for="item in metrics.slice(0,4)" :key="item.label"><dt>{{ item.label }}</dt><dd>{{ item.value }}</dd></div></dl>
    <dl><div v-for="item in activity ? metrics.slice(4) : metrics" :key="item.label"><dt>{{ item.label }}</dt><dd>{{ item.value }}</dd></div></dl>
  </section>
</template>
<style scoped>
.unit-detail {padding:16px 18px;border-top:3px solid var(--team);font-variant-numeric:tabular-nums;}
header {display:flex;align-items:center;gap:12px;margin:0 0 14px;}header :deep(.unit-icon) {width:72px;height:40px;}
h4 {margin:0 0 3px;color:var(--bax-fg);font-size:calc(18px * var(--bax-font-scale));}small,dt {color:var(--bax-muted);font-size:calc(12px * var(--bax-font-scale));}
dl {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,115px),1fr));gap:10px 16px;margin:0;}
dl div {border-top:1px solid var(--bax-divider);padding-top:5px;}dd {margin:2px 0 0;font-size:calc(15px * var(--bax-font-scale));font-weight:600;}
.key-figures {grid-template-columns:repeat(4,minmax(0,1fr));margin-bottom:15px;gap:6px 16px;}
.key-figures dd {color:var(--bax-fg);font-size:calc(22px * var(--bax-font-scale));}
@container (max-width: 420px) {.key-figures {grid-template-columns:repeat(2,minmax(0,1fr));}}
</style>
