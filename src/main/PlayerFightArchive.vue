<script lang="ts">
import { reactive } from "vue";

// The request continues when switching profiles. Returning to it restores the stop action.
const active = reactive(new Set<number>());
const stopping = reactive(new Set<number>());
const messages = reactive(new Map<number, string>());
</script>
<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from "vue";
import type { EncounterRow } from "../shared/bindings/EncounterRow";
import type { PlayerFight } from "../shared/bindings/PlayerFight";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { mmss, phaseNames } from "../shared/format";
import FightRoster from "./FightRoster.vue";
import SpecIcon from "../shared/SpecIcon.vue";
import MatchTag from "../shared/MatchTag.vue";

const props = defineProps<{id: number; encounters: EncounterRow[]}>();
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

function outcome(report?: PlayerFight, encounter?: EncounterRow): "spectator" | "unknown" | "win" | "loss" {
  if ((report?.team ?? encounter?.team) === "spectators") return "spectator";
  if (report?.won == null) return "unknown";
  return report.won ? "win" : "loss";
}
const scopes = [
  ["all", "全部"], ["together", "同场"], ["teammate", "队友"], ["opponent", "对手"], ["local", "本地"],
] as const;
const scope = ref<typeof scopes[number][0]>("all");
const resultFilter = ref("all");
const records = computed(() => {
  const reports = new Map(fights.value.map(f => [f.matchId, f]));
  const encounters = new Map(props.encounters.map(e => [e.matchId, e]));
  return [...new Set([...reports.keys(), ...encounters.keys()])].map(matchId => {
    const report = reports.get(matchId);
    const encounter = encounters.get(matchId);
    return {
      matchId, report, encounter,
      at: report?.start ?? encounter?.at ?? null,
      map: report?.map ?? encounter?.map ?? null,
      together: !!encounter || !!report?.playedTogether,
      relation: report?.relation && report.relation !== "other" ? report.relation : encounter?.relation,
      local: !!encounter || !!report?.hasLocalLog,
      result: outcome(report, encounter),
      seconds: report?.durationSecs ?? (encounter?.durationMs == null ? null : encounter.durationMs / 1000),
    };
  }).sort((a,b) => (b.at ?? 0) - (a.at ?? 0) || b.matchId - a.matchId);
});
const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  return records.value.filter(r => {
    if (scope.value === "together" && !r.together || scope.value === "local" && !r.local) return false;
    if ((scope.value === "teammate" || scope.value === "opponent") && r.relation !== scope.value) return false;
    if (resultFilter.value !== "all" && r.result !== resultFilter.value) return false;
    return !q || [r.matchId, r.map, r.encounter?.map, r.encounter?.logName, ...(r.report?.roster.map(p => p.name) ?? []), ...(r.report?.specs.map(s => s.name) ?? [])]
      .join(" ").toLowerCase().includes(q);
  });
});
const pages = computed(() => Math.ceil(filtered.value.length / pageSize));
const visible = computed(() => filtered.value.slice(page.value * pageSize, (page.value + 1) * pageSize));
watch([query, scope, resultFilter], () => { page.value = 0; });
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
  scope.value = "all";
  resultFilter.value = "all";
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
        <h3>归档战报 <small>{{ records.length }} 场</small></h3>
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
    <div v-if="records.length" class="archive-controls">
      <input v-model="query" class="filter" type="search" placeholder="搜索地图、专精、对局 ID 或当时昵称" aria-label="搜索对局" />
      <div class="filter-row">
        <div class="scope-tags" role="group" aria-label="对局范围">
          <button v-for="[key,label] in scopes" :key="key" type="button" :aria-pressed="scope === key" @click="scope = key">{{ label }}</button>
        </div>
        <select v-model="resultFilter" aria-label="对局结果">
          <option value="all">全部结果</option><option value="win">胜利</option><option value="loss">失败</option><option value="spectator">旁观</option><option value="unknown">未判定</option>
        </select>
      </div>
    </div>
    <ul v-if="visible.length" class="reports">
      <li v-for="f in visible" :key="f.matchId">
        <button class="fight" type="button" @click="emit('preview', {id:f.matchId, map:f.map})">
          <MatchTag :kind="f.result" />
          <span class="map">
            <span class="map-title">
              <strong>{{ f.map ?? "地图待识别" }}</strong>
              <MatchTag v-if="f.local" kind="local" />
              <MatchTag v-if="f.together" kind="together" />
              <small v-if="f.relation === 'teammate'">队友</small><small v-else-if="f.relation === 'opponent'">对手</small>
            </span>
            <small>{{ f.at == null ? "时间未知" : time.format(f.at) }}</small>
            <small v-if="f.encounter">{{ phaseNames[f.encounter.phase] }}<template v-if="f.encounter.logName"> · {{ f.encounter.logName }}</template></small>
          </span>
          <span class="specs" v-if="f.report"><span v-for="s in f.report.specs" :key="s.ids[0]"><SpecIcon :id="s.ids[0]!" :label="s.name" />{{ s.name }}</span><small v-if="!f.report.specs.length">专精未识别</small></span>
          <small v-else>仅本地记录</small>
          <span class="numbers"><span>{{ f.seconds == null ? "时长未知" : mmss(f.seconds * 1000) }}</span><small v-if="f.report?.ratingDelta != null" :class="{gain:f.report.ratingDelta > 0, loss:f.report.ratingDelta < 0}">ELO {{ f.report.ratingDelta > 0 ? '+' : '' }}{{ Number(f.report.ratingDelta.toFixed(1)) }}</small></span>
          <FightRoster v-if="f.report" :fight="f.report" :player-id="id" />
          <span class="chevron" aria-hidden="true">›</span>
        </button>
      </li>
    </ul>
    <p v-else-if="!failure" class="hint">{{ loading ? "正在读取归档…" : records.length ? "没有符合条件的战报。" : "暂无归档，查询后可在这里查看战报。" }}</p>
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
.archive-controls { position:sticky; top:0; z-index:4; display:grid; gap:6px; padding:6px 0; background:var(--bax-surface-solid); border-bottom:1px solid var(--bax-border); }
.filter { width:100%; }
.filter-row,.scope-tags {display:flex;align-items:center;flex-wrap:wrap;gap:4px;}
.filter-row {justify-content:space-between;}
.scope-tags button {padding:3px 7px;background:transparent;border:1px solid transparent;color:var(--bax-muted);}
.scope-tags button[aria-pressed="true"] {background:var(--bax-selected-bg);color:var(--bax-fg);border-color:var(--bax-selected-edge);}
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
