<script setup lang="ts">
import { computed } from "vue";

import { useSetting } from "../shared/settings";
import { useSnapshot } from "../shared/engine";
import { calibratedAxes, type RadarAxis } from "./player-analysis";

const props = defineProps<{title:string; axes:RadarAxis[]; values:number[]; games:number}>();
const baselines = useSetting(useSnapshot(), "ui.radarBaselines");
const axes = computed(() => calibratedAxes(props.axes, baselines.value));
const max = computed(() => Math.max(2, Math.ceil(Math.max(...props.values.map((n,i) => n/axes.value[i].base)))));
const position = (i:number, r:number) => {
  const a = 2*Math.PI*i/props.axes.length;
  return {x:150+Math.sin(a)*r, y:106-Math.cos(a)*r};
};
const ring = (radii:number[]) => radii.map((r,i) => {
  const p=position(i,r); return p.x+","+p.y;
}).join(" ");
const grid = (r:number) => ring(props.axes.map(() => r));
const shape = computed(() => ring(props.values.map((v,i) => 68*v/axes.value[i].base/max.value)));
const format = (v:number) => v.toLocaleString("zh-CN",{maximumFractionDigits:1});
</script>
<template>
  <section class="radar" :aria-label="title">
    <header><h4>{{ title }}</h4><small>{{ games }} 场</small></header>
    <svg viewBox="0 0 300 218" role="img" :aria-label="title + '，实线为玩家场均，虚线为参考基准'">
      <polygon v-for="fraction in [0.5,1]" :key="fraction" :points="grid(68*fraction)" class="grid" />
      <line v-for="(axis,i) in axes" :key="axis.label" x1="150" y1="106" :x2="position(i,68).x" :y2="position(i,68).y" class="grid" />
      <polygon :points="grid(68/max)" class="benchmark" />
      <polygon v-if="games" :points="shape" class="data" />
      <g v-for="(axis,i) in axes" :key="axis.label">
        <title>{{ axis.label }} · 玩家 {{ games ? format(values[i])+axis.unit : '暂无数据' }} · 基准 {{ format(axis.base) }}{{ axis.unit }}</title>
        <circle v-if="games" :cx="position(i,68*values[i]/axis.base/max).x" :cy="position(i,68*values[i]/axis.base/max).y" r="2.5" />
        <text :x="position(i,84).x" :y="position(i,85).y-2" :text-anchor="Math.abs(position(i,1).x-150)<0.1 ? 'middle' : position(i,1).x>150 ? 'start' : 'end'">
          {{ axis.label }}<tspan :x="position(i,84).x" dy="14">{{ games ? format(values[i])+axis.unit : '–' }}</tspan>
        </text>
      </g>
    </svg>
    <span class="legend"><i />玩家场均 <i class="base" />参考基准</span>
  </section>
</template>
<style scoped>
.radar {min-width:0;font-variant-numeric:tabular-nums;}header {display:flex;align-items:baseline;gap:8px;}
h4 {margin:0;font-size:calc(14px * var(--bax-font-scale));}small,.legend {color:var(--bax-muted);font-size:calc(12px * var(--bax-font-scale));}
svg {display:block;width:100%;max-height:224px;}.grid {stroke:var(--bax-border);fill:none;}.benchmark {fill:none;stroke:var(--bax-muted);stroke-dasharray:4 3;stroke-width:1.6;}
.data {fill:color-mix(in srgb,var(--bax-accent) 16%,transparent);stroke:var(--bax-accent);stroke-width:2;}circle {fill:var(--bax-accent);}
text {fill:var(--bax-fg);font-size:calc(12px * var(--bax-font-scale));}tspan {fill:var(--bax-muted);font-size:calc(11px * var(--bax-font-scale));}
.legend {display:flex;align-items:center;justify-content:center;gap:6px;}.legend i {width:15px;border-top:2px solid var(--bax-accent);}.legend .base {margin-left:6px;border-top:2px dashed var(--bax-muted);}
</style>
