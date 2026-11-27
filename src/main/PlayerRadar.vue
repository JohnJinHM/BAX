<script setup lang="ts">
import { computed } from "vue";
import HoverTip from "../shared/HoverTip.vue";

import { useSetting } from "../shared/settings";
import { useSnapshot } from "../shared/engine";
import { axisRatio, calibratedAxes, referenceValue, type RadarAxis } from "./player-analysis";

const props = defineProps<{title:string; axes:RadarAxis[]; values:(number|null)[]; games:number; efficiencyGames?:number}>();
const baselines = useSetting(useSnapshot(), "ui.radarBaselines");
const axes = computed(() => calibratedAxes(props.axes, baselines.value));
const ranged = computed(() => axes.value.some(a => a.low != null));
const ratios = computed(() => props.values.map((value,i) =>
  value == null || !props.games ? null : axisRatio(axes.value[i],value)));
const max = computed(() => {
  const peak = Math.max(1,...ratios.value.map(n => n ?? 0));
  return ranged.value ? peak : Math.max(2,Math.ceil(peak));
});
const position = (i:number, r:number) => {
  const a = 2*Math.PI*i/props.axes.length;
  return {x:150+Math.sin(a)*r, y:106-Math.cos(a)*r};
};
const ring = (radii:number[]) => radii.map((r,i) => {
  const p=position(i,r); return p.x+","+p.y;
}).join(" ");
const grid = (r:number) => ring(props.axes.map(() => r));
const points = computed(() => ratios.value.map((n,i) => n == null ? null : position(i,68*(n/max.value))));
const complete = computed(() => points.value.every(p => p != null));
const shape = computed(() => points.value.flatMap(p => p ? [p.x+","+p.y] : []).join(" "));
const format = (v:number) => v.toLocaleString("zh-CN",{maximumFractionDigits:1});
function displayValue(i:number, compact = false) {
  const value = props.values[i];
  if (!props.games || value == null) return "–";
  const { key, unit } = axes.value[i];
  if (compact && key === "efficiency") return format(value)+"/分钟";
  return format(value)+(unit === "%" ? "" : " ")+unit;
}
const benchmark = computed(() => ring(axes.value.map(axis => 68 * axisRatio(axis, referenceValue(axis)) / max.value)));
const combatHints: Record<string,string> = {
  kills:"平均每场击毁的敌方单位数量，取自战报的官方击毁统计。",
  objectives:"每场战报记录的占点次数。",
  survival:"每场用 1 减去损失数占原出兵次数的比例，最低记为 0，再取场均。",
  efficiency:"每场的官方累计出兵分（含回收架次）除以整场分钟数，再对有效对局取平均。",
};
const tips = computed(() => [
  ...axes.value.map((axis,i) => {
    const rule = combatHints[axis.key] ?? ("每场"+axis.label+"出兵次数占八类作战单位总次数的比例，再取场均。运输、补给和其他单位不计入分母。");
    const reference = "参考基准 "+referenceValue(axis).toLocaleString("zh-CN",{maximumFractionDigits:3})+" "+axis.unit;
    const range = axis.low == null ? reference : reference+"（低值 "+axis.low+" · 高值 "+axis.base+"）";
    const sample = axis.key === "efficiency" ? "有效对局 "+(props.efficiencyGames ?? 0)+" 场。" : "";
    return {key:axis.key,title:axis.label,detail:rule+sample+"\n\n玩家场均 "+displayValue(i)+"  ·  "+range};
  }),
  {key:"average",title:"玩家场均",detail:"实线表示玩家在有效归档对局中的场均值。轴旁标注实际数值。"},
  {key:"baseline",title:"参考基准",detail:ranged.value
    ? "虚线表示各轴低值与高值的平均值。低值及以下位于中心。超过高值时按相同斜率外推，整张图同步扩大量程。上下限可在设置中调整。"
    : "虚线表示各兵种的参考出兵占比，可在设置中调整。"},
]);
</script>
<template>
  <section class="radar" :aria-label="title">
    <header><h4>{{ title }}</h4><small>{{ games }} 场</small></header>
    <HoverTip v-slot="{ trigger }" :items="tips">
    <svg viewBox="0 0 300 218" role="group" :aria-label="title + '，实线为玩家场均，虚线为参考基准'">
      <polygon v-for="fraction in [0.5,1]" :key="fraction" :points="grid(68*fraction)" class="grid" />
      <line v-for="(axis,i) in axes" :key="axis.label" x1="150" y1="106" :x2="position(i,68).x" :y2="position(i,68).y" class="grid" />
      <polygon :points="benchmark" class="benchmark" />
      <polygon v-if="games && complete" :points="shape" class="data" />
      <polyline v-else-if="games" :points="shape" class="data partial" />
      <g v-for="(axis,i) in axes" :key="axis.label" class="axis-tip" role="img" :aria-label="tips[i].title+'，'+tips[i].detail" v-bind="trigger(axis.key)">
        <circle v-if="points[i]" :cx="points[i]!.x" :cy="points[i]!.y" r="2.5" />
        <text :class="{ 'efficiency-label': axis.key === 'efficiency' }" :x="position(i,84).x" :y="position(i,85).y-2" :text-anchor="Math.abs(position(i,1).x-150)<0.1 ? 'middle' : position(i,1).x>150 ? 'start' : 'end'">
          {{ axis.label }}<tspan :x="position(i,84).x" dy="14">{{ displayValue(i, true) }}</tspan>
        </text>
      </g>
    </svg>
    <span class="legend"><span v-bind="trigger('average')"><i />玩家场均</span><span v-bind="trigger('baseline')"><i class="base" />参考基准</span></span>
    </HoverTip>
  </section>
</template>
<style scoped>
.radar {min-width:0;font-variant-numeric:tabular-nums;}header {display:flex;align-items:baseline;gap:8px;}
h4 {margin:0;font-size:calc(14px * var(--bax-font-scale));}small,.legend {color:var(--bax-muted);font-size:calc(12px * var(--bax-font-scale));}
svg {display:block;width:100%;max-height:224px;}.grid {stroke:var(--bax-border);fill:none;}.benchmark {fill:none;stroke:var(--bax-muted);stroke-dasharray:4 3;stroke-width:1.6;}
.data {fill:color-mix(in srgb,var(--bax-accent) 16%,transparent);stroke:var(--bax-accent);stroke-width:2;}.partial {fill:none;}circle {fill:var(--bax-accent);}
text {fill:var(--bax-fg);font-size:calc(12px * var(--bax-font-scale));}tspan {fill:var(--bax-muted);font-size:calc(11px * var(--bax-font-scale));}
/* 效率轴标签右对齐于 x=66，长数值向内移，给图表左边留出 2px。 */
.efficiency-label {transform-box:fill-box;transform:translateX(max(0px, calc(100% - 64px)));}
.legend {display:flex;align-items:center;justify-content:center;gap:6px;}.legend i {width:15px;border-top:2px solid var(--bax-accent);}.legend .base {margin-left:6px;border-top:2px dashed var(--bax-muted);}
.axis-tip,.legend span {cursor:help;}
.axis-tip text {pointer-events:bounding-box;}
.legend span {display:inline-flex;gap:5px;align-items:center;}
.legend span:focus-visible,.axis-tip:focus-visible {outline:1px solid var(--bax-accent);}
</style>
