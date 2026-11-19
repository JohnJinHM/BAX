<script setup lang="ts">
import { computed } from "vue";
import type { PregameView } from "./bindings/PregameView";
import { percent, rating } from "./format";
const props = defineProps<{ view: PregameView; compact?: boolean }>();
const groups = computed(() => ["left", "right"].map(side => ({
  side, title: side === "left" ? "画面左侧" : "画面右侧",
  players: props.view.players.filter(p => p.observed.side === side),
})).filter(g => g.players.length));
</script>

<template>
  <section v-if="view.status || view.players.length" class="pregame" :class="{ compact }" aria-label="赛前候选名单">
    <div class="pregame-head"><strong>赛前预览</strong><span>待日志确认</span></div>
    <p v-if="!view.players.length || view.status.includes('查询') || view.status.includes('安装')" class="status" role="status">{{ view.status }}</p>
    <div v-for="group in groups" :key="group.side" class="side">
      <h3>{{ group.title }}</h3>
      <ul>
        <li v-for="p in group.players" :key="p.observed.row">
          <div class="identity"><span class="nickname" :title="p.observed.name">{{ p.observed.name }}</span><span class="level">Lv{{ p.observed.level }}</span></div>
          <div v-if="p.ambiguous" class="result">{{ p.candidates.length > 1 ? "多个可能账号，等待确认" : "账号待确认" }}</div>
          <div v-else-if="p.candidates[0]" class="result">
            <span :title="`候选 ID ${p.candidates[0].id} · ${p.candidates[0].name}`">{{ p.candidates[0].local ? '本地候选' : '候选' }}</span>
            <span v-if="!compact" class="candidate-name">{{ p.candidates[0].name }} · ID {{ p.candidates[0].id }}</span>
            <span v-if="p.candidates[0].profile">ELO {{ rating(p.candidates[0].profile.rating) }} · 胜率 {{ percent(p.candidates[0].profile.ranked?.winRate) }}</span>
          </div>
          <div v-else class="result">尚未匹配账号</div>
        </li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.pregame { margin-block: 12px; font-size: 13px; }
.pregame-head { display: flex; align-items: baseline; gap: 12px; }
.pregame-head span, .status, .result, .side h3 { color: var(--bax-muted); }
.status { margin: 8px 0; }
.side h3 { font-size: 12px; font-weight: 500; margin: 14px 0 4px; }
ul { list-style: none; padding: 0; margin: 0; }
li { padding: 6px 0; border-bottom: 1px solid var(--bax-border); }
.identity { display: flex; gap: 12px; min-width: 0; }
.nickname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.level { margin-left: auto; font-variant-numeric: tabular-nums; white-space: nowrap; }
.result { display: flex; gap: 10px; flex-wrap: wrap; font-size: 12px; margin-top: 3px; overflow-wrap: anywhere; }
.compact { margin: 8px 10px; font-size: 12px; }
.compact li { padding: 4px 0; }
.compact .result { font-size: 11px; }
</style>
