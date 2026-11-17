<script setup lang="ts">
import { onMounted, ref, watch } from "vue";

import type { PlayerHit } from "../shared/bindings/PlayerHit";
import { commands, errorText } from "../shared/engine";
import { ago, rating } from "../shared/format";
import { useNow } from "../shared/useNow";
import { selectedPlayer } from "./navigation";
import PlayerDetailView from "./PlayerDetailView.vue";

const now = useNow(60_000);
const query = ref("");
const hits = ref<PlayerHit[]>([]);
const failure = ref<string | null>(null);
let timer: number | undefined;
let latest = 0;

async function search() {
  const ticket = ++latest;
  try {
    const result = await commands.searchPlayers(query.value, 80);
    // 输入很快时，只采用最后一次查询的结果
    if (ticket === latest) {
      hits.value = result;
      failure.value = null;
    }
  } catch (error) {
    if (ticket === latest) failure.value = errorText(error);
  }
}

watch(query, () => {
  window.clearTimeout(timer);
  timer = window.setTimeout(search, 250);
});
onMounted(search);

function title(hit: PlayerHit): string {
  return hit.name ?? hit.logName ?? `ID ${hit.id}`;
}
</script>

<template>
  <div class="layout">
    <section class="card list">
      <input v-model="query" type="text" class="search" placeholder="按名字或 ID 查找" />
      <p v-if="failure" class="error">{{ failure }}</p>
      <p class="hint">{{ query.trim() ? `找到 ${hits.length} 人` : "最近见过的玩家" }}</p>
      <ul>
        <li
          v-for="hit in hits"
          :key="hit.id"
          :class="{ active: hit.id === selectedPlayer }"
          @click="selectedPlayer = hit.id"
        >
          <div class="row-top">
            <span class="name">{{ title(hit) }}</span>
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
  grid-template-columns: 300px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.list {
  position: sticky;
  top: 0;
  max-height: calc(100vh - 40px);
  overflow: auto;
}

.search {
  width: 100%;
}

ul {
  list-style: none;
  margin: 0;
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
  font-size: 12px;
}

.elo {
  font-weight: 600;
}

.row-sub {
  display: flex;
  gap: 10px;
}
</style>
