<script setup lang="ts">
import { computed } from "vue";
import type { PlayerFight } from "../shared/bindings/PlayerFight";
import { preferredModels } from "./player-analysis";
import UnitIcon from "./UnitIcon.vue";
const props = defineProps<{fights:PlayerFight[]}>();
const sections = computed(() => [
  {title:"偏好单位", scope:"全场", ...preferredModels(props.fights,false)},
  {title:"开局偏好", scope:"前 3 分钟", ...preferredModels(props.fights,true)},
]);
</script>
<template>
  <section v-for="s in sections" :key="s.title" class="preferences">
    <h3>{{ s.title }}</h3><p class="hint">{{ s.scope }} · 最近 20 场内 {{ s.samples }} 场有效记录 · 按出场率排序</p>
    <ol v-if="s.top.length">
      <li v-for="{unit,games} in s.top" :key="unit.id">
        <UnitIcon :id="unit.id" :name="unit.name" :kind="unit.class" />
        <span>{{ unit.name }}</span><b>{{ Math.round(games/s.samples*100) }}%</b>
        <small>{{ games }}/{{ s.samples }} 场</small>
      </li>
    </ol><p v-else class="hint">暂无可统计的单位。</p>
  </section>
</template>
<style scoped>
h3 {margin-bottom:4px;}p {margin:4px 0 10px;}
ol {list-style:none;display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,105px),1fr));gap:12px;padding:0;margin:0;}
li {display:grid;grid-row:span 4;grid-template-rows:subgrid;row-gap:3px;align-items:end;overflow-wrap:anywhere;font-size:calc(13px * var(--bax-font-scale));}
b,small {font-variant-numeric:tabular-nums;}small {color:var(--bax-muted);}
</style>
