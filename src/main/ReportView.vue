<script setup lang="ts">
// 一场比赛的战后分析。数据在 Rust 里统计好（bax-core::report），这里只负责画。
import type { EChartsCoreOption } from "echarts/core";
import { computed, ref, watch } from "vue";

import type { HistoryRow } from "../shared/bindings/HistoryRow";
import type { MatchReportView } from "../shared/bindings/MatchReportView";
import type { PlayerReport } from "../shared/bindings/PlayerReport";
import type { Team } from "../shared/bindings/Team";
import { commands, errorText, useSnapshot } from "../shared/engine";
import {
  compositionGroups,
  count,
  mmss,
  rating,
  teamNames,
  unitClassNames,
} from "../shared/format";
import EChart from "./EChart.vue";
import SortieExplorer from "./SortieExplorer.vue";
import { openPlayer } from "./navigation";

const props = defineProps<{ id: number; row?: HistoryRow | null }>();
const emit = defineEmits<{ requested: [] }>();

const snapshot = useSnapshot();
const view = ref<MatchReportView | null>(null);
const loading = ref(false);
const failure = ref<string | null>(null);
const expanded = ref<number | null>(null);
const requesting = ref(false);
const selectedRange = ref<[number, number] | null>(null);

const isCancelled = () => props.row?.report.state === "cancelled";

async function load() {
  if (props.row?.report.state === "cancelled") { view.value = null; return; }
  const id = props.id;
  loading.value = true;
  failure.value = null;
  try {
    const result = await commands.matchReport(id);
    if (props.id === id && !isCancelled()) view.value = result;
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    loading.value = false;
  }
}

watch(
  () => [props.id, props.row?.report.state],
  () => {
    view.value = null;
    expanded.value = null;
    void load();
  },
  { immediate: true },
);
// 归档情况变了（新战报、单位目录更新）就重新读
watch(
  () => snapshot.value?.archive.seq,
  () => void load(),
);

async function requestNow() {
  if (props.row?.report.state === "cancelled") return;
  requesting.value = true;
  try {
    await commands.fetchReport(props.id);
    emit("requested");
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    requesting.value = false;
  }
}

// ------------------------------------------------------------ 队伍与颜色

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

const colors = computed(() => ({
  fg: cssVar("--bax-fg"),
  muted: cssVar("--bax-muted"),
  faint: cssVar("--bax-faint"),
  grid: cssVar("--bax-border"),
  surface: cssVar("--bax-surface"),
  surface2: cssVar("--bax-surface-2"),
  friend: cssVar("--bax-friend"),
  foe: cssVar("--bax-foe"),
  alpha: cssVar("--bax-alpha"),
  bravo: cssVar("--bax-bravo"),
}));

const selfTeam = computed<Team | null>(() => {
  const v = view.value;
  if (!v) return null;
  const me = v.analysis.players.find((p) => p.id === v.selfId);
  return me?.team ?? v.selfTeam ?? null;
});

/** 本方在前。 */
const teamOrder = computed<Team[]>(() => (selfTeam.value === "bravo" ? ["bravo", "alpha"] : ["alpha", "bravo"]));

function teamLabel(team: Team): string {
  const mine = selfTeam.value;
  if (mine === "alpha" || mine === "bravo") return team === mine ? `本方（${teamNames[team]}）` : `对方（${teamNames[team]}）`;
  return teamNames[team];
}

function teamColor(team: Team | null): string {
  const mine = selfTeam.value;
  if (mine === "alpha" || mine === "bravo") return team === mine ? colors.value.friend : colors.value.foe;
  return team === "bravo" ? colors.value.bravo : colors.value.alpha;
}

const players = computed<PlayerReport[]>(() => {
  const v = view.value;
  if (!v) return [];
  const order = teamOrder.value;
  return [...v.analysis.players].sort(
    (a, b) => order.indexOf(a.team ?? "spectators") - order.indexOf(b.team ?? "spectators"),
  );
});

function name(p: PlayerReport): string {
  return p.name ?? `ID ${p.id}`;
}



const result = computed(() => {
  const v = view.value;
  const mine = selfTeam.value;
  if (!v?.analysis.winner || (mine !== "alpha" && mine !== "bravo")) return null;
  return v.analysis.winner === mine ? "胜" : "负";
});

function date(at: number | null | undefined): string {
  if (at == null) return "–";
  return new Date(at).toLocaleString("zh-CN", { hour12: false, month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

function escape(text: string): string {
  return text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function eloDelta(p: PlayerReport): string {
  if (p.oldRating == null || p.newRating == null) return "";
  const d = Math.round(p.newRating - p.oldRating);
  return d > 0 ? `+${d}` : String(d);
}

// ------------------------------------------------------------ 图表

const axisCommon = computed(() => ({
  axisLine: { lineStyle: { color: colors.value.faint } },
  axisTick: { show: false },
  axisLabel: { color: colors.value.muted, fontSize: 11 },
  splitLine: { lineStyle: { color: colors.value.grid } },
}));

const tooltipCommon = computed(() => ({
  backgroundColor: colors.value.surface2,
  borderColor: colors.value.grid,
  textStyle: { color: colors.value.fg, fontSize: 12 },
  confine: true,
}));

/** 纵轴的玩家名：文字用正文颜色，前面一小块队伍颜色。 */
function playerAxisLabel(rows: PlayerReport[]) {
  const c = colors.value;
  return {
    color: c.fg,
    fontSize: 11,
    formatter: (value: string, i: number) => {
      const short = value.length > 14 ? `${value.slice(0, 13)}…` : value;
      return `{t${rows[i]?.team === teamOrder.value[0] ? 0 : 1}|▍}${short}`;
    },
    rich: {
      t0: { color: teamColor(teamOrder.value[0]), padding: [0, 4, 0, 0] },
      t1: { color: teamColor(teamOrder.value[1]), padding: [0, 4, 0, 0] },
    },
  };
}

/** 双方累计出兵价值随时间。 */
const curveOption = computed<EChartsCoreOption | null>(() => {
  const v = view.value;
  if (!v) return null;
  const series = teamOrder.value.map((team) => {
    const t = v.analysis.teams.find((x) => x.team === team);
    let acc = 0;
    const data: [number, number][] = [[0, 0]];
    (t?.valueByMinute ?? []).forEach((value, i) => {
      acc += value;
      data.push([i + 1, acc]);
    });
    return {
      name: teamLabel(team),
      type: "line",
      data,
      showSymbol: false,
      lineStyle: { width: 2 },
      color: teamColor(team),
      endLabel: { show: true, color: colors.value.muted, fontSize: 11, formatter: (p: { seriesName: string }) => p.seriesName },
      emphasis: { focus: "series" },
      markArea: selectedRange.value ? {
        silent: true,
        itemStyle: { color: colors.value.fg, opacity: 0.05 },
        data: [[{ xAxis: selectedRange.value[0] / 60 }, { xAxis: selectedRange.value[1] / 60 }]],
      } : undefined,
    };
  });
  return {
    animation: false,
    grid: { left: 56, right: 120, top: 32, bottom: 28 },
    legend: { top: 0, left: 0, textStyle: { color: colors.value.muted }, itemWidth: 16, itemHeight: 2, icon: "rect" },
    tooltip: {
      ...tooltipCommon.value,
      trigger: "axis",
      axisPointer: { type: "line", lineStyle: { color: colors.value.faint } },
      formatter: (items: { seriesName: string; value: [number, number]; color: string }[]) => {
        if (!items.length) return "";
        const head = `开局后第 ${items[0].value[0]} 分钟末`;
        const rows = items.map((i) => `<span style="color:${i.color}">■</span> ${escape(i.seriesName)}　${count(i.value[1])}`);
        return [head, ...rows].join("<br>");
      },
    },
    xAxis: { type: "value", min: 0, max: v.analysis.minutes, minInterval: 1, ...axisCommon.value, splitLine: { show: false }, axisLabel: { ...axisCommon.value.axisLabel, formatter: "{value} 分" } },
    yAxis: { type: "value", ...axisCommon.value },
    series,
  };
});

/** 每人的兵种构成（估算价值）。 */
const compositionOption = computed<EChartsCoreOption | null>(() => {
  const v = view.value;
  if (!v) return null;
  const rows = players.value;
  const totals = rows.map((p) => p.composition.reduce((s, c) => s + c.value, 0));
  const series = compositionGroups.map((group) => ({
    name: group.name,
    type: "bar",
    stack: "value",
    color: group.color,
    barWidth: 14,
    itemStyle: { borderColor: colors.value.surface, borderWidth: 1 },
    emphasis: { focus: "series" },
    data: rows.map((p) =>
      p.composition.filter((c) => group.classes.includes(c.class)).reduce((s, c) => s + c.value, 0),
    ),
  }));
  return {
    animation: false,
    grid: { left: 130, right: 24, top: 36, bottom: 28 },
    legend: { top: 0, left: 0, textStyle: { color: colors.value.muted }, itemWidth: 12, itemHeight: 12 },
    tooltip: {
      ...tooltipCommon.value,
      trigger: "item",
      formatter: (item: { seriesName: string; dataIndex: number; value: number; color: string }) => {
        const p = rows[item.dataIndex];
        const total = totals[item.dataIndex] || 1;
        return `${escape(name(p))}<br><span style="color:${item.color}">■</span> ${escape(item.seriesName)}　${count(item.value)}（${Math.round((item.value / total) * 100)}%）`;
      },
    },
    xAxis: { type: "value", ...axisCommon.value },
    yAxis: {
      type: "category",
      inverse: true,
      data: rows.map((p) => name(p)),
      ...axisCommon.value,
      splitLine: { show: false },
      axisLabel: playerAxisLabel(rows),
    },
    series,
  };
});

const compositionHeight = computed(() => players.value.length * 26 + 70);

function pendingText(row: HistoryRow | null | undefined): string {
  const r = row?.report;
  if (!r) return "还没有这场比赛的战报。";
  switch (r.state) {
    case "cancelled":
      return "这场比赛在准备阶段取消，不获取战报。";
    case "pending":
      return `正在等 BATrace 收录战报：已经试了 ${r.attempts} 次，${date(r.nextAt)} 再试。`;
    case "gaveUp":
      return `试了 ${r.attempts} 次都没有取到战报${r.lastError ? `（${r.lastError}）` : ""}，不再自动尝试。`;
    default:
      return "还没有这场比赛的战报。";
  }
}
</script>

<template>
  <section class="card report">
    <p v-if="failure" class="error">{{ failure }}</p>
    <template v-if="view">
      <div class="row head">
        <h2>比赛 {{ view.analysis.matchId }}</h2>
        <span v-if="result" class="result" :class="result === '胜' ? 'win' : 'loss'">{{ result }}</span>
        <span class="hint">
          {{ view.map ?? (view.analysis.mapId != null ? `地图 #${view.analysis.mapId}` : "地图未知") }} ·
          {{ date(view.analysis.startTime) }} ·
          时长 {{ view.analysis.durationSecs != null ? mmss(view.analysis.durationSecs * 1000) : "–" }}
        </span>
      </div>

      <div class="teams">
        <div v-for="team in teamOrder" :key="team" class="team" :style="{ borderTopColor: teamColor(team) }">
          <template v-for="t in view.analysis.teams.filter((x) => x.team === team)" :key="t.team">
            <div class="row">
              <strong>{{ teamLabel(team) }}</strong>
              <span v-if="t.won != null" :class="t.won ? 'ok' : 'hint'">{{ t.won ? "获胜" : "失败" }}</span>
              <span class="hint">{{ t.players }} 人 · 均分 {{ rating(t.avgRating) }}</span>
              <span class="points num">{{ t.points ?? "–" }}</span>
            </div>
            <dl class="figures">
              <div><dt>出兵价值</dt><dd class="num">{{ count(t.deployedValue) }}</dd></div>
              <div><dt>击毁价值</dt><dd class="num">{{ count(t.destructionScore) }}</dd></div>
              <div><dt>损失价值</dt><dd class="num">{{ count(t.lossesScore) }}</dd></div>
              <div><dt>击毁 / 损失</dt><dd class="num">{{ count(t.destruction) }} / {{ count(t.losses) }}</dd></div>
              <div><dt>占点</dt><dd class="num">{{ count(t.objectives) }}</dd></div>
              <div><dt>补给消耗</dt><dd class="num">{{ count(t.supplyConsumed) }}</dd></div>
            </dl>
          </template>
        </div>
      </div>

      <h3>累计出兵价值</h3>
      <EChart v-if="curveOption" :option="curveOption" :height="220" label="双方累计出兵价值随时间的变化" />

      <SortieExplorer :key="view.analysis.matchId" :analysis="view.analysis" :players="players" :teams="teamOrder" :team-label="teamLabel" :team-color="teamColor" @range="(start, end) => selectedRange = [start, end]" />

      <h3>兵种构成（估算价值）</h3>
      <EChart v-if="compositionOption" :option="compositionOption" :height="compositionHeight" label="每名玩家各兵种的估算出兵价值" />

      <h3>玩家</h3>
      <div class="table-scroll"><table>
        <thead>
          <tr>
            <th class="left">玩家</th>
            <th>ELO</th>
            <th>出兵价值</th>
            <th>击毁价值</th>
            <th>损失价值</th>
            <th>击毁 / 损失</th>
            <th>占点</th>
            <th>补给消耗</th>
            <th>出兵次数</th>
            <th>阵亡</th>
            <th>最后出兵</th>
          </tr>
        </thead>
        <tbody>
          <template v-for="p in players" :key="p.id">
            <tr class="player" :class="{ self: p.id === view.selfId, open: expanded === p.id }" @click="expanded = expanded === p.id ? null : p.id">
              <td class="left">
                <span class="swatch" :style="{ background: teamColor(p.team) }" />
                <a href="#" class="name" title="打开玩家档案" @click.prevent.stop="openPlayer(p.id)">{{ name(p) }}</a>
                <span v-if="p.deserter" class="flag" title="游戏记录的离场">离场</span>
              </td>
              <td class="num">
                {{ rating(p.oldRating) }}<span v-if="eloDelta(p)" class="delta" :class="eloDelta(p).startsWith('+') ? 'up' : 'down'"> {{ eloDelta(p) }}</span>
              </td>
              <td class="num strong">{{ count(p.deployedValue) }}</td>
              <td class="num">{{ count(p.destructionScore) }}</td>
              <td class="num">{{ count(p.lossesScore) }}</td>
              <td class="num">{{ count(p.destruction) }} / {{ count(p.losses) }}</td>
              <td class="num">{{ count(p.objectives) }}</td>
              <td class="num">{{ count(p.supplyConsumed) }}</td>
              <td class="num">{{ p.sorties }}<span v-if="p.refunded" class="hint">（回收 {{ p.refunded }}）</span></td>
              <td class="num">{{ p.dead }}</td>
              <td class="num">{{ p.lastSpawnSecs != null ? mmss(p.lastSpawnSecs * 1000) : "–" }}</td>
            </tr>
            <tr v-if="expanded === p.id" class="units-row">
              <td colspan="11">
                <table class="units">
                  <thead>
                    <tr>
                      <th class="left">单位</th>
                      <th class="left">兵种</th>
                      <th>次数</th>
                      <th>回收</th>
                      <th>阵亡</th>
                      <th>击毁</th>
                      <th>伤害</th>
                      <th>估算价值</th>
                      <th>首次出兵</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="u in p.units" :key="u.variantKey">
                      <td class="left">{{ u.unitName }}</td>
                      <td class="left hint">{{ unitClassNames[u.class] }}</td>
                      <td class="num">{{ u.count }}</td>
                      <td class="num">{{ u.refunded || "" }}</td>
                      <td class="num">{{ u.dead }}</td>
                      <td class="num">{{ u.kills }}</td>
                      <td class="num">{{ count(u.damage) }}</td>
                      <td class="num">{{ count(u.value) }}</td>
                      <td class="num">{{ u.firstSpawnSecs != null ? mmss(u.firstSpawnSecs * 1000) : "–" }}</td>
                    </tr>
                  </tbody>
                </table>
                <p v-if="p.transferredOut" class="hint">其中 {{ p.transferredOut }} 个单位转交给了队友。</p>
              </td>
            </tr>
          </template>
        </tbody>
      </table></div>

      <p class="hint foot">
        数据来自 BATrace 战报。队伍与玩家的合计是官方数据；单位与兵种的价值按单位目录的基础价格估算，
        并按每人官方的出兵价值等比例缩放，不含改装选项的差别。点玩家一行展开他的单位明细。
      </p>
      <p v-if="!view.analysis.catalogComplete" class="error">
        {{ view.catalogFetchedAt == null ? "还没有单位目录，" : "有单位不在单位目录里，" }}单位名称和估算价值可能不全。
      </p>
    </template>
    <template v-else-if="!loading">
      <h2>比赛 {{ id }}</h2>
      <p class="hint">{{ pendingText(row) }}</p>
      <button v-if="row?.report.state !== 'cancelled'" class="primary" :disabled="requesting" @click="requestNow">{{ requesting ? "正在请求…" : "立即获取战报" }}</button>
    </template>
    <p v-else class="hint">正在读取…</p>
  </section>
</template>

<style scoped>
.report {
  min-width: 0;
}
.table-scroll { overflow-x: auto; }
@media (max-width: 650px) {
  .report { padding: 12px; }
  .teams { grid-template-columns: 1fr !important; }
}

.head h2 {
  margin: 0;
}

.result {
  padding: 1px 10px;
  border-radius: 4px;
  font-weight: 600;
  font-size: 13px;
}

.result.win {
  background: rgb(95 211 141 / 0.15);
  color: var(--bax-good);
}

.result.loss {
  background: rgb(240 97 109 / 0.15);
  color: var(--bax-bad);
}

.teams {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  margin-top: 16px;
}

.team {
  padding: 10px 12px;
  border: 1px solid var(--bax-border);
  border-top: 2px solid;
  border-radius: 6px;
  background: var(--bax-surface-2);
}

.points {
  margin-left: auto;
  font-size: 18px;
  font-weight: 600;
}

.figures {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px 12px;
  margin: 8px 0 0;
}

.figures dt {
  font-size: 12px;
  color: var(--bax-muted);
}

.figures dd {
  margin: 0;
  font-weight: 600;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

th,
td {
  padding: 4px 8px;
  text-align: right;
  white-space: nowrap;
  border-bottom: 1px solid var(--bax-border);
}

th {
  font-weight: 500;
  font-size: 12px;
  color: var(--bax-muted);
}

.left {
  text-align: left;
}

tr.player {
  cursor: pointer;
}

tr.player:hover,
tr.player.open {
  background: var(--bax-surface-2);
}

tr.self .name {
  font-weight: 600;
}

.swatch {
  display: inline-block;
  width: 3px;
  height: 12px;
  margin-right: 6px;
  vertical-align: -1px;
  border-radius: 1px;
}

.name {
  color: inherit;
  text-decoration: none;
}

.name:hover {
  color: var(--bax-accent);
  text-decoration: underline;
}

.flag {
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 3px;
  font-size: 11px;
  background: var(--bax-surface-2);
  color: var(--bax-muted);
  border: 1px solid var(--bax-border);
}

.strong {
  font-weight: 600;
}

.delta {
  font-size: 12px;
}

.delta.up {
  color: var(--bax-good);
}

.delta.down {
  color: var(--bax-bad);
}

.units-row > td {
  padding: 6px 8px 12px 24px;
  background: var(--bax-surface-2);
}

.units th,
.units td {
  border-bottom-color: rgb(255 255 255 / 0.05);
}

.foot {
  margin-top: 16px;
}
</style>
