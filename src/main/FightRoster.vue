<script setup lang="ts">
import { computed } from "vue";
import type { PlayerFight } from "../shared/bindings/PlayerFight";
import { teamNames } from "../shared/format";
const props = defineProps<{fight:PlayerFight; playerId:number}>();
const sides = computed(() => {
  const winner = props.fight.winner;
  const teams = winner === "bravo" ? ["bravo","alpha"] as const : ["alpha","bravo"] as const;
  return teams.map(team => ({team, members:props.fight.roster.filter(p => p.team === team),
    label:winner == null ? teamNames[team] : team === winner ? "胜方" : "败方"}));
});
</script>
<template>
  <span class="rosters">
    <span v-for="s in sides" :key="s.team" class="side">
      <b>{{ s.label }}</b>
      <span class="members"><span v-for="p in s.members" :key="p.id" :class="{ current:p.id === playerId }" :title="p.id === playerId ? '当前档案玩家' : undefined">{{ p.name ?? 'ID '+p.id }}</span>
        <small v-if="!s.members.length">名单未记录</small>
      </span>
    </span>
  </span>
</template>
<style scoped>
.rosters {grid-column:1/-1;display:grid;grid-template-columns:1fr 1fr;gap:8px 20px;padding:6px 0 0;border-top:1px solid var(--bax-divider);}
.side {display:flex;align-items:baseline;gap:8px;min-width:0;}.side>b {flex:none;font-size:calc(12px * var(--bax-font-scale));color:var(--bax-muted);}
.members {display:flex;flex-wrap:wrap;align-items:baseline;gap:2px 9px;font-size:calc(12px * var(--bax-font-scale));color:var(--bax-soft);}
.members span {overflow-wrap:anywhere;}.current {font-weight:700;color:var(--bax-fg);text-decoration:underline;text-decoration-color:var(--bax-accent);text-underline-offset:3px;}
@container (max-width:560px) {.rosters {grid-template-columns:1fr;}}
</style>
