<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, reactive, ref, watch } from "vue";

import type { PlayerHit } from "../shared/bindings/PlayerHit";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { currentRelations } from "../shared/roster";
import { ago, rating } from "../shared/format";
import { useNow } from "../shared/useNow";
import { selectedPlayer } from "./navigation";
import PlayerDetailView from "./PlayerDetailView.vue";

const now = useNow(60_000);
const query = ref("");
const snapshot = useSnapshot();
const relations = computed(() => currentRelations(snapshot.value?.current));
const sort = ref("recent");
const savingFavorites = reactive(new Set<number>());
const filter = ref("all");
const busy = ref(false);
const more = ref(false);
const PAGE = 80;
const hits = ref<PlayerHit[]>([]);
const failure = ref<string | null>(null);
let timer: number | undefined;
let latest = 0;

async function search(append = false, count = PAGE) {
  const ticket = ++latest;
  busy.value = true;
  try {
    const result: PlayerHit[] = [];
    const offset = append ? hits.value.length : 0;
    // 收藏可能移动跨页记录，从头重读当前已显示的范围。
    while (result.length < count) {
      const limit = Math.min(500, count - result.length);
      const page = await commands.searchPlayers(query.value, limit, sort.value, filter.value, offset + result.length);
      if (ticket !== latest) return;
      result.push(...page);
      if (page.length < limit) break;
    }
    // 输入很快时，只采用最后一次查询的结果
    if (ticket === latest) {
      hits.value = append ? [...hits.value, ...result] : result;
      more.value = result.length === count;
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
watch([() => snapshot.value?.selfId, () => snapshot.value?.current?.matchId,
  () => Array.from(relations.value.entries()).join("|")], () => void search());
onMounted(() => void search());
onBeforeUnmount(() => { ++latest; window.clearTimeout(timer); });

async function toggleFavorite(hit: PlayerHit) {
  if (savingFavorites.has(hit.id)) return;
  savingFavorites.add(hit.id);
  try {
    await commands.setPlayerFavorite(hit.id, !hit.isFavorite);
    await search(false, Math.max(PAGE, Math.ceil(hits.value.length / PAGE) * PAGE));
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    savingFavorites.delete(hit.id);
  }
}

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
        <label>排序<select v-model="sort"><option value="recent">最近见过</option><option value="encounters">同场最多</option><option value="ratingDesc">ELO 从高到低</option><option value="ratingAsc">ELO 从低到高</option><option value="name">名字</option></select></label>
      </div>
      <p v-if="failure" class="error">{{ failure }}</p>
      <p class="hint" role="status">{{ busy ? '正在查询…' : `已显示 ${hits.length} 人` }}</p>
      <ul>
        <li
          v-for="hit in hits"
          :key="hit.id"
          :class="{ active: hit.id === selectedPlayer, teammate: relations.get(hit.id) === '队友', opponent: relations.get(hit.id) === '对手' }"
          tabindex="0"
          @keydown.enter.self="selectedPlayer = hit.id"
          @keydown.space.self.prevent="selectedPlayer = hit.id"
          @click="selectedPlayer = hit.id"
        >
          <div class="row-top">
            <span class="name" :title="title(hit)">{{ title(hit) }}</span>
            <button type="button" class="favorite" :aria-pressed="hit.isFavorite" :disabled="savingFavorites.has(hit.id)"
              :aria-label="`收藏 ${title(hit)}`" :title="hit.isFavorite ? '取消收藏' : '收藏玩家'" @click.stop="toggleFavorite(hit)">
              <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                <path d="m12 3 2.8 5.7 6.3.9-4.5 4.4 1.1 6.2-5.7-3-5.7 3 1.1-6.2-4.5-4.4 6.3-.9Z" />
              </svg>
            </button>
            <span v-if="hit.id === snapshot?.selfId" class="self-mark">本机</span>
            <span v-if="relations.has(hit.id)" class="relation">本局{{ relations.get(hit.id) }}</span>
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

li.teammate { --team-color: var(--bax-good); }
li.opponent { --team-color: var(--bax-bad); }
li.teammate, li.opponent { background: color-mix(in srgb, var(--team-color) 7%, transparent); }
.relation { color: var(--team-color); font-size: calc(12px * var(--bax-font-scale)); white-space: nowrap; }

li:hover {
  background: var(--bax-surface-2);
}

li.active {
  background: var(--bax-selected-bg);
  box-shadow: inset 2px 0 0 var(--bax-selected-edge);
}

.row-top {
  display: flex;
  align-items: center;
  gap: 4px;
}

.favorite {
  display: grid;
  place-items: center;
  flex: none;
  width: 24px;
  height: 24px;
  padding: 0;
  background: transparent;
  border: 0;
  color: var(--bax-muted);
}
.favorite path { fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linejoin: round; }
.favorite:hover, .favorite[aria-pressed="true"] { color: var(--bax-accent); }
.favorite[aria-pressed="true"] path { fill: currentColor; }
.favorite:focus-visible { outline: 2px solid var(--bax-accent); }
.name {
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
  margin-left: auto;
  font-weight: 600;
}

.row-sub {
  display: flex;
  gap: 10px;
}

@media (max-width: 900px) {
  .layout { grid-template-columns: minmax(0, 1fr); }
  .list { position: static; max-height: min(210px, 32vh); overflow: auto; }
}
</style>
