<script setup lang="ts">
import { computed, ref } from "vue";
import type { SupplyPlayer } from "../shared/bindings/SupplyPlayer";
import type { UnitClass } from "../shared/bindings/UnitClass";
import type { Team } from "../shared/bindings/Team";
import { count, unitClassNames } from "../shared/format";
import type { TreeGroup, TreeTile } from "./treemap-layout";
import { modelKey } from "./deployment-models";
import TreemapUnitDetail from "./TreemapUnitDetail.vue";
import AnalysisTreemap from "./AnalysisTreemap.vue";

const props = defineProps<{ players: SupplyPlayer[]; teams: Team[]; teamLabel: (team: Team) => string; teamColor: (team: Team | null) => string }>();
const mode = ref<"tree" | "exchange">("tree");
const selection = ref<{team: Team; kind: UnitClass} | null>(null);
const selectedUnit = ref<{owner:string; key:string} | null>(null);
const sides = computed(() => props.teams.map(team => {
  const players = props.players.filter(p => p.team === team);
  return {team, players, label:props.teamLabel(team), color:props.teamColor(team), total:players.reduce((n,p) => n+p.consumed,0)};
}));
const percentage = (v: number, total: number) => total ? (v / total * 100).toLocaleString("zh-CN", {maximumFractionDigits:1}) + "%" : "0%";
const groups = computed<TreeGroup[]>(() => {
  const focus = selection.value;
  if (!focus) return sides.value.map(s => {
    const totals = new Map<UnitClass,number>();
    for (const p of s.players) for (const m of p.models) totals.set(m.class,(totals.get(m.class) ?? 0)+m.points);
    const tiles = [...totals].map(([key,value]) => ({key,label:unitClassNames[key],value,share:percentage(value,s.total)}));
    return {key:s.team,label:s.label,color:s.color,tiles};
  });
  const side = sides.value.find(s => s.team === focus.team)!;
  return side.players.map(p => {
    const tiles: TreeTile[] = p.models.filter(m => m.class === focus.kind).map(m => ({
      key:modelKey(m.id,m.name),label:m.name,value:m.points,share:percentage(m.points,side.total),
      icon:m.id == null ? undefined : {id:m.id, name:m.name, kind:m.class, thumbnail:m.thumbnail},
    }));
    return {key:String(p.id),label:p.name,color:side.color,tiles};
  }).filter(g => g.tiles.length);
});
const total = computed(() => sides.value.reduce((n,s) => n+s.total,0));
const maxExchange = computed(() => Math.max(0,...props.players.flatMap(p => [p.fromAllies,p.byAllies])));
const discrepancy = computed(() => props.players.some(p => p.models.reduce((n,m) => n+m.points,0) > p.consumed));
const owner = computed(() => props.players.find(p => p.team === selection.value?.team && String(p.id) === selectedUnit.value?.owner));
const model = computed(() => owner.value?.models.find(m => m.class === selection.value?.kind && modelKey(m.id,m.name) === selectedUnit.value?.key));
const side = computed(() => sides.value.find(s => s.team === selection.value?.team));
const extra = computed(() => {
  if (!model.value || !owner.value || !side.value) return [];
  const m = model.value;
  return [
    ...(!m.activity ? [{label:"补给消耗（点）",value:count(m.points)}] : []),
    {label:"占本队消耗",value:percentage(m.points,side.value.total)},
    {label:"占该玩家消耗",value:percentage(m.points,owner.value.consumed)},
  ];
});
function drill(group:string, key:string) {
  if (selection.value) selectedUnit.value = {owner:group,key};
  else { selection.value = {team:group as Team,kind:key as UnitClass}; selectedUnit.value = null; }
}
function back() {
  if (model.value) selectedUnit.value = null;
  else { selectedUnit.value = null; selection.value = null; }
}
</script>
<template>
  <section class="supply" aria-label="补给分析">
    <h3>补给分析</h3>
    <div class="tabs" role="group" aria-label="补给视图">
      <button :aria-pressed="mode === 'tree'" @click="mode = 'tree'">补给消耗</button>
      <button :aria-pressed="mode === 'exchange'" @click="mode = 'exchange'">队友补给往来</button>
    </div>
    <div v-show="mode === 'tree'">
      <div class="totals"><div><span>全场消耗</span><b>{{ count(total) }} <small>点</small></b></div>
        <div v-for="s in sides" :key="s.team"><span>{{ s.label }}</span><b>{{ count(s.total) }} <small>点</small></b></div>
      </div>
      <AnalysisTreemap :groups="groups" unit="点" label="补给消耗矩形树图" :depth="model ? 2 : selection ? 1 : 0"
        :view-key="JSON.stringify([selection,owner?.id,model?.id,model?.name])" :detail="!!model" @choose="drill" @back="back">
        <template #path>全场<template v-if="selection"> › {{ teamLabel(selection.team) }} › {{ unitClassNames[selection.kind] }}</template><template v-if="owner && model"> › {{ owner.name }} › {{ model.name }}</template></template>
        <template #total><template v-if="model">{{ count(model.points) }} 点</template></template>
        <template #detail><TreemapUnitDetail v-if="owner && model && side" :name="model.name" :color="side.color"
          :icon="model.id == null ? undefined : {id:model.id,name:model.name,kind:model.class,thumbnail:model.thumbnail}"
          :activity="model.activity" :extra="extra"
          :scope="owner.name+' · '+(model.activity ? '仅统计消耗过补给的实例，归于最终持有者' : '战报总量与已归类单位消耗的差额')" /></template>
      </AnalysisTreemap>
      <p v-if="discrepancy">部分单位明细超过战报的玩家总量，顶部总量取自玩家记录。</p>
      <p v-if="!model">{{ selection ? "按消耗补给的玩家和型号分组。选择型号查看消耗明细。" : "百分比为本队占比。选择兵种查看各玩家和型号的消耗。" }}</p>
    </div>
    <template v-if="mode === 'exchange'">
      <div class="bar-head"><span>玩家</span><span>← 使用队友补给</span><span>队友使用其补给 →</span></div>
      <div v-for="s in sides" :key="s.team" class="exchange"><h4>{{ s.label }}</h4>
        <div v-for="p in s.players" :key="p.id" class="bar-row" :style="{'--team':s.color}">
          <span class="player">{{ p.name }}</span>
          <span class="half left"><i :style="{width:(p.fromAllies/(maxExchange || 1)*100)+'%'}" /><b>{{ count(p.fromAllies) }}</b></span>
          <span class="half right"><i :style="{width:(p.byAllies/(maxExchange || 1)*100)+'%'}" /><b>{{ count(p.byAllies) }}</b></span>
        </div>
      </div>
      <p>每侧刻度为 0–{{ count(maxExchange) }} 点，两侧共用中央零线。</p>
    </template>
  </section>
</template>
<style scoped>
.supply {margin:24px 0;container-type:inline-size;font-variant-numeric:tabular-nums;}
h3,h4 {margin:0;color:var(--bax-fg);}h4 {font-size:calc(13px * var(--bax-font-scale));margin:10px 0 4px;}
.tabs {display:flex;gap:16px;border-bottom:1px solid var(--bax-border);margin:8px 0 14px;}
.tabs button {border:0;background:transparent;color:var(--bax-muted);border-radius:0;padding:7px 0;}
.tabs button[aria-pressed=true] {color:var(--bax-fg);box-shadow:inset 0 -2px var(--bax-accent);}
.totals {display:flex;flex-wrap:wrap;gap:14px clamp(18px,4vw,52px);margin:10px 0 14px;}
.totals div {display:grid;gap:3px;}.totals b {font-size:calc(20px * var(--bax-font-scale));}.totals span,small,p {font-size:calc(12px * var(--bax-font-scale));color:var(--bax-muted);}
.bar-row,.bar-head {display:grid;grid-template-columns:minmax(75px,1fr) repeat(2,minmax(0,2fr));align-items:center;}
.bar-head {font-size:calc(12px * var(--bax-font-scale));color:var(--bax-muted);gap:4px;}.bar-head span:nth-child(2) {text-align:right;padding-right:8px;}
.bar-row {min-height:32px;border-bottom:1px solid var(--bax-border);}.player {overflow-wrap:anywhere;padding-right:8px;font-size:calc(13px * var(--bax-font-scale));}
.half {position:relative;display:flex;align-items:center;height:26px;}.half i {position:absolute;bottom:2px;height:5px;background:var(--team);}
.half b {font-size:calc(12px * var(--bax-font-scale));font-weight:500;line-height:1;padding:0 6px 7px;}
.left {justify-content:flex-end;border-right:1px solid var(--bax-faint);}.left i {right:0;background:color-mix(in srgb,var(--team) 55%,var(--bax-muted));}.right i {left:0;}
</style>
