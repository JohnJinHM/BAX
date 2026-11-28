<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { PlayerSearchHit } from "../shared/bindings/PlayerSearchHit";
import type { PlayerSearchResponse } from "../shared/bindings/PlayerSearchResponse";
import { commands, errorText } from "../shared/engine";

const props = defineProps<{ initialQuery: string; selected: number | null }>();
const emit = defineEmits<{ select: [hit: PlayerSearchHit] }>();
const field = ref<HTMLInputElement | null>(null);
const query = ref(props.initialQuery);
const term = computed(() => query.value.trim());
const result = ref<PlayerSearchResponse | null>(null);
const failure = ref("");
const busy = ref(false);
let sequence = 0;

watch(term, () => {
  ++sequence;
  result.value = null;
  failure.value = "";
  busy.value = false;
}, { flush: "sync" });
onMounted(() => field.value?.focus());
onBeforeUnmount(() => { ++sequence; });

async function search() {
  if (!term.value || busy.value) return;
  const ticket = ++sequence;
  busy.value = true;
  result.value = null;
  failure.value = "";
  try {
    const response = await commands.searchBatracePlayers(term.value);
    if (ticket === sequence) result.value = response;
  } catch (error) {
    if (ticket === sequence) failure.value = errorText(error);
  } finally {
    if (ticket === sequence) busy.value = false;
  }
}
</script>

<template>
  <form class="search-form" role="search" aria-label="BATrace 玩家搜索" @submit.prevent="search">
    <input ref="field" v-model="query" type="search" aria-label="昵称、玩家 ID 或 Steam64" placeholder="昵称、玩家 ID 或 Steam64" />
    <button :disabled="!term || busy">搜索</button>
  </form>
  <p v-if="failure" class="error" role="alert">BATrace 查询失败：{{ failure }}</p>
  <p v-else-if="busy || result" class="hint" role="status">
    <template v-if="busy">正在搜索 BATrace…</template>
    <template v-else-if="result?.players.length">已显示 {{ result.players.length }} 人{{ result.count > result.players.length ? `，共 ${result.count} 人` : '' }}</template>
    <template v-else>BATrace 没有找到匹配的玩家。</template>
  </p>
  <ul v-if="result?.players.length" aria-label="BATrace 搜索结果">
    <li v-for="hit in result.players" :key="hit.id">
      <button class="result" :aria-pressed="selected === hit.id" @click="emit('select', hit)">
        <span class="identity"><strong>{{ hit.name || `ID ${hit.id}` }}</strong><small v-if="hit.level != null" class="hint">等级 {{ hit.level }}</small></span>
        <small class="hint">ID {{ hit.id }}</small>
        <small v-if="hit.steam_id" class="hint">Steam64 {{ hit.steam_id }}</small>
      </button>
    </li>
  </ul>
</template>

<style scoped>
.search-form { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; }
.search-form input { min-width: 0; }
ul { list-style: none; margin: 0; padding: 0; }
.result { width: 100%; display: grid; gap: 2px; padding: 7px 8px; background: transparent; border: 0; text-align: left; overflow-wrap: anywhere; }
.result:hover { background: var(--bax-surface-2); }
.result[aria-pressed="true"] { background: var(--bax-selected-bg); }
.identity { display: flex; align-items: baseline; justify-content: space-between; gap: 6px; }
.identity strong { min-width: 0; }
.identity small { flex: none; }
</style>
