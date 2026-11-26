<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { HistoryRow } from "../shared/bindings/HistoryRow";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { mmss, phaseNames } from "../shared/format";
import { historyPlayer, selectedMatch } from "./navigation";

// 战后分析带着图表库，打开这一页时才加载
const ReportView = defineAsyncComponent(() => import("./ReportView.vue"));

const PAGE = 100;
const snapshot = useSnapshot();
const query = ref("");
let timer: number | undefined;
const rows = ref<HistoryRow[]>([]);
const failure = ref<string | null>(null);
const exhausted = ref(false);
const loading = ref(false);
let requestedRows = PAGE;
let sequence = 0;

async function load(more = false) {
  if (more && (loading.value || exhausted.value)) return;
  if (more) requestedRows = rows.value.length + PAGE;
  const request = ++sequence;
  loading.value = true;
  try {
    const next = more ? [...rows.value] : [];
    let atEnd = false;
    while (next.length < requestedRows) {
      const limit = Math.min(500, requestedRows - next.length);
      const page = await commands.matchHistory(limit, next.length, query.value.trim(), historyPlayer.value?.id);
      if (request !== sequence) return;
      next.push(...page);
      atEnd = page.length < limit;
      if (atEnd) break;
    }
    rows.value = next;
    exhausted.value = atEnd;
    failure.value = null;
    if (selectedMatch.value == null || ((query.value.trim() || historyPlayer.value) && !next.some(r => r.matchId === selectedMatch.value))) {
      selectedMatch.value = rows.value.find((r) => r.report.state === "ready")?.matchId ?? rows.value[0]?.matchId ?? null;
    }
  } catch (error) {
    if (request === sequence) failure.value = errorText(error);
  } finally {
    if (request === sequence) loading.value = false;
  }
}

onMounted(() => void load());
watch([query, historyPlayer], () => {
  ++sequence;
  requestedRows = PAGE;
  rows.value = [];
  selectedMatch.value = null;
  failure.value = null;
  loading.value = true;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void load(), 200);
});
onBeforeUnmount(() => { ++sequence; window.clearTimeout(timer); });
// 有新比赛、新战报或者队列变了就刷新列表
watch(
  [() => snapshot.value?.archive.seq, () => snapshot.value?.current?.matchId, () => snapshot.value?.current?.phase],
  () => void load(),
);

const selectedRow = computed(() => rows.value.find((r) => r.matchId === selectedMatch.value) ?? null);
const pending = computed(() => snapshot.value?.archive.pending ?? 0);

function date(at: number): string {
  return new Date(at).toLocaleString("zh-CN", { hour12: false, month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

// 列表行右侧的状态只放四个字以内，保持每行等高；需要更完整的说明时放进 title
function reportText(row: HistoryRow): string {
  switch (row.report.state) {
    case "cancelled":
      return "取消";
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

function reportTitle(row: HistoryRow): string | undefined {
  return row.report.state === "cancelled" ? "准备阶段取消，不获取战报" : undefined;
}
</script>

<template>
  <div class="layout">
    <section class="card list">
      <label class="history-search">查询过往对局
        <input v-model="query" type="search" placeholder="地图、玩家名、玩家或对局 ID" />
      </label>
      <button v-if="historyPlayer" class="scope" :aria-label="`取消玩家筛选：${historyPlayer.name}`" @click="historyPlayer = null">{{ historyPlayer.name }} ×</button>
      <div class="row">
        <span v-if="pending" class="hint">{{ pending }} 场在等战报</span>
      </div>
      <p v-if="failure" class="error">{{ failure }}</p>
      <ul aria-label="比赛记录">
        <li v-for="row in rows" :key="row.matchId">
          <button type="button" class="match-entry" :aria-current="row.matchId === selectedMatch ? 'true' : undefined"
            @click="selectedMatch = row.matchId">
            <span class="row-top">
              <span v-if="row.selfTeam === 'spectators'" class="result spectator" title="旁观比赛" aria-label="旁观比赛">旁</span>
              <span v-else-if="row.report.state === 'cancelled'" class="result cancelled">弃</span>
              <span v-else-if="row.won != null" class="result" :class="row.won ? 'win' : 'loss'">{{ row.won ? "胜" : "负" }}</span>
              <span class="map">{{ row.map ?? "地图未知" }}</span>
              <span class="when num">{{ date(row.at) }}</span>
            </span>
            <span class="row-sub hint">
              <span class="num">{{ row.matchId }}</span>
              <span v-if="row.durationMs != null" class="num">{{ mmss(row.durationMs) }}</span>
              <span>{{ row.players }} 人</span>
              <span class="state" :class="row.report.state" :title="reportTitle(row)">{{ reportText(row) }}</span>
            </span>
          </button>
        </li>
      </ul>
      <button v-if="rows.length && !exhausted" class="more" :disabled="loading" @click="load(true)">更多</button>
      <p v-if="loading" class="hint" role="status">正在查询…</p>
      <p v-else-if="!rows.length && !failure && (query.trim() || historyPlayer)" class="hint" role="status">没有找到符合条件的对局。</p>
      <p v-else-if="!rows.length && !failure" class="hint">还没有比赛记录。打完一场比赛，或者在设置里导入历史日志之后，这里会列出来。</p>
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
  grid-template-columns: calc(280px * var(--bax-font-scale)) minmax(0, 1fr);
  gap: calc(16px * var(--bax-density));
  align-items: start;
}

.list { position: sticky; top: 0; max-height: calc(100vh - 40px * var(--bax-density)); overflow-y: auto; }

.history-search { display: grid; gap: 6px; }
.history-search input { width: 100%; }
.scope { margin-top: 8px; }

.list h2 {
  margin: 0;
}

ul {
  list-style: none;
  margin: calc(8px * var(--bax-density)) 0 0;
  padding: 0;
}

.match-entry {
  display: block;
  width: 100%;
  text-align: left;
  color: inherit;
  font: inherit;
  background: transparent;
  border: 0;
  padding: calc(6px * var(--bax-density)) 8px;
  border-radius: var(--bax-radius-sm);
  cursor: pointer;
}

.match-entry:hover {
  background: var(--bax-surface-2);
}

.match-entry[aria-current="true"] {
  background: var(--bax-selected-bg);
  box-shadow: inset 2px 0 0 var(--bax-selected-edge);
}

.match-entry:focus-visible {
  outline: 2px solid var(--bax-accent);
  outline-offset: -2px;
}

/* 每行固定一行高：地图名过长时只省略地图名，日期、标签与状态不换行 */
.row-top {
  display: flex;
  align-items: center;
  gap: 6px;
}

.map {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.when {
  flex-shrink: 0;
  margin-left: auto;
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-muted);
  white-space: nowrap;
}

.row-sub {
  display: flex;
  gap: 8px;
  white-space: nowrap;
}

.row-sub > span {
  flex-shrink: 0;
}

.result {
  flex-shrink: 0;
  padding: 0 5px;
  border-radius: min(3px, var(--bax-radius-xs));
  font-size: calc(12px * var(--bax-font-scale));
  font-weight: 600;
}

.result.win {
  background: color-mix(in srgb, var(--bax-good) 15%, transparent);
  color: var(--bax-good);
}

.result.loss {
  background: color-mix(in srgb, var(--bax-bad) 15%, transparent);
  color: var(--bax-bad);
}

.result.cancelled,
.result.spectator {
  background: color-mix(in srgb, var(--bax-muted) 15%, transparent);
  color: var(--bax-muted);
  white-space: nowrap;
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
  .layout { grid-template-columns: calc(240px * var(--bax-font-scale)) minmax(380px, 1fr); }
  .list { padding: calc(10px * var(--bax-density)); }
}
</style>
