<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { MatchAnalysis } from "../shared/bindings/MatchAnalysis";
import type { PlayerReport } from "../shared/bindings/PlayerReport";
import type { Team } from "../shared/bindings/Team";
import { count, mmss, unitClassNames } from "../shared/format";
import { clusterNearby, unitTiles, lifeMinimum, fitMark, inRange, netValue, packLanes, timeBins } from "./sortie-layout";
import type { Entry } from "./sortie-layout";
import { keysInBox, selectionBox, selectKeys } from "./sortie-selection";
import type { Point, SelectionBox, SelectionMark } from "./sortie-selection";
import DeploymentTreemap from "./DeploymentTreemap.vue";
import UnitIcon from "./UnitIcon.vue";
import ReturnBadge from "./ReturnBadge.vue";
import CheckFilter from "./CheckFilter.vue";
import { useSnapshot, errorText } from "../shared/engine";
import { useSetting, saveSetting } from "../shared/settings";

const props = defineProps<{ analysis: MatchAnalysis; players: PlayerReport[]; teams: Team[]; teamLabel: (team: Team) => string; teamColor: (team: Team | null) => string }>();
const emit = defineEmits<{ range: [start: number, end: number] }>();
const mode = ref<"tree" | "players" | "life">("tree");
const team = ref("");
const selectedPlayers = ref<(string | number)[]>([]);
const selectedKinds = ref<(string | number)[]>(Object.keys(unitClassNames));
const playerOptions = computed(() => props.players.filter(p => !team.value || p.team === team.value)
  .map(p => ({ value: p.id, label: playerName(p.id) })));
const kindOptions = Object.entries(unitClassNames).map(([value, label]) => ({ value, label }));
const query = ref("");
const includeRefunded = ref(true);
const batch = ref(0);
const start = ref(0);
const end = ref(1);
const selected = ref<number[]>([]);
const selectedSet = computed(() => new Set(selected.value));
const plot = ref<HTMLElement | null>(null);
const selectionSurface = ref<HTMLElement | null>(null);
const box = ref<SelectionBox | null>(null);
let selectionDrag: { pointerId: number; surface: HTMLElement; anchor: Point; point: Point; client: Point;
  marks: SelectionMark[]; previous: number[]; additive: boolean; moved: boolean } | null = null;
let selectionFrame = 0;
const plotWidth = ref(700);
const preferences = useSetting(useSnapshot(), "ui.sortie");
const plotHeight = ref(600);
const resizeError = ref("");
watch(() => preferences.value.height, (height) => { plotHeight.value = Number.isFinite(height) ? Math.max(240, Math.min(2400, height)) : 600; }, { immediate: true });
async function saveHeight() {
  try { await saveSetting(preferences.value, "ui.sortie", { height: plotHeight.value }); resizeError.value = ""; }
  catch (error) { resizeError.value = errorText(error); }
}
let resizeStart: { y: number; height: number } | null = null;
function resizeBegin(event: PointerEvent) {
  if (event.button !== 0) return;
  resizeStart = { y: event.clientY, height: plotHeight.value };
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}
function resizeMove(event: PointerEvent) {
  if (resizeStart) plotHeight.value = Math.max(240, Math.min(2400, resizeStart.height + event.clientY - resizeStart.y));
}
function resizeEnd() { if (resizeStart) { resizeStart = null; void saveHeight(); } }
function resizeKey(event: KeyboardEvent) {
  if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  plotHeight.value = event.key === "Home" ? 240 : event.key === "End" ? 2400 : Math.max(240, Math.min(2400, plotHeight.value + (event.key === "ArrowDown" ? 60 : -60)));
  void saveHeight();
}
const fullEnd = computed(() => Math.max(1, props.analysis.minutes * 60, props.analysis.durationSecs ?? 0, ...props.analysis.sorties.map((s) => s.spawnSecs)));
const recordEnd = computed(() => Math.max(1, props.analysis.durationSecs ?? 0, ...props.analysis.sorties.flatMap((s) => [s.spawnSecs, s.deathSecs ?? 0])));
const entries = computed<Entry[]>(() => props.analysis.sorties.map((s, key) => ({ ...s, key })));
const owners = computed(() => new Map(props.players.map((p) => [p.id, p])));
const playerName = (id: number) => owners.value.get(id)?.name ?? `ID ${id}`;
const time = (seconds: number) => mmss(seconds * 1000);
const scopeStart = start;
const scopeEnd = end;
const span = computed(() => Math.max(1, end.value - start.value));
const filtered = computed(() => entries.value.filter((s) =>
  (!team.value || owners.value.get(s.player)?.team === team.value) &&
  selectedPlayers.value.includes(s.player) && selectedKinds.value.includes(s.class) &&
  (!query.value.trim() || s.unitName.toLowerCase().includes(query.value.trim().toLowerCase())) &&
  (includeRefunded.value || !s.refunded)));
const scoped = computed(() => filtered.value.filter((s) => inRange(s, scopeStart.value, scopeEnd.value)));
const visiblePlayers = computed(() => props.players.filter((p) => (!team.value || p.team === team.value) && selectedPlayers.value.includes(p.id)));
const detail = computed(() => entries.value.filter((s) => selectedSet.value.has(s.key)));
const ticks = computed(() => Array.from({ length: 5 }, (_, i) => start.value + span.value * i / 4));
const summaries = computed(() => props.teams.map((t) => {
  const units = scoped.value.filter((s) => owners.value.get(s.player)?.team === t);
  const classes = Object.entries(unitClassNames).map(([cls, label]) => {
    const matching = units.filter((s) => s.class === cls);
    return { label, count: matching.length, value: netValue(matching) };
  }).filter((x) => x.count);
  return { team: t, entries: units, value: netValue(units), classes };
}));
const overview = computed(() => timeBins(filtered.value, 0, fullEnd.value, fullEnd.value / 80));
const overviewMax = computed(() => Math.max(1, ...overview.value.map((b) => b.entries.length)));
const rows = computed(() => visiblePlayers.value.filter(() => filtered.value.length && mode.value !== "tree").map((p) => {
  const units = filtered.value.filter((s) => s.player === p.id && (mode.value === "life"
    ? s.spawnSecs <= end.value && (s.refunded ? s.spawnSecs >= start.value : (s.deathSecs ?? recordEnd.value) >= start.value)
    : inRange(s, start.value, end.value)));
  const clusters = mode.value === "life" ? units.map((s) => ({ entries: [s], start: s.spawnSecs, end: s.spawnSecs }))
    : clusterNearby(units, span.value, plotWidth.value, batch.value);
  const marks = packLanes(clusters.map((c) => {
    const first = c.entries[0];
    const tiles = unitTiles(c.entries);
    const iconWidth = 44;
    const tileWidth = Math.max(76, ...tiles.map((tile) => 44 + 3 + (tile.count > 1 ? (String(tile.count).length + 1) * 7 : 0) + (tile.unit.refunded ? 12 : 0)));
    const columns = Math.max(1, Math.min(3, tiles.length, Math.floor((plotWidth.value - 10) / tileWidth)));
    const minimum = mode.value === "life" ? lifeMinimum(iconWidth, first.refunded)
      : tiles.length === 1 ? lifeMinimum(44, first.refunded) + (tiles[0].count > 1 ? 22 : 0) : columns * tileWidth + 10;
    const x = Math.max(0, (c.start - start.value) / span.value * plotWidth.value);
    const lifeWidth = ((first.deathSecs ?? recordEnd.value) - Math.max(start.value, c.start)) / span.value * plotWidth.value;
    const desiredWidth = mode.value === "life" && !first.refunded ? lifeWidth : minimum;
    const height = mode.value === "life" ? 30 : Math.ceil(tiles.length / columns) * 32 + 6;
    return { ...c, ...fitMark(x, desiredWidth, plotWidth.value, minimum), iconWidth, tiles, columns, height };
  }));
  const laneHeights: number[] = [];
  for (const mark of marks) laneHeights[mark.lane] = Math.max(laneHeights[mark.lane] ?? 0, mark.height + 6);
  const positioned = marks.map((mark) => ({ ...mark, top: 6 + laneHeights.slice(0, mark.lane).reduce((sum, height) => sum + height, 0) }));
  return { player: p, marks: positioned, height: Math.max(48, laneHeights.reduce((sum, height) => sum + height, 0) + 12) };

}));
function setRange(a: number, b: number) {
  const lo = Math.max(0, Math.min(fullEnd.value, Math.round(Math.min(a, b))));
  start.value = lo;
  end.value = Math.max(lo, Math.min(fullEnd.value, Math.round(Math.max(a, b))));
}
function zoom(factor: number) {
  const width = Math.min(fullEnd.value, Math.max(15, span.value * factor));
  const center = (start.value + end.value) / 2;
  const lo = Math.max(0, Math.min(fullEnd.value - width, center - width / 2));
  setRange(lo, lo + width);
}
function pan(direction: number) {
  const lo = Math.max(0, Math.min(fullEnd.value - span.value, start.value + direction * span.value / 2));
  setRange(lo, lo + span.value);
}
function resetFilters() {
  team.value = ""; selectedPlayers.value = props.players.map(p => p.id); selectedKinds.value = Object.keys(unitClassNames); query.value = ""; includeRefunded.value = true;
}
function select(units: Entry[], event: MouseEvent) { selected.value = selectKeys(selected.value, units.map(s => s.key), event.shiftKey, true); }
function selectionPoint(client: Point, surface: HTMLElement): Point {
  const rect = surface.getBoundingClientRect();
  return { x: Math.max(0, Math.min(rect.width, client.x - rect.left)), y: Math.max(0, Math.min(rect.height, client.y - rect.top)) };
}
function beginSelection(event: PointerEvent) {
  const surface = selectionSurface.value;
  if (mode.value !== "life" || event.button !== 0 || event.pointerType === "touch" || !surface ||
    (event.target as Element).closest("button, a, input, select")) return;
  event.preventDefault();
  surface.focus({ preventScroll: true });
  const rect = surface.getBoundingClientRect();
  const client = { x: event.clientX, y: event.clientY };
  const point = selectionPoint(client, surface);
  const marks = [...surface.querySelectorAll<HTMLElement>(".mark")].map(el => {
    const bounds = el.getBoundingClientRect();
    return { key: Number(el.dataset.key), x: bounds.left - rect.left, y: bounds.top - rect.top, width: bounds.width, height: bounds.height };
  });
  selectionDrag = { pointerId: event.pointerId, surface, anchor: point, point, client, marks,
    previous: [...selected.value], additive: event.shiftKey, moved: false };
  surface.setPointerCapture(event.pointerId);
}
function applySelection() {
  const drag = selectionDrag;
  if (!drag) return;
  const next = selectionBox(drag.anchor, drag.point);
  drag.moved ||= Math.max(next.width, next.height) >= 4;
  if (!drag.moved) return;
  box.value = next;
  selected.value = selectKeys(drag.previous, keysInBox(next, drag.marks), drag.additive);
}
function moveSelection(event?: PointerEvent) {
  const drag = selectionDrag;
  if (!drag || (event && event.pointerId !== drag.pointerId)) return;
  if (event) drag.client = { x: event.clientX, y: event.clientY };
  drag.point = selectionPoint(drag.client, drag.surface);
  cancelAnimationFrame(selectionFrame);
  selectionFrame = requestAnimationFrame(applySelection);
}
function closeSelection(cancelled = false) {
  const drag = selectionDrag;
  if (!drag) return;
  cancelAnimationFrame(selectionFrame);
  if (cancelled) selected.value = drag.previous;
  selectionDrag = null;
  box.value = null;
  if (drag.surface.hasPointerCapture(drag.pointerId)) drag.surface.releasePointerCapture(drag.pointerId);
}
function finishSelection(event: PointerEvent) {
  const drag = selectionDrag;
  if (!drag || event.pointerId !== drag.pointerId) return;
  drag.point = selectionPoint({ x: event.clientX, y: event.clientY }, drag.surface);
  applySelection();
  if (!drag.moved && !drag.additive) selected.value = [];
  closeSelection();
}
function escapeSelection(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (selectionDrag) closeSelection(true);
  else selected.value = [];
  event.stopPropagation();
}
function title(units: Entry[]) {
  const first = units[0];
  const last = Math.max(...units.map((s) => s.spawnSecs));
  return `${playerName(first.player)} · ${time(first.spawnSecs)}${last !== first.spawnSecs ? `–${time(last)}` : ''} · ${[...new Set(units.map((s) => s.unitName))].join("、")} ×${units.length}${units.some((s) => s.refunded) ? ` · 含回收 ${units.filter((s) => s.refunded).length} 次` : ''} · 净估算价值 ${count(netValue(units))}`;
}
let dragAnchor: number | null = null;
function pointerTime(event: PointerEvent) {
  const rect = (event.currentTarget as Element).getBoundingClientRect();
  return Math.max(0, Math.min(fullEnd.value, (event.clientX - rect.left) / rect.width * fullEnd.value));
}
function beginDrag(event: PointerEvent) {
  if (event.button !== 0) return;
  dragAnchor = pointerTime(event);
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
}
function drag(event: PointerEvent) {
  if (dragAnchor != null) setRange(dragAnchor, pointerTime(event));
}
function finishDrag(event: PointerEvent) {
  if (dragAnchor == null) return;
  const at = pointerTime(event);
  if (Math.abs(at - dragAnchor) > 2) setRange(dragAnchor, at);
  dragAnchor = null;
}
let observer: ResizeObserver | undefined;
let resizeFrame = 0;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => { plotWidth.value = Math.max(150, entry.contentRect.width - 164); });
  });
  if (plot.value) observer.observe(plot.value);
});
watch(plot, (current, previous) => {
  if (previous) observer?.unobserve(previous);
  if (current) observer?.observe(current);
});
onBeforeUnmount(() => { observer?.disconnect(); cancelAnimationFrame(resizeFrame); closeSelection(true); });
watch(() => props.analysis.matchId, () => { start.value = 0; end.value = fullEnd.value; selected.value = []; resetFilters(); }, { immediate: true });
watch([scopeStart, scopeEnd], () => emit("range", scopeStart.value, scopeEnd.value), { immediate: true });
watch([filtered, mode, scopeStart, scopeEnd], () => { closeSelection(); selected.value = []; });
watch(mode, async () => { await nextTick(); if (plot.value) plot.value.scrollTop = 0; });
</script>

<template>
  <section class="explorer" aria-label="出兵分析">
    <div class="explorer-controls">
    <div class="heading"><h3>出兵分析</h3><span v-if="mode !== 'tree'" class="muted">{{ count(scoped.length) }} 次 · {{ time(scopeStart) }}–{{ time(scopeEnd) }}</span></div>
    <div class="modes" role="group" aria-label="分析视图">
      <button v-for="item in ([['tree', '出兵分布'], ['life', '存续时间'], ['players', '出兵批次']] as const)" :key="item[0]" :aria-pressed="mode === item[0]" @click="mode = item[0]">{{ item[1] }}</button>
    </div>
    <template v-if="mode !== 'tree'">
    <div class="filters">
      <label>队伍<select v-model="team" class="dropdown-trigger"><option value="">双方队伍</option><option v-for="t in teams" :key="t" :value="t">{{ teamLabel(t) }}</option></select></label>
      <CheckFilter v-model="selectedPlayers" label="玩家" :options="playerOptions" />
      <CheckFilter v-model="selectedKinds" label="兵种" :options="kindOptions" />
      <label class="search">单位<input v-model="query" type="search" placeholder="搜索单位名称" /></label>
      <label class="check"><input v-model="includeRefunded" type="checkbox" />包含回收</label>
      <button @click="resetFilters">重置筛选</button>
    </div>

      <div class="toolbar">
        <button :disabled="span <= 15" @click="zoom(0.5)">放大</button><button :disabled="span >= fullEnd" @click="zoom(2)">缩小</button>
        <button :disabled="start === 0" @click="pan(-1)">前移</button><button :disabled="end === fullEnd" @click="pan(1)">后移</button>
        <button @click="setRange(0, fullEnd)">全局</button>
        <label v-if="mode === 'players'">批次<select v-model.number="batch"><option :value="0">自动密度</option><option :value="5">5 秒</option><option :value="15">15 秒</option><option :value="30">30 秒</option></select></label>
      </div>
      <div class="navigator">
        <svg class="overview" viewBox="0 0 800 40" preserveAspectRatio="none" aria-label="全局出兵密度，拖动选择时间范围" @pointerdown="beginDrag" @pointermove="drag" @pointerup="finishDrag" @pointercancel="dragAnchor = null">
          <rect v-for="(b, i) in overview" :key="i" :x="i * 10" :y="38 - b.entries.length / overviewMax * 32" width="8" :height="Math.max(1, b.entries.length / overviewMax * 32)" fill="var(--bax-muted)" />
          <rect :x="start / fullEnd * 800" y="1" :width="Math.max(2, span / fullEnd * 800)" height="38" fill="var(--bax-accent)" fill-opacity=".13" stroke="var(--bax-accent)" />
        </svg>
        <div class="ranges"><label>从 {{ time(start) }}<input aria-label="区间开始" type="range" min="0" :max="fullEnd" :value="start" @input="setRange(Math.min(Number(($event.target as HTMLInputElement).value), end), end)" /></label><label>至 {{ time(end) }}<input aria-label="区间结束" type="range" min="0" :max="fullEnd" :value="end" @input="setRange(start, Math.max(Number(($event.target as HTMLInputElement).value), start))" /></label></div>
      </div>

      <p v-if="mode === 'life'" class="selection-help muted">空白处拖动框选；Shift + 点击增减选择，Shift + 框选追加。Esc 取消框选。</p>
      <div class="axis" :style="{ width: `${plotWidth + 164}px` }"><span>玩家 / 队伍</span><div><span v-for="tick in ticks" :key="tick">{{ time(tick) }}</span></div></div>
    </template>
    </div>
    <DeploymentTreemap v-if="mode === 'tree'" :players="players" :teams="teams" :team-label="teamLabel" :team-color="teamColor" />
    <template v-else>
    <div ref="plot" class="plot" :style="{ height: `${plotHeight}px` }" @scroll="moveSelection()">
        <Transition name="analysis-view" mode="out-in"><div :key="mode" ref="selectionSurface" class="plot-content" :class="{ selecting: box }" :tabindex="mode === 'life' ? 0 : undefined" :aria-label="mode === 'life' ? '存续时间单位选择' : undefined" @pointerdown="beginSelection" @pointermove="moveSelection" @pointerup="finishSelection" @pointercancel="closeSelection(true)" @lostpointercapture="closeSelection(true)" @keydown="escapeSelection">
        <div v-for="row in rows" :key="row.player.id" class="timeline-row" :style="{ minHeight: `${row.height}px` }">
          <button class="owner" :style="{ color: teamColor(row.player.team) }" @click="selectedPlayers = [row.player.id]">{{ playerName(row.player.id) }}<small>{{ mode === 'life' ? '记录存续' : `${row.marks.reduce((n, m) => n + m.entries.length, 0)} 次出兵` }}</small></button>
          <div class="lane" :style="{ height: `${row.height}px` }">
            <i v-for="n in 5" :key="n" class="gridline" :style="{ left: `${(n - 1) * 25}%` }" />
            <button v-for="mark in row.marks" :key="mark.entries[0].key" :data-key="mark.entries[0].key" class="mark" :class="{ batch: mode !== 'life', life: mode === 'life' && !mark.entries[0].refunded, selected: mark.entries.some((s) => selectedSet.has(s.key)) }" :style="{ left: `${mark.x}px`, top: `${mark.top}px`, height: `${mark.height}px`, width: `${mark.width}px`, '--team-color': teamColor(row.player.team) }" :title="title(mark.entries)" :aria-label="title(mark.entries)" :aria-pressed="mark.entries.some((s) => selectedSet.has(s.key))" @click="select(mark.entries, $event)">
              <template v-if="mode === 'life'">
                <span class="mark-visual" :style="{ width: `${mark.iconWidth}px` }">
                  <UnitIcon unit-only :id="mark.entries[0].unitId" :thumbnail="mark.entries[0].thumbnail" :name="mark.entries[0].unitName" :kind="mark.entries[0].class" />
                </span>
                <span v-if="mark.width >= 110" class="mark-name">{{ mark.entries[0].unitName }}</span>
                <ReturnBadge v-if="mark.entries[0].refunded" class="refund-corner" />
              </template>
              <span v-else class="batch-units" :style="{ gridTemplateColumns: `repeat(${mark.columns}, minmax(0, 1fr))` }">
                <span v-for="tile in mark.tiles" :key="tile.key" class="batch-unit" :class="{ 'has-rtb': tile.unit.refunded }">
                  <UnitIcon :id="tile.unit.unitId" :thumbnail="tile.unit.thumbnail" :name="tile.unit.unitName" :kind="tile.unit.class" unit-only />
                  <b v-if="tile.count > 1">×{{ tile.count }}</b>
                  <ReturnBadge v-if="tile.unit.refunded" class="refund-corner" />
                </span>
              </span>

            </button>
          </div>
        </div>
      <p v-if="!filtered.length" class="empty">没有符合筛选的出兵记录。<button @click="resetFilters">清除筛选</button></p>
      <p v-else-if="!scoped.length && mode !== 'life'" class="empty">所选时间范围内没有出兵。</p>
        <div v-if="box" class="selection-box" :style="{ left: `${box.x}px`, top: `${box.y}px`, width: `${box.width}px`, height: `${box.height}px` }" aria-hidden="true" />
        </div></Transition>
    </div>
    <div class="resize-handle" role="separator" tabindex="0" aria-label="调整出兵时间线高度" aria-orientation="horizontal" :aria-valuenow="plotHeight" :aria-valuemin="240" :aria-valuemax="2400" @pointerdown="resizeBegin" @pointermove="resizeMove" @pointerup="resizeEnd" @pointercancel="resizeEnd" @lostpointercapture="resizeEnd" @keydown="resizeKey"><span /></div>
    <p v-if="resizeError" class="error">高度保存失败：{{ resizeError }}</p>
    <div class="symbol-legend" aria-label="图标说明">
      <span><ReturnBadge />已回收</span>
    </div>

    <div class="analysis-bottom">
      <section class="interval" aria-label="区间投入统计">
        <h4>区间投入 <span class="muted">{{ time(scopeStart) }}–{{ time(scopeEnd) }}</span></h4>
        <p class="muted">按当前筛选和出兵时间统计，回收计次数、净价值记 0。</p>
        <div class="comparison"><div v-for="s in summaries" :key="s.team"><strong :style="{ color: teamColor(s.team) }">{{ teamLabel(s.team) }}</strong><p><b>{{ count(s.entries.length) }}</b> 次 <span class="muted">/</span> <b>{{ count(s.value) }}</b> 净估算价值</p><div v-for="c in s.classes" :key="c.label" class="stat-line"><span>{{ c.label }}</span><span>{{ c.count }} 次 · {{ s.value ? Math.round(c.value / s.value * 100) : 0 }}%</span></div></div></div>
        <details><summary>各玩家投入</summary><div v-for="p in visiblePlayers" :key="p.id" class="stat-line"><button class="text-button" @click="selectedPlayers = [p.id]">{{ playerName(p.id) }}</button><span>{{ scoped.filter((s) => s.player === p.id).length }} 次 · {{ count(netValue(scoped.filter((s) => s.player === p.id))) }}</span></div></details>
      </section>
      <section class="detail" aria-label="选中单位详情" aria-live="polite">
        <div class="heading"><h4>单位详情 <span v-if="detail.length" class="muted">{{ detail.length }} 次出兵</span></h4><button v-if="detail.length" @click="selected = []">取消选择</button></div>
        <p v-if="!detail.length" class="muted">点击时间线上的单位，详情固定在这里。</p>
        <div v-else class="detail-list"><article v-for="s in detail" :key="s.key"><div class="unit-title"><UnitIcon unit-only :id="s.unitId" :thumbnail="s.thumbnail" :name="s.unitName" :kind="s.class" /><button class="text-button" @click="query = s.unitName">{{ s.unitName }}</button><span class="muted">{{ unitClassNames[s.class] }}</span><ReturnBadge v-if="s.refunded" /></div><p>{{ playerName(s.player) }} · {{ time(s.spawnSecs) }} 出兵<br />{{ s.refunded ? '已回收，时间未知' : s.deathSecs != null ? `${time(s.deathSecs)} 阵亡 · 存续 ${time(Math.max(0, s.deathSecs - s.spawnSecs))}` : '未记录阵亡' }}<br />估算成本 {{ count(s.cost) }} · 击毁 {{ s.kills }}<span v-if="s.airdropped"> · 空投</span><span v-if="s.transferredTo != null"><br />转交给 {{ playerName(s.transferredTo) }}</span></p></article></div>
      </section>
    </div>
    </template>
  </section>
</template>

<style scoped>
.explorer { margin: 24px 0; min-width: 0; font-size: calc(13px * var(--bax-font-scale)); font-variant-numeric: tabular-nums; container-type: inline-size; }
.explorer-controls { position: sticky; top: 0; z-index: 5; background: var(--bax-surface-solid); padding: 10px 0 2px; border-bottom: 1px solid var(--bax-border); }
.analysis-view-enter-active { transition: opacity 180ms ease-out, transform 180ms cubic-bezier(.16, 1, .3, 1); }
.analysis-view-leave-active { transition: opacity 100ms ease-out; }
.analysis-view-enter-from { opacity: 0; transform: translateY(4px); }
.analysis-view-leave-to { opacity: 0; }
@media (prefers-reduced-motion: reduce) { .analysis-view-enter-active, .analysis-view-leave-active { transition: none; } .analysis-view-enter-from { transform: none; } }
.heading, .toolbar, .filters, .modes { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; }
.heading { justify-content: space-between; }
h3, h4 { margin: 0; color: var(--bax-fg); font-size: calc(14px * var(--bax-font-scale)); }
h4 { font-size: calc(13px * var(--bax-font-scale)); }
.muted, small { color: var(--bax-soft); font-size: calc(12px * var(--bax-font-scale)); }
.modes { margin: 12px 0 16px; border-bottom: 1px solid var(--bax-border); gap: 16px; }
.modes button { border: 0; border-bottom: 2px solid transparent; border-radius: 0; background: transparent; padding: 8px 0; color: var(--bax-soft); }
.modes button[aria-pressed="true"] { color: var(--bax-fg); border-bottom-color: var(--bax-accent); }
.filters { align-items: end; gap: 12px; }
label { display: flex; flex-direction: column; gap: 5px; color: var(--bax-soft); font-size: calc(12px * var(--bax-font-scale)); }
select, input[type="search"] { min-height: 32px; padding: 4px 8px; border: 1px solid var(--bax-faint); border-radius: var(--bax-radius-xs); background: var(--bax-surface-2); color: var(--bax-fg); max-width: 190px; }
.search { flex: 1; min-width: 140px; }.search input { width: 100%; max-width: none; }
.check { flex-direction: row; align-items: center; min-height: 32px; }
.toolbar { margin: 16px 0 10px; }.toolbar label { flex-direction: row; align-items: center; margin-left: auto; }
.navigator { margin: 12px 0; }.overview { width: 100%; height: 40px; cursor: crosshair; touch-action: none; }
.ranges { display: flex; gap: 24px; }.ranges label { flex: 1; }.ranges input { width: 100%; }
.plot { min-width: 0; overflow: auto; scrollbar-gutter: stable; scrollbar-color: var(--bax-faint) var(--bax-surface); }
.plot-content { position: relative; }
.plot-content:focus-visible { outline: 1px solid var(--bax-accent); outline-offset: -1px; }
.plot-content.selecting { user-select: none; cursor: crosshair; }
.selection-box { position: absolute; z-index: 3; pointer-events: none; border: 1px solid var(--bax-accent); background: color-mix(in srgb, var(--bax-accent) 15%, transparent); }
.selection-help { margin: 8px 0; }
.detail-list { scrollbar-color: var(--bax-faint) var(--bax-surface); }
.timeline-row:hover, .stat-line:hover, .detail-list article:hover, .stat-line:focus-within, .detail-list article:focus-within { background: var(--bax-row-hover); }
.resize-handle { height: 18px; display: grid; place-items: center; cursor: ns-resize; touch-action: none; border-top: 1px solid var(--bax-border); }
.resize-handle span { width: 40px; height: 3px; background: var(--bax-faint); border-radius: min(2px, var(--bax-radius-xs)); }
.resize-handle:hover span, .resize-handle:focus-visible span { background: var(--bax-accent); }
.resize-handle:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 2px; }
.axis, .timeline-row { display: grid; grid-template-columns: 152px minmax(150px, 1fr); gap: 12px; }
.axis { max-width: 100%; padding: 8px 0; color: var(--bax-soft); font-size: calc(11px * var(--bax-font-scale)); }
.axis > div { display: flex; justify-content: space-between; }.timeline-row { border-top: 1px solid var(--bax-border); }
.owner { text-align: left; border: 0; border-radius: 0; background: transparent; padding: 6px 0; overflow-wrap: anywhere; min-width: 0; align-self: start; }.owner small { display: block; font-size: calc(11px * var(--bax-font-scale)); margin-top: 2px; }
.lane { position: relative; min-width: 150px; }.gridline { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--bax-border); pointer-events: none; }
.gridline:last-of-type { transform: translateX(-1px); }
.mark { position: absolute; display: flex; align-items: center; justify-content: flex-start; gap: 3px; height: 30px; padding: 0 4px; border-radius: var(--bax-radius-xs); border-color: var(--bax-faint); overflow: hidden; font-size: 11px; /* 固定尺寸框，字号不缩放 */ }
.mark-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.mark b { margin-left: auto; flex-shrink: 0; }
.mark.batch { color: white; }.mark.life { border-bottom: 2px solid var(--team-color); }.mark.selected { outline: 2px solid var(--bax-accent); outline-offset: 1px; }
.mark-visual { display: flex; align-items: center; justify-content: center; flex: 0 0 auto; height: 24px; min-width: 0; }
.mark-visual :deep(.unit-icon) { width: 44px; height: 24px; object-fit: contain; flex-shrink: 0; }
.mark .refund-corner { position: absolute; right: 1px; bottom: 1px; width: 10px; height: 10px; pointer-events: none; }
.symbol-legend { display: flex; flex-wrap: wrap; gap: 8px 16px; margin: 0 0 18px; font-size: calc(12px * var(--bax-font-scale)); color: var(--bax-soft); }
.symbol-legend > span { display: inline-flex; align-items: center; gap: 6px; }
.symbol-legend :deep(.unit-glyph) { flex-shrink: 0; }
.batch-units { display: grid; width: 100%; gap: 0; }
.batch-unit { display: flex; align-items: center; position: relative; height: 32px; min-width: 0; }

.batch-unit :deep(.unit-icon) { width: 44px; height: 24px; flex-shrink: 0; }
.batch-unit b { position: absolute; left: 46px; top: 4px; margin: 0; padding: 0; font-size: 11px; line-height: 12px; white-space: nowrap; /* 固定尺寸框，字号不缩放 */ }
.mark .batch-unit .refund-corner { left: 46px; right: auto; bottom: 4px; }
.legend { margin: 10px 0 18px; line-height: 1.7; }.empty { padding: 24px 0; text-align: center; color: var(--bax-soft); }
.analysis-bottom { display: grid; grid-template-columns: 1fr 1fr; border-top: 1px solid var(--bax-border); gap: 24px; padding-top: 16px; }.analysis-bottom > section { min-width: 0; }.comparison { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }.comparison p { font-size: calc(12px * var(--bax-font-scale)); }.comparison b { font-size: calc(16px * var(--bax-font-scale)); font-weight: 500; }.stat-line { display: flex; justify-content: space-between; gap: 10px; padding: 3px 0; font-size: calc(12px * var(--bax-font-scale)); }.stat-line > span:last-child { white-space: nowrap; }.detail-list { max-height: 270px; overflow-y: auto; margin-top: 8px; }.detail-list article { padding: 10px 0; border-bottom: 1px solid var(--bax-border); }.detail-list p { margin: 4px 0; font-size: calc(12px * var(--bax-font-scale)); line-height: 1.8; }.unit-title { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }.text-button { border: 0; background: transparent; padding: 0; text-align: left; color: var(--bax-accent); }.text-button:hover { text-decoration: underline; }details { margin-top: 12px; }summary { cursor: pointer; color: var(--bax-soft); }
button:focus-visible, input:focus-visible, select:focus-visible, summary:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 3px; }
@media (max-width: 850px) { .analysis-bottom { grid-template-columns: 1fr; }.filters label:not(.check) { flex: 1 1 140px; }.filters select { max-width: none; width: 100%; }.toolbar label { margin-left: 0; } }
@container (max-width: 680px) { .analysis-bottom { grid-template-columns: 1fr; } }
@media (max-width: 500px) { .comparison { gap: 12px; }.modes { gap: 12px; }.ranges { gap: 12px; } }
</style>
