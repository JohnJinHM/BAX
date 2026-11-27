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
import { compositionGroups, mmss } from "../shared/format";
import SpecIcon from "../shared/SpecIcon.vue";
import MatchTag from "../shared/MatchTag.vue";

const props = defineProps<{id: number}>();
const emit = defineEmits<{preview: [value: {id: number; map: string | null}]}>();
const snapshot = useSnapshot();
const limit = ref<number | string>(20);
const validLimit = computed(() => typeof limit.value === "number" && Number.isInteger(limit.value) && limit.value >= 1 && limit.value <= 100);
const fights = ref<PlayerFight[]>([]);
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
      ...f.specs.map(s => s.name)].join(" ").toLowerCase().includes(q));
});
const metric = ref<"value" | "count">("value");
function shares(f: PlayerFight) {
  const values = compositionGroups.map(group => ({
    ...group, amount: f.composition.reduce((sum, c) => sum + (group.classes.includes(c.class) ? c[metric.value] : 0), 0),
  })).filter(g => g.amount > 0);
  const total = values.reduce((sum, g) => sum + g.amount, 0);
  return values.map(g => ({ ...g, percent: 100 * g.amount / total }));
}
const opening = computed(() => {
  const recent = filtered.value.filter(f => f.team !== "spectators").slice(0, 20);
  const samples = recent.filter(f => f.opening != null);
  const counts = new Map<string, number>();
  for (const f of samples) for (const unit of f.opening ?? []) counts.set(unit, (counts.get(unit) ?? 0) + 1);
  return { n: samples.length, total: recent.length, top: [...counts].sort((a,b) => b[1]-a[1] || a[0].localeCompare(b[0])).slice(0,5) };
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
    <section v-if="opening.total" class="openings">
      <h4>开局偏好 <small>前 3 分钟 · 最近 {{ opening.total }} 场中 {{ opening.n }} 场有时间记录</small></h4>
      <ul v-if="opening.top.length"><li v-for="[unit, games] in opening.top" :key="unit">{{ unit }} <strong>{{ games }}/{{ opening.n }} 场</strong></li></ul>
      <small v-else>暂无可统计的开局单位</small>
    </section>
    <label v-if="visible.length">逐局兵种构成 <select v-model="metric"><option value="value">估算基础价值占比</option><option value="count">出兵次数占比</option></select></label>
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
          <span class="numbers"><span>{{ f.durationSecs == null ? "时长未知" : mmss(f.durationSecs * 1000) }}</span><small v-if="f.ratingDelta != null">ELO {{ f.ratingDelta > 0 ? '+' : '' }}{{ Number(f.ratingDelta.toFixed(1)) }}</small></span>
          <span class="mix" v-if="f.team !== 'spectators'">
            <span class="stack" aria-hidden="true"><i v-for="g in shares(f)" :key="g.key" :style="{width:g.percent+'%', background:g.color}" /></span>
            <small>{{ shares(f).map(g => g.name + ' ' + Math.round(g.percent) + '%').join(' · ') || '暂无构成数据' }}</small>
          </span>
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
.openings h4 { margin:12px 0 6px; }.openings small {font-weight:400;}
.openings ul {list-style:none;display:flex;flex-wrap:wrap;gap:5px 16px;padding:0;margin:0 0 14px;}
.openings strong {color:var(--bax-muted);font-weight:normal;}
.mix {grid-column:1/-1;grid-row:2;display:grid;gap:4px;}.stack {display:flex;overflow:hidden;border-radius:3px;height:6px;}
.chevron {grid-row:1;grid-column:5;}
@container (max-width: 450px) {
  .fight { grid-template-columns:auto minmax(0,1fr) auto; gap:4px 8px; }
  .mix {grid-row:3;}.specs { grid-column:2; display:flex; flex-wrap:wrap; }.numbers { grid-column:3; grid-row:1/3; }.chevron { display:none; }
}
</style>
