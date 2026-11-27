<script setup lang="ts">
import { computed, ref } from "vue";
import type { PlayerReport } from "../shared/bindings/PlayerReport";
import type { Team } from "../shared/bindings/Team";
import type { TreeGroup } from "./treemap-layout";
import { count, mmss, unitClassNames } from "../shared/format";
import { deploymentModels } from "./deployment-models";
import AnalysisTreemap from "./AnalysisTreemap.vue";
import TreemapUnitDetail from "./TreemapUnitDetail.vue";

const props = defineProps<{ players: PlayerReport[]; teams: Team[]; teamLabel: (team: Team) => string; teamColor: (team: Team | null) => string }>();
const playerId = ref<number>();
const unitKey = ref<string>();
const player = computed(() => props.players.find(p => p.id === playerId.value));
const models = computed(() => deploymentModels(player.value?.units ?? []));
const model = computed(() => unitKey.value ? models.value.get(unitKey.value) : undefined);
const playerName = (p: PlayerReport) => p.name ?? "ID " + p.id;
const teamName = (t: Team | null) => t ? props.teamLabel(t) : "队伍未知";
const groups = computed<TreeGroup[]>(() => {
  if (player.value) {
    const p = player.value;
    return [{key:String(p.id),color:props.teamColor(p.team),tiles:[...models.value].map(([key,u]) => ({
      key,label:u.unitName,value:u.deployedScore,icon:{id:u.unitId,name:u.unitName,kind:u.class,thumbnail:u.thumbnail},
    }))}];
  }
  const teams = props.players.some(p => p.team == null) ? [...props.teams,null] : props.teams;
  return teams.map(t => ({
    key:t ?? "unknown",label:teamName(t),color:props.teamColor(t),
    tiles:props.players.filter(p => p.team === t).map(p => ({
      key:String(p.id),label:playerName(p),value:p.units.reduce((n,u) => n+u.deployedScore,0),
    })),
  }));
});
const points = computed(() => model.value?.deployedScore ?? groups.value.reduce((n,g) => n+g.tiles.reduce((sum,t) => sum+t.value,0),0));
const extra = computed(() => {
  const u = model.value;
  if (!u) return [];
  const time = (s:number|null) => s == null ? "–" : mmss(s*1000);
  return [
    {label:"累计出兵分（估算）",value:count(u.deployedScore)}, {label:"净投入（估算）",value:count(u.value)},
    {label:"回收分数（估算）",value:count(u.deployedScore-u.value)}, {label:"转交队友",value:count(u.transferred)},
    {label:"首次出兵",value:time(u.firstSpawnSecs)}, {label:"末次出兵",value:time(u.lastSpawnSecs)},
  ];
});
function choose(_:string, key:string) {
  if (player.value) unitKey.value = key;
  else { playerId.value = Number(key); unitKey.value = undefined; }
}
function back() {
  if (model.value) unitKey.value = undefined;
  else playerId.value = undefined;
}
</script>
<template>
  <div class="drill">
    <AnalysisTreemap :groups="groups" unit="分" label="出兵分数矩形树图" :depth="model ? 2 : player ? 1 : 0"
      :view-key="JSON.stringify([player?.id,model?.variantKey])" :detail="!!model" @choose="choose" @back="back">
      <template #path>全场<template v-if="player"> › {{ teamName(player.team) }} › {{ playerName(player) }}</template><template v-if="model"> › {{ model.unitName }}</template></template>
      <template #total>{{ count(points) }} 分</template>
      <template #detail><TreemapUnitDetail v-if="player && model" :name="model.unitName" :color="teamColor(player.team)"
        :icon="{id:model.unitId,name:model.unitName,kind:model.class,thumbnail:model.thumbnail}" :activity="model"
        :scope="playerName(player)+' · '+unitClassNames[model.class]+' · 按原出兵者统计'" :extra="extra" /></template>
    </AnalysisTreemap>
    <p v-if="!model">面积按累计估算出兵分数计算，含回收架次。{{ player ? "选择型号查看单位详情。" : "选择玩家查看单位型号。" }}</p>
  </div>
</template>
<style scoped>
p {font-size:calc(12px * var(--bax-font-scale));color:var(--bax-muted);}
</style>
