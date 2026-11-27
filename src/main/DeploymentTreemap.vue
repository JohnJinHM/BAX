<script setup lang="ts">
import { computed, ref } from "vue";
import type { PlayerReport } from "../shared/bindings/PlayerReport";
import type { Team } from "../shared/bindings/Team";
import type { TreeTile } from "./treemap-layout";
import { count } from "../shared/format";
import AnalysisTreemap from "./AnalysisTreemap.vue";

const props = defineProps<{ players: PlayerReport[]; teams: Team[]; teamLabel: (team: Team) => string; teamColor: (team: Team | null) => string }>();
const teamKey = ref<string>();
const playerId = ref<number>();
const label = (t: Team | null) => t ? props.teamLabel(t) : "队伍未知";
const sides = computed(() => [...props.teams, ...(props.players.some(p => p.team == null) ? [null] : [])]
  .map(t => ({ key:t ?? "unknown", label:label(t), color:props.teamColor(t), players:props.players.filter(p => p.team === t) })));
const side = computed(() => sides.value.find(t => t.key === teamKey.value));
const player = computed(() => side.value?.players.find(p => p.id === playerId.value));
const playerName = (p: PlayerReport) => p.name ?? "ID " + p.id;
const total = (p: PlayerReport) => p.units.reduce((n,u) => n + u.deployedScore, 0);
const tiles = computed<TreeTile[]>(() => {
  if (!side.value) return sides.value.map(t => ({key:t.key, label:t.label, color:t.color, value:t.players.reduce((n,p) => n+total(p),0)}));
  if (!player.value) return side.value.players.map(p => ({key:String(p.id), label:playerName(p), value:total(p)}));
  const models = new Map<string, TreeTile>();
  for (const u of player.value.units) {
    const key = JSON.stringify([u.unitId,u.unitName]);
    const model = models.get(key);
    if (model) model.value += u.deployedScore;
    else models.set(key, {key,label:u.unitName,value:u.deployedScore,icon:{id:u.unitId,name:u.unitName,kind:u.class,thumbnail:u.thumbnail}});
  }
  return [...models.values()].sort((a,b) => b.value-a.value);
});
function reset() { teamKey.value = undefined; playerId.value = undefined; }
function choose(_: string, key: string) { if (side.value) playerId.value = Number(key); else teamKey.value = key; }
</script>
<template>
  <div class="drill">
    <nav aria-label="出兵分析层级">
      <button :disabled="!side" @click="reset">全场</button>
      <template v-if="side"><span>›</span><button :disabled="!player" @click="playerId = undefined">{{ side.label }}</button></template>
      <template v-if="player"><span>›</span><strong>{{ playerName(player) }}</strong></template>
      <b>{{ count(tiles.reduce((n,t) => n+t.value,0)) }} 分</b>
    </nav>
    <AnalysisTreemap :groups="[{key:'deployments',color:side?.color ?? 'var(--bax-accent)',tiles}]" unit="分" label="出兵分数矩形树图" :drill="!player" @choose="choose" />
    <p>面积按累计估算出兵分数计算，含回收架次。{{ player ? '同型号的重复出兵已合计。' : side ? '选择玩家查看单位型号。' : '选择队伍查看玩家投入。' }}</p>
  </div>
</template>
<style scoped>
nav {display:flex;align-items:center;gap:8px;margin:8px 0;flex-wrap:wrap;}nav b {margin-left:auto;font-variant-numeric:tabular-nums;}
nav button {background:transparent;color:var(--bax-accent);padding:4px 2px;border:0;}nav button:disabled {color:var(--bax-fg);opacity:1;}p {font-size:calc(12px * var(--bax-font-scale));color:var(--bax-muted);}
</style>
