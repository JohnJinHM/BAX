<script lang="ts">
import { reactive } from "vue";

// The request continues when switching profiles. Returning to it restores the stop action.
const active = reactive(new Set<number>());
const stopping = reactive(new Set<number>());
const messages = reactive(new Map<number, string>());
</script>
<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from "vue";
import type { PlayerFight } from "../shared/bindings/PlayerFight";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { mmss } from "../shared/format";
import FightRoster from "./FightRoster.vue";
import SpecIcon from "../shared/SpecIcon.vue";
import MatchTag from "../shared/MatchTag.vue";

const props = defineProps<{id: number}>();
const emit = defineEmits<{preview: [value: {id: number; map: string | null}]}>();
const snapshot = useSnapshot();
const limit = ref<number | string>(20);
const validLimit = computed(() => typeof limit.value === "number" && Number.isInteger(limit.value) && limit.value >= 1 && limit.value <= 100);
const fights = defineModel<PlayerFight[]>("fights", {required:true});
const lastQueriedAt = ref<number | null>(null);
const query = ref("");
const page = ref(0);
const pageSize = 8;
const failure = ref("");
const loading = ref(false);
let sequence = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let mounted = true;

function outcome(f: PlayerFight) {
  if (f.team === "spectators") return "spectator";
  return f.won === null ? "unknown" : f.won ? "win" : "loss";
}
const outcomeTerms = { spectator: "旁观 观战", unknown: "未判定", win: "胜利", loss: "负 失败" };
const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  return !q ? fights.value : fights.value.filter(f =>
    [f.map ?? "地图待识别", outcomeTerms[outcome(f)], f.hasLocalLog ? "本地" : "", f.playedTogether ? "同场" : "",
      ...f.roster.map(p => p.name), ...f.specs.map(s => s.name)].join(" ").toLowerCase().includes(q));
});
const pages = computed(() => Math.ceil(filtered.value.length / pageSize));
const visible = computed(() => filtered.value.slice(page.value * pageSize, (page.value + 1) * pageSize));
watch(query, () => { page.value = 0; });
watch(pages, count => { page.value = Math.min(page.value, Math.max(0, count - 1)); });
const time = new Intl.DateTimeFormat("zh-CN", {year:"numeric", month:"2-digit", day:"2-digit", hour:"2-digit", minute:"2-digit", hour12:false});

async function load() {
  const id = props.id;
  const seq = ++sequence;
  try {
    const archive = await commands.playerFights(id);
    if (mounted && props.id === id && sequence === seq) {
      fights.value = archive.fights;
      lastQueriedAt.value = archive.lastQueriedAt;
      failure.value = "";
    }
  } catch (e) {
    if (mounted && props.id === id && sequence === seq) failure.value = errorText(e);
  } finally {
    if (sequence === seq) loading.value = false;
  }
}
watch(() => props.id, () => {
  clearTimeout(timer);
  timer = undefined;
  fights.value = [];
  lastQueriedAt.value = null;
  query.value = "";
  page.value = 0;
  failure.value = "";
  loading.value = true;
  void load();
}, { immediate:true });
watch([() => snapshot.value?.archive.seq, () => snapshot.value?.selfId], () => {
  // Coalesce bursts of archive notifications while still showing new rows during a search.
  if (timer == null) timer = setTimeout(() => { timer = undefined; void load(); }, 350);
});
watch(() => active.has(props.id), (running, wasRunning) => {
  if (wasRunning && !running) void load();
});
onBeforeUnmount(() => { mounted = false; clearTimeout(timer); });

async function search() {
  const id = props.id;
  if (!validLimit.value || active.has(id)) return;
  active.add(id);
  lastQueriedAt.value = Date.now();
  messages.delete(id);
  try { messages.set(id, await commands.syncPlayerFights(id, Number(limit.value))); }
  catch (error) { messages.set(id, errorText(error)); }
  finally {
    active.delete(id);
    stopping.delete(id);
  }
}
async function stop() {
  const id = props.id;
  stopping.add(id);
  try { await commands.stopPlayerFights(id); }
  catch (error) { stopping.delete(id); messages.set(id, errorText(error)); }
}
</script>
<template>
  <section class="archive" aria-label="归档战报">
    <header>
      <div>
        <h3>归档战报 <small>{{ fights.length }} 场</small></h3>
        <small v-if="lastQueriedAt != null || (!loading && !failure)" class="last-query">上次查询：{{ lastQueriedAt == null ? "暂无记录" : time.format(lastQueriedAt) }}</small>
      </div>
      <form @submit.prevent="search">
        <label>最近 <input v-model.number="limit" type="number" min="1" max="100" step="1" required :disabled="active.has(id)" aria-label="查询场数（1 至 100）" /> 场</label>
        <button v-if="active.has(id)" type="button" :disabled="stopping.has(id)" @click="stop">{{ stopping.has(id) ? '正在停止…' : '停止查询' }}</button>
        <button v-else class="primary" :disabled="!validLimit">刷新过往对局</button>
      </form>
    </header>
    <p v-if="active.has(id) || messages.has(id)" class="hint" role="status">{{ messages.get(id) ?? '正在查询 STB 并收录战报…' }}</p>
    <p v-if="failure" class="error">{{ failure }}</p>
    <input v-if="fights.length" v-model="query" class="filter" type="search" placeholder="筛选地图、标签或专精" aria-label="筛选已归档战报" />
    <ul v-if="visible.length" class="reports">
      <li v-for="f in visible" :key="f.matchId">
        <button class="fight" type="button" @click="emit('preview', {id:f.matchId, map:f.map})">
          <MatchTag :kind="outcome(f)" />
          <span class="map">
            <span class="map-title">
              <strong>{{ f.map ?? "地图待识别" }}</strong>
              <MatchTag v-if="f.hasLocalLog" kind="local" />
              <MatchTag v-if="f.playedTogether" kind="together" />
            </span>
            <small>{{ f.start == null ? "开局时间未知" : time.format(f.start) }}</small>
          </span>
          <span class="specs"><span v-for="s in f.specs" :key="s.ids[0]"><SpecIcon :id="s.ids[0]!" :label="s.name" />{{ s.name }}</span><small v-if="!f.specs.length">专精未识别</small></span>
          <span class="numbers"><span>{{ f.durationSecs == null ? "时长未知" : mmss(f.durationSecs * 1000) }}</span><small v-if="f.ratingDelta != null" :class="{gain:f.ratingDelta > 0, loss:f.ratingDelta < 0}">ELO {{ f.ratingDelta > 0 ? '+' : '' }}{{ Number(f.ratingDelta.toFixed(1)) }}</small></span>
          <FightRoster :fight="f" :player-id="id" />
          <span class="chevron" aria-hidden="true">›</span>
        </button>
      </li>
    </ul>
    <p v-else-if="!failure" class="hint">{{ loading ? "正在读取归档…" : fights.length ? "没有符合条件的战报。" : "暂无归档，查询后可在这里查看战报。" }}</p>
    <nav v-if="pages > 1" aria-label="归档战报分页">
      <button :disabled="!page" @click="page--">上一页</button><small>{{ page+1 }} / {{ pages }}</small><button :disabled="page+1 >= pages" @click="page++">下一页</button>
    </nav>
  </section>
</template>
<style scoped>
.archive { margin:18px 0; border-block:1px solid var(--bax-border); padding:14px 0; container-type:inline-size; }
header, form, form label, nav { display:flex; align-items:center; gap:8px; }
header { flex-wrap:wrap; justify-content:space-between; }
h3 { margin:0; color:var(--bax-fg); font-size:calc(16px * var(--bax-font-scale)); } h3 small { margin-left:5px; font-weight:400; }
small, form label, .chevron { color:var(--bax-muted); }
form input { width:4.5em; padding:5px 6px; text-align:center; font:inherit; font-variant-numeric:tabular-nums; color:var(--bax-fg); background:var(--bax-surface-2); border:1px solid var(--bax-border); border-radius:var(--bax-radius-xs); color-scheme:dark; }
form input:focus-visible { outline:2px solid var(--bax-accent); outline-offset:1px; }
form input:invalid { border-color:var(--bax-warn); }
.filter { width:100%; margin:12px 0 4px; }
.reports { list-style:none; padding:0; margin:0; }
.fight { width:100%; display:grid; grid-template-columns:auto minmax(100px,1.3fr) minmax(100px,1fr) auto auto; align-items:center; gap:12px; padding:11px 4px; text-align:left; color:var(--bax-fg); background:transparent; border:none; border-bottom:1px solid var(--bax-divider); border-radius:0; }
.fight:hover, .fight:focus-visible { background:var(--bax-surface-2); }
.last-query { display:block; margin-top:3px; font-variant-numeric:tabular-nums; }
.map,.numbers,.specs { display:grid; gap:3px; min-width:0; }
.map-title { display:flex; flex-wrap:wrap; align-items:center; gap:3px 6px; }
.map strong { overflow-wrap:anywhere; }.map small,.numbers,.archive nav { font-variant-numeric:tabular-nums; }
.specs > span { display:flex; align-items:center; gap:5px; }.specs :deep(.spec-icon) { width:18px; height:18px; }
.numbers { text-align:right; }.chevron { font-size:1.4em; } nav { justify-content:flex-end; margin-top:10px; }
select { color-scheme:dark; color:var(--bax-fg); background:var(--bax-surface-2); border:1px solid var(--bax-border); border-radius:var(--bax-radius-xs); padding:4px 6px; font:inherit; }
.gain {color:var(--bax-good);} .loss {color:var(--bax-bad);}
.chevron {grid-row:1;grid-column:5;}
@container (max-width: 450px) {
  .fight { grid-template-columns:auto minmax(0,1fr) auto; gap:4px 8px; }
  .specs { grid-column:2; display:flex; flex-wrap:wrap; }.numbers { grid-column:3; grid-row:1/3; }.chevron { display:none; }
}
</style>
