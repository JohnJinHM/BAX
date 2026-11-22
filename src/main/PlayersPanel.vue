<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";

import type { PlayerHit } from "../shared/bindings/PlayerHit";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { ago, rating } from "../shared/format";
import { useNow } from "../shared/useNow";
import { selectedPlayer } from "./navigation";
import PlayerDetailView from "./PlayerDetailView.vue";

const now = useNow(60_000);
const query = ref("");
const snapshot = useSnapshot();
const sort = ref("default");
const filter = ref("all");
const busy = ref(false);
const more = ref(false);
const PAGE = 80;
const hits = ref<PlayerHit[]>([]);
const failure = ref<string | null>(null);
let timer: number | undefined;
let latest = 0;

async function search(append = false) {
  const ticket = ++latest;
  busy.value = true;
  try {
    const result = await commands.searchPlayers(query.value, PAGE, sort.value, filter.value, append ? hits.value.length : 0);
    // 输入很快时，只采用最后一次查询的结果
    if (ticket === latest) {
      hits.value = append ? [...hits.value, ...result] : result;
      more.value = result.length === PAGE;
      failure.value = null;
    }
  } catch (error) {
    if (ticket === latest) failure.value = errorText(error);
  } finally {
    if (ticket === latest) busy.value = false;
  }
}

watch([query, sort, filter], () => {
  ++latest;
  busy.value = true;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void search(), 250);
});
watch(() => snapshot.value?.selfId, () => void search());
onMounted(() => void search());
onBeforeUnmount(() => { ++latest; window.clearTimeout(timer); });

function title(hit: PlayerHit): string {
  return hit.name ?? hit.logName ?? `ID ${hit.id}`;
}
</script>

<template>
  <div class="layout">
    <section class="card list">
      <input v-model="query" type="text" class="search" aria-label="按名字或 ID 查找" placeholder="按名字或 ID 查找" />
      <div class="list-controls">
        <label>筛选<select v-model="filter"><option value="all">全部玩家</option><option value="noted">有备注</option><option value="encountered">有同场记录</option><option value="rated">有 ELO</option></select></label>
        <label>排序<select v-model="sort"><option value="default">本机与同场最多</option><option value="recent">最近见过</option><option value="ratingDesc">ELO 从高到低</option><option value="ratingAsc">ELO 从低到高</option><option value="name">名字</option></select></label>
      </div>
      <p v-if="failure" class="error">{{ failure }}</p>
      <p class="hint" role="status">{{ busy ? '正在查询…' : `已显示 ${hits.length} 人` }}</p>
      <ul>
        <li
          v-for="hit in hits"
          :key="hit.id"
          :class="{ active: hit.id === selectedPlayer }"
          tabindex="0"
          @keydown.enter="selectedPlayer = hit.id"
          @keydown.space.prevent="selectedPlayer = hit.id"
          @click="selectedPlayer = hit.id"
        >
          <div class="row-top">
            <span class="name">{{ title(hit) }}</span>
            <span v-if="hit.id === snapshot?.selfId" class="self-mark">本机</span>
            <span v-if="hit.hasNote" class="note-mark" title="有备注">✎</span>
            <span class="elo num">{{ rating(hit.rating) }}</span>
          </div>
          <div class="row-sub hint">
            <span>ID {{ hit.id }}</span>
            <span v-if="hit.encounters > 0">同场 {{ hit.encounters }} 次</span>
            <span>{{ ago(hit.lastSeenAt, now) }}</span>
          </div>
        </li>
      </ul>
      <button v-if="more" class="more" :disabled="busy" @click="search(true)">加载更多</button>
      <p v-if="!hits.length && !failure" class="hint">没有找到。导入历史日志（bax logs import）后可以找到更多玩家。</p>
    </section>
    <PlayerDetailView v-if="selectedPlayer != null" :id="selectedPlayer" />
    <section v-else class="card empty">
      <p class="hint">从左边选一名玩家，或者在“当前对局”里点玩家的名字。</p>
    </section>
  </div>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: calc(300px * var(--bax-font-scale)) minmax(0, 1fr);
  gap: calc(16px * var(--bax-density));
  align-items: start;
}

.list {
  position: sticky;
  top: 0;
  max-height: calc(100vh - 40px * var(--bax-density));
  overflow: auto;
}

.search {
  width: 100%;
}
.list-controls { display: grid; grid-template-columns: 1fr 1.3fr; gap: 8px; margin-top: 10px; }
.list-controls label { display: grid; gap: 4px; color: var(--bax-muted); font-size: calc(12px * var(--bax-font-scale)); min-width: 0; }
.list-controls select { width: 100%; padding: 6px 3px; border: 1px solid var(--bax-border); border-radius: var(--bax-radius-xs); background: var(--bax-surface-2); color: var(--bax-fg); }
.self-mark { color: var(--bax-accent); font-size: calc(11px * var(--bax-font-scale)); }
.more { width: 100%; }
li:focus-visible, select:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 2px; }

ul {
  list-style: none;
  margin: 0;
  padding: 0;
}

li {
  padding: calc(6px * var(--bax-density)) 8px;
  border-radius: var(--bax-radius-sm);
  cursor: pointer;
}

li:hover {
  background: var(--bax-surface-2);
}

li.active {
  background: var(--bax-selected-bg);
  box-shadow: inset 2px 0 0 var(--bax-selected-edge);
}

.row-top {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.note-mark {
  color: var(--bax-warn);
  font-size: calc(12px * var(--bax-font-scale));
}

.elo {
  font-weight: 600;
}

.row-sub {
  display: flex;
  gap: 10px;
}
</style>
