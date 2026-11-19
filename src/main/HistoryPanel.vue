<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch } from "vue";

import type { HistoryRow } from "../shared/bindings/HistoryRow";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { mmss, phaseNames } from "../shared/format";
import { selectedMatch } from "./navigation";

// 战后分析带着图表库，打开这一页时才加载
const ReportView = defineAsyncComponent(() => import("./ReportView.vue"));

const PAGE = 100;
const snapshot = useSnapshot();
const rows = ref<HistoryRow[]>([]);
const failure = ref<string | null>(null);
const exhausted = ref(false);

async function load(more = false) {
  try {
    const offset = more ? rows.value.length : 0;
    const page = await commands.matchHistory(more ? PAGE : Math.max(PAGE, rows.value.length), offset);
    rows.value = more ? [...rows.value, ...page] : page;
    exhausted.value = page.length < PAGE;
    failure.value = null;
    if (selectedMatch.value == null) {
      selectedMatch.value = rows.value.find((r) => r.report.state === "ready")?.matchId ?? rows.value[0]?.matchId ?? null;
    }
  } catch (error) {
    failure.value = errorText(error);
  }
}

onMounted(() => void load());
// 有新比赛、新战报或者队列变了就刷新列表
watch(
  () => [snapshot.value?.archive.seq, snapshot.value?.current?.matchId, snapshot.value?.current?.phase],
  () => void load(),
);

const selectedRow = computed(() => rows.value.find((r) => r.matchId === selectedMatch.value) ?? null);
const pending = computed(() => snapshot.value?.archive.pending ?? 0);

function date(at: number): string {
  return new Date(at).toLocaleString("zh-CN", { hour12: false, month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function reportText(row: HistoryRow): string {
  switch (row.report.state) {
    case "ready":
      return "有战报";
    case "pending":
      return "等战报";
    case "gaveUp":
      return "没有战报";
    case "none":
      return row.phase === "ended" ? "" : phaseNames[row.phase];
  }
}
</script>

<template>
  <div class="layout">
    <section class="card list">
      <div class="row">
        <h2>比赛记录</h2>
        <span v-if="pending" class="hint">{{ pending }} 场在等战报</span>
      </div>
      <p v-if="failure" class="error">{{ failure }}</p>
      <ul>
        <li
          v-for="row in rows"
          :key="row.matchId"
          :class="{ active: row.matchId === selectedMatch }"
          @click="selectedMatch = row.matchId"
        >
          <div class="row-top">
            <span v-if="row.won != null" class="result" :class="row.won ? 'win' : 'loss'">{{ row.won ? "胜" : "负" }}</span>
            <span class="map">{{ row.map ?? "地图未知" }}</span>
            <span class="when num">{{ date(row.at) }}</span>
          </div>
          <div class="row-sub hint">
            <span class="num">{{ row.matchId }}</span>
            <span v-if="row.durationMs != null" class="num">{{ mmss(row.durationMs) }}</span>
            <span>{{ row.players }} 人</span>
            <span class="state" :class="row.report.state">{{ reportText(row) }}</span>
          </div>
        </li>
      </ul>
      <button v-if="rows.length && !exhausted" class="more" @click="load(true)">更多</button>
      <p v-if="!rows.length && !failure" class="hint">还没有比赛记录。打完一场比赛，或者在设置里导入历史日志之后，这里会列出来。</p>
    </section>
    <ReportView v-if="selectedMatch != null" :id="selectedMatch" :row="selectedRow" @requested="load()" />
    <section v-else class="card empty">
      <p class="hint">从左边选一场比赛。</p>
    </section>
  </div>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.list h2 {
  margin: 0;
}

ul {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
}

li {
  padding: 6px 8px;
  border-radius: 6px;
  cursor: pointer;
}

li:hover {
  background: var(--bax-surface-2);
}

li.active {
  background: rgb(76 194 255 / 0.12);
}

.row-top {
  display: flex;
  align-items: center;
  gap: 6px;
}

.map {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.when {
  margin-left: auto;
  font-size: 12px;
  color: var(--bax-muted);
}

.row-sub {
  display: flex;
  gap: 8px;
}

.result {
  padding: 0 5px;
  border-radius: 3px;
  font-size: 12px;
  font-weight: 600;
}

.result.win {
  background: rgb(95 211 141 / 0.15);
  color: var(--bax-good);
}

.result.loss {
  background: rgb(240 97 109 / 0.15);
  color: var(--bax-bad);
}

.state {
  margin-left: auto;
}

.state.ready {
  color: var(--bax-fg);
}

.state.pending {
  color: var(--bax-warn);
}

.more {
  width: 100%;
  margin-top: 8px;
}

.empty {
  min-height: 80px;
}

@media (max-width: 1180px) {
  .layout { grid-template-columns: minmax(0, 1fr); }
  .list ul { display: flex; overflow-x: auto; gap: 8px; max-height: 95px; }
  .list li { flex: 0 0 245px; }
}
</style>
