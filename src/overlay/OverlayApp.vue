<script setup lang="ts">
import { rosterGroups, type RosterRow } from "../shared/roster";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import { commands, errorText, useAppStatus, useSnapshot } from "../shared/engine";
import { fetchText, mmss, percent, phaseNames, rating } from "../shared/format";
import { saveSetting, useSetting } from "../shared/settings";
import { useSkin } from "../shared/skin";
import { useNow } from "../shared/useNow";
import { useCurrentReport } from "../shared/useCurrentReport";
import { logStatus, matchSpecs, predictionHint, rosterSummary, rosterWaitingText, showOverlaySpecs } from "../shared/match-presentation";
import AnimatedNumber from "../shared/AnimatedNumber.vue";
import SpecIcon from "../shared/SpecIcon.vue";
import PlayerBadges from "../shared/PlayerBadges.vue";
import { OverlayAutomation } from "../shared/overlay-automation";

const snapshot = useSnapshot();
const status = useAppStatus();
useSkin(snapshot);
const appearance = useSetting(snapshot, "ui.overlay");
const now = useNow(250);

const current = computed(() => snapshot.value?.current ?? null);
const locked = computed(() => status.value?.overlayLocked ?? false);
const failure = ref<string | null>(null);
const expanded = ref(appearance.value.expanded);
const automaticMotion = ref(false);
function setExpanded(value: boolean, automatic = false) {
  if (expanded.value === value) return;
  automaticMotion.value = automatic;
  expanded.value = value;
}
const automation = new OverlayAutomation();
const { report: result, failure: reportFailure } = useCurrentReport(snapshot);
const specsVisible = computed(() => showOverlaySpecs(current.value));
const specsFor = (p: RosterRow) => matchSpecs(p, current.value, result.value);
const ratingResults = computed(() => new Map(result.value?.analysis.players.map(p => [p.id, p]) ?? []));
const selfResult = computed(() => ratingResults.value.get(result.value?.selfId ?? snapshot.value?.selfId ?? -1));
const selfDelta = computed(() => {
  const p = selfResult.value;
  return p?.newRating != null && p.oldRating != null ? p.newRating - p.oldRating : null;
});
function deltaFor(id: number | null) {
  const p = id == null ? null : ratingResults.value.get(id);
  return p?.newRating != null && p.oldRating != null ? p.newRating - p.oldRating : null;
}
watch(() => appearance.value.expanded, value => setExpanded(value));
watch([current, appearance, now], () => {
  const next = automation.update(current.value, appearance.value, now.value);
  if (next != null) setExpanded(next, true);
}, { immediate: true });

// ---------------------------------------------------------------- 计时

const elapsed = computed(() => {
  const clock = current.value?.clock;
  return clock ? now.value - clock.startAt : null;
});

const timer = computed(() => {
  const m = current.value;
  if (!m) return "";
  if (m.phase === "ended") return "已结束";
  if (elapsed.value == null) return m.rejoined ? "未校准" : "--:--";
  return elapsed.value < 0 ? `准备 ${mmss(-elapsed.value)}` : mmss(elapsed.value);
});

/** 下一条还没到点的提醒。 */
const nextReminder = computed(() => {
  const list = snapshot.value?.reminders ?? [];
  const pending = list
    .filter((r) => r.enabled && r.outcome == null && r.fireAt != null && r.fireAt > now.value)
    .sort((a, b) => (a.fireAt ?? 0) - (b.fireAt ?? 0));
  const first = pending[0];
  return first && first.fireAt != null ? { label: first.label, inMs: first.fireAt - now.value } : null;
});

/** 15 秒内刚触发的提醒。 */
const activeAlert = computed(() => {
  const alert = snapshot.value?.alert;
  if (!alert || alert.matchId !== current.value?.matchId) return null;
  return now.value - alert.at < 15_000 ? alert : null;
});

// ---------------------------------------------------------------- 队伍

function beforeRating(p: RosterRow): number | null {
  return (p.id != null ? ratingResults.value.get(p.id)?.oldRating : null) ?? p.profile?.rating ?? null;
}
const teams = computed(() => rosterGroups(current.value)
  .map(g => {
    const values = g.players.flatMap(p => beforeRating(p) != null ? [beforeRating(p)!] : []);
    return { ...g, rated: values.length, averageValue: values.length ? values.reduce((a, b) => a + b, 0) / values.length : null };
  }));

/**
 * 收起时的双色计量条：按 ELO 的期望得分公式，由两队均分差算出左边一队的胜率，
 * E = 1 / (1 + 10^((右 − 左) / 400))。均分只算有 ELO 的玩家。
 */
const meter = computed(() => {
  const [left, right] = teams.value.filter(g => g.key !== "spectators");
  if (!left || !right) return null;
  const a = left.averageValue;
  const b = right.averageValue;
  if (a == null || b == null) return { left, right, share: 0.5, known: false, title: "" };
  const share = 1 / (1 + 10 ** ((b - a) / 400));
  const diff = Math.round(a - b);
  const title = [
    `${left.title}胜率 ${percent(share)}，${right.title}胜率 ${percent(1 - share)}（按 ELO 均分差估算）`,
    `均分差 ${diff > 0 ? "+" : ""}${diff}`,
    `有 ELO 的玩家：${left.title} ${left.rated}/${left.count}，${right.title} ${right.rated}/${right.count}`,
  ].join("\n");
  return { left, right, share, known: true, title };
});

const emptyText = computed(() => {
  const log = snapshot.value?.log;
  const state = logStatus(log);
  if (state.needsDirectory) return `${state.text} 可在主窗口选择目录。`;
  return log?.ready ? "日志已就绪，等待进入对局。" : state.text;
});

function playerTitle(p: RosterRow): string {
  const parts: string[] = p.provisional ? ["赛前识别，等待日志确认"] : [];
  if (p.note) parts.push(`备注：${p.note}`);
  if (p.logName && p.logName !== p.name) parts.push(`日志名 ${p.logName}`);
  const e = p.encounters;
  if (e.total > 0) parts.push(`以前同场 ${e.total} 次（队友 ${e.asTeammate}，对手 ${e.asOpponent}）`);
  if (!p.provisional && p.profile && !p.profileCurrent) parts.push("资料不是这场比赛取得的");
  const state = fetchText(p.fetch);
  if (state) parts.push(state);
  const result = p.id != null ? ratingResults.value.get(p.id) : null;
  if (result?.oldRating != null && result.newRating != null) parts.push(`赛前 ELO ${rating(result.oldRating)} → 赛后 ${rating(result.newRating)}`);
  return parts.join("\n");
}

// ---------------------------------------------------------------- 操作

async function run(action: () => Promise<void>) {
  failure.value = null;
  try {
    await action();
  } catch (error) {
    failure.value = errorText(error);
  }
}

const toggleExpanded = () => {
  setExpanded(!expanded.value);
  return run(() => saveSetting(appearance.value, "ui.overlay", { expanded: expanded.value }));
};

// ---------------------------------------------------------------- 锁定后锁按钮仍可点

// 锁定时窗口整体鼠标穿透。外壳轮询鼠标位置，鼠标落在这里报告的锁按钮上时暂时不穿透，
// 并用事件告诉页面显示悬停样式：恢复穿透后页面收不到鼠标离开，`:hover` 会卡住。
const lockButton = ref<HTMLElement | null>(null);
const lockHot = ref(false);
let lastHotspot = "";

function reportHotspot() {
  const rect = lockButton.value?.getBoundingClientRect();
  const scale = window.devicePixelRatio || 1;
  const hotspot = rect
    ? { x: rect.left * scale, y: rect.top * scale, width: rect.width * scale, height: rect.height * scale }
    : null;
  const key = JSON.stringify(hotspot);
  if (key === lastHotspot) return;
  lastHotspot = key;
  commands.setOverlayHotspot(hotspot).catch(() => {
    lastHotspot = "";
  });
}

watch(locked, async () => {
  lockHot.value = false;
  await nextTick();
  reportHotspot();
});

// ---------------------------------------------------------------- 窗口大小跟随内容

const panel = ref<HTMLElement | null>(null);
let observer: ResizeObserver | null = null;
let lastSize = "";
let layoutFrame = 0;

function fitWindow() {
  const el = panel.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const width = Math.ceil(rect.width);
  const height = Math.ceil(rect.height);
  const key = `${width}x${height}`;
  if (key === lastSize || width === 0 || height === 0) return;
  lastSize = key;
  // 由外壳改大小：开启吸附边缘时要和位置一起算（贴着下边展开时保持贴边）
  void commands.fitOverlay(width, height).catch(() => { lastSize = ""; });
}

function relayout() {
  fitWindow();
  reportHotspot();
}

let unlistenHover: (() => void) | null = null;

onMounted(async () => {
  observer = new ResizeObserver(() => { cancelAnimationFrame(layoutFrame); layoutFrame = requestAnimationFrame(relayout); });
  if (panel.value) observer.observe(panel.value);
  // 换到缩放比例不同的显示器时 devicePixelRatio 会变
  window.addEventListener("resize", reportHotspot);
  unlistenHover = await getCurrentWebviewWindow().listen<boolean>("bax://overlay-lock-hover", (event) => {
    lockHot.value = event.payload;
  });
});
onUnmounted(() => {
  observer?.disconnect();
  cancelAnimationFrame(layoutFrame);
  window.removeEventListener("resize", reportHotspot);
  unlistenHover?.();
});
watch(appearance, () => requestAnimationFrame(relayout));
</script>

<template>
  <div
    ref="panel"
    class="panel"
    :class="{ unlocked: !locked, alerting: activeAlert, 'automatic-motion': automaticMotion, 'no-motion': !appearance.animations }"
    :style="{
      '--bax-opacity': appearance.opacity,
      '--bax-fade-position': `${appearance.fadePosition * 100}%`,
      '--bax-edge-opacity': appearance.opacity * appearance.fadeEndOpacity,
      zoom: appearance.scale,
    }"
  >
    <header :data-tauri-drag-region="locked ? undefined : ''">
      <span class="phase" :class="current?.phase" :data-tauri-drag-region="locked ? undefined : ''">
        {{ current ? phaseNames[current.phase] : "BAX" }}
      </span>
      <span class="map" :data-tauri-drag-region="locked ? undefined : ''">{{ current?.map ?? "" }}</span>
      <span class="timer num" :class="{ countdown: (elapsed ?? 0) < 0 }">{{ timer }}</span>
      <span class="tools">
        <template v-if="!locked">
          <button :title="expanded ? '收起' : '展开'" :aria-expanded="expanded" @click="toggleExpanded">
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path class="expand-chevron" :class="{ expanded }" d="M3.5 6L8 10.5L12.5 6" />
            </svg>
          </button>
          <button title="打开主窗口" @click="run(commands.openMainWindow)">
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4.5h10M3 8h10M3 11.5h10" /></svg>
          </button>
        </template>
        <button
          ref="lockButton"
          :class="{ hot: lockHot }"
          :title="locked ? '解锁：可以拖动、点击' : '锁定：鼠标穿透到游戏'"
          @click="run(() => commands.setOverlayLocked(!locked))"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true">
            <rect x="3.5" y="7.25" width="9" height="6.25" rx="1.5" />
            <path :d="locked ? 'M5.5 7.25V5.5a2.5 2.5 0 0 1 5 0v1.75' : 'M5.5 7.25V5.5a2.5 2.5 0 0 1 4.85-.85'" />
          </svg>
        </button>
        <button v-if="!locked" title="隐藏" @click="run(() => commands.setOverlayVisible(false))">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" /></svg>
        </button>
      </span>
    </header>
    <div v-if="rosterSummary(current, result)" class="match-context">{{ rosterSummary(current, result) }}</div>

    <div v-if="activeAlert" class="alert">⏰ {{ activeAlert.label }}</div>
    <div v-else-if="nextReminder && current?.phase !== 'ended'" class="next">
      {{ nextReminder.label }} · 还有 <span class="num">{{ mmss(nextReminder.inMs) }}</span>
    </div>

    <template v-if="current">
      <div v-if="meter" class="reveal meter-reveal" :class="{ visible: !expanded }" :aria-hidden="expanded" :inert="expanded"><div class="reveal-content">
      <section class="meter" :title="meter.title">
        <div class="meter-head">
          <span class="side" :class="meter.left.tone">
            <span class="side-title">{{ meter.left.title }}</span>
            <AnimatedNumber class="side-avg" :value="meter.left.averageValue" :enabled="appearance.animations" />
          </span>
          <span v-if="meter.known" class="odds num">
            <span :class="meter.left.tone">{{ percent(meter.share) }}</span>
            <span class="odds-label">胜率</span>
            <span :class="meter.right.tone">{{ percent(1 - meter.share) }}</span>
          </span>
          <span v-else class="odds odds-label">等待资料</span>
          <span class="side" :class="meter.right.tone">
            <AnimatedNumber class="side-avg" :value="meter.right.averageValue" :enabled="appearance.animations" />
            <span class="side-title">{{ meter.right.title }}</span>
          </span>
        </div>
        <div class="bar" :class="{ unknown: !meter.known }">
          <span class="fill" :class="meter.left.tone" :style="{ width: `${meter.share * 100}%` }" />
          <span class="fill" :class="meter.right.tone" />
        </div>
      </section>
      </div></div>
      <div class="reveal roster-reveal" :class="{ visible: expanded }" :aria-hidden="!expanded" :inert="!expanded"><div class="reveal-content">
      <div v-if="current.phase === 'ended'" class="result-summary" role="status">
        <template v-if="selfDelta != null"><span>本场 ELO</span><AnimatedNumber :key="current.matchId" class="result-delta" :class="{ gain: selfDelta > 0, loss: selfDelta < 0 }" :value="selfDelta" signed :enabled="appearance.animations" /><span class="result-total">{{ rating(selfResult?.oldRating) }} → {{ rating(selfResult?.newRating) }}</span></template>
        <span v-else class="next">{{ result ? '本场没有 ELO 变化数据' : '比赛已结束 · 等待加分数据' }}</span>
      </div>
      <section
        v-for="block in teams"
        :key="block.key"
        class="team"
        :class="block.tone"
      >
        <div class="team-head">
          <span class="team-title">{{ block.title }}</span>
          <span v-if="block.key !== 'spectators'" class="team-avg num">{{ current.phase === 'ended' ? '赛前均分' : '均分' }} <AnimatedNumber :value="block.averageValue" :enabled="appearance.animations" /></span>
          <span class="team-count num">{{ block.count }} 人</span>
        </div>
        <ul>
          <li
            v-for="p in block.players"
            :key="p.key"
            :class="{ self: p.isSelf, stale: !p.provisional && p.profile && !p.profileCurrent }"
            :title="playerTitle(p)"
          >
            <span class="elo num">{{ rating(beforeRating(p)) }}</span>
            <AnimatedNumber v-if="deltaFor(p.id) != null" class="rating-delta" :class="{ gain: deltaFor(p.id)! > 0, loss: deltaFor(p.id)! < 0 }" :value="deltaFor(p.id)" signed :enabled="appearance.animations" />
            <span v-else class="lv num">{{ p.displayLevel != null ? `Lv${p.displayLevel}` : "" }}</span>
            <span class="wr num">{{ percent(p.profile?.ranked?.winRate) }}</span>
            <span class="met num">{{ p.encounters.total > 0 ? `遇${p.encounters.total}` : "" }}</span>
            <span class="name">
              <span class="name-line">
                <a v-if="p.id != null" class="nickname" :href="`https://dash.batrace.top/player/${p.id}`" target="_blank" rel="noopener noreferrer" title="在浏览器中打开 BATrace 玩家页面" @click.prevent="run(() => commands.openBatracePlayer(p.id!))">{{ p.name }}</a>
                <span v-else class="nickname">{{ p.name }}</span>
                <PlayerBadges :row="p" />
                <span v-if="p.note" class="note"> · {{ p.note }}</span>
              </span>
              <span class="spec-reveal" :class="{ visible: specsVisible && specsFor(p).length }" :aria-hidden="!specsVisible || !specsFor(p).length">
                <span class="spec-hint" :title="current.phase === 'ended' ? specsFor(p).map(s => s.name).join(' + ') : predictionHint(p)"><span>{{ current.phase === 'ended' ? '本局专精' : '专精预测' }}</span><SpecIcon v-for="spec in specsFor(p).slice(0, 2)" :key="spec.ids[0]" :id="spec.ids[0]!" :label="spec.name" /></span>
              </span>
            </span>
            <span
              v-if="p.fetch.state !== 'idle'"
              class="fetch"
              :class="p.fetch.state"
              :title="fetchText(p.fetch)"
            >
              {{ p.fetch.state === "failed" ? "!" : "…" }}
            </span>
          </li>
        </ul>
      </section>
      </div></div>
      <p v-if="!current.rosterKnown" class="next" role="status">{{ rosterWaitingText(current) }}</p>
    </template>
    <div v-else class="empty">{{ emptyText }}</div>

    <div v-if="failure || reportFailure" class="failure">{{ failure || reportFailure }}</div>
  </div>
</template>

<style scoped>
.match-context { margin-top: 4px; color: var(--bax-muted); font-size: 11px; }
.spec-reveal { display: grid; grid-template-rows: 0fr; opacity: 0; transition: grid-template-rows 220ms cubic-bezier(.16, 1, .3, 1), opacity 160ms ease-out; }
.spec-reveal.visible { grid-template-rows: 1fr; opacity: 1; }
.spec-hint { display: flex; align-items: center; min-height: 0; overflow: hidden; gap: 4px; color: var(--bax-muted); font-size: 10px; }
.spec-hint .spec-icon { width: 22px; height: 22px; }
.panel {
  --reveal-duration: 240ms;
  --reveal-easing: cubic-bezier(.16, 1, .3, 1);
  width: 340px;
  padding: 8px 10px 10px;
  border-radius: var(--bax-radius);
  background: linear-gradient(to right, rgb(var(--bax-bg-rgb) / var(--bax-edge-opacity)) 0%, rgb(var(--bax-bg-rgb) / var(--bax-opacity)) var(--bax-fade-position));
  /* 渐变铺满边框所在的一圈；默认从内边距开始，透明边框下会平铺出另一端的颜色 */
  background-origin: border-box;
  border: var(--bax-overlay-border);
  font-size: 13px;
  line-height: 1.35;
}
.panel.automatic-motion { --reveal-duration: 720ms; }
.reveal { display: grid; grid-template-rows: 0fr; opacity: 0; transition: grid-template-rows var(--reveal-duration) var(--reveal-easing), opacity var(--reveal-duration) var(--reveal-easing); }
.reveal.visible { grid-template-rows: 1fr; opacity: 1; }
.reveal-content { min-height: 0; overflow: hidden; }
.expand-chevron { transform-origin: center; transition: transform var(--reveal-duration) var(--reveal-easing); }
.expand-chevron.expanded { transform: rotate(180deg); }
.result-summary { display: flex; align-items: baseline; gap: 10px; padding-top: 8px; }
.result-delta { font-size: 23px; font-weight: 600; animation: result-arrive 450ms cubic-bezier(.16, 1, .3, 1); }
.result-total { font-size: 11px; color: var(--bax-muted); }
.gain { color: var(--bax-good); }
.loss { color: var(--bax-bad); }
.rating-delta { text-align: right; font-size: 11px; font-weight: 600; }
@keyframes result-arrive { from { transform: translateY(5px); opacity: .4; } to { transform: translateY(0); opacity: 1; } }
.no-motion *, .no-motion { animation: none !important; transition: none !important; }
@media (prefers-reduced-motion: reduce) { .panel, .panel * { animation: none !important; transition-duration: 0s !important; } }

.panel.unlocked {
  border: var(--bax-overlay-unlocked-border);
}

.panel.alerting {
  animation: flash 1s ease-in-out 5;
}

@keyframes flash {
  50% {
    box-shadow: inset 0 0 0 2px var(--bax-warn);
  }
}

header {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 24px;
}

.phase {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: var(--bax-radius-xs);
  background: var(--bax-surface-2);
  color: var(--bax-muted);
  white-space: nowrap;
}

.phase.inBattle {
  color: var(--bax-good);
}

.phase.deployment {
  color: var(--bax-warn);
}

.map {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--bax-muted);
}

.timer {
  font-size: 18px;
  font-weight: 600;
  color: var(--bax-overlay-value);
}

.timer.countdown {
  color: var(--bax-warn);
  font-size: 15px;
}

.tools {
  display: flex;
  gap: 2px;
}

.tools button {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: var(--bax-radius-xs);
  background: transparent;
  color: var(--bax-muted);
  cursor: pointer;
}

.tools svg {
  width: 16px;
  height: 16px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

/* 锁定时页面收不到鼠标离开，悬停样式由外壳的事件驱动（.hot），不用 :hover */
.panel.unlocked .tools button:hover,
.tools button.hot {
  background: var(--bax-surface-2);
  color: var(--bax-fg);
}

/* ---------------------------------------------------------------- 颜色 */

.mine {
  --tone: var(--bax-friend);
}

.enemy {
  --tone: var(--bax-foe);
}

.alpha {
  --tone: var(--bax-alpha);
}

.bravo {
  --tone: var(--bax-bravo);
}

/* ---------------------------------------------------------------- 收起时的计量条 */

.meter {
  margin-top: 8px;
}

.meter-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.side {
  display: flex;
  align-items: baseline;
  gap: 6px;
}

.side-title {
  font-weight: 600;
  color: var(--tone);
}

.side-avg {
  font-size: 15px;
  font-weight: 600;
  color: var(--bax-overlay-value);
}

.odds {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-weight: 600;
}

.odds .mine,
.odds .enemy,
.odds .alpha,
.odds .bravo {
  color: var(--tone);
}

.odds-label {
  font-size: 11px;
  font-weight: 400;
  color: var(--bax-faint);
}

.bar {
  display: flex;
  gap: 2px;
  height: 6px;
  margin-top: 4px;
}

.bar .fill {
  border-radius: min(3px, var(--bax-radius-xs));
  background: var(--tone);
  transition: width 420ms var(--reveal-easing);
}

.bar .fill:last-child {
  flex: 1;
}

.bar.unknown .fill {
  background: var(--bax-surface-2);
}

.alert {
  margin-top: 6px;
  padding: 4px 8px;
  border-radius: var(--bax-radius-xs);
  background: color-mix(in srgb, var(--bax-warn) 18%, transparent);
  color: var(--bax-warn);
  font-weight: 600;
}

.next {
  margin-top: 4px;
  font-size: 11px;
  color: var(--bax-faint);
}

.team {
  margin-top: 8px;
}

.team-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding-bottom: 2px;
  border-bottom: 1px solid var(--bax-border);
}

.team-title {
  font-weight: 600;
  color: var(--tone, var(--bax-fg));
}

.team-avg {
  margin-left: auto;
  color: var(--bax-fg);
}

.team-count {
  color: var(--bax-faint);
  font-size: 11px;
}

ul {
  list-style: none;
  margin: 2px 0 0;
  padding: 0;
}

li {
  display: grid;
  grid-template-columns: 40px 38px 34px 32px minmax(0, 1fr) 12px;
  align-items: center;
  gap: 4px;
  padding: 1px 2px;
  border-radius: var(--bax-radius-xs);
}

li.self {
  background: var(--bax-self-bg);
  box-shadow: inset 2px 0 0 var(--bax-self-edge);
}
.panel.unlocked li:hover, .panel.unlocked li:focus-within { background: var(--bax-row-hover); }

.elo {
  font-weight: 600;
  text-align: right;
}

.elo, .side-avg, .team-avg .num, .rating-delta, .result-delta, .result-total {
  text-shadow: 0 1px 2px rgb(0 0 0 / 95%), 0 0 3px rgb(0 0 0 / 75%);
}

li.stale .elo {
  color: var(--bax-muted);
}

.lv,
.wr {
  color: var(--bax-muted);
  font-size: 11px;
  text-align: right;
}

.met {
  font-size: 11px;
  color: var(--bax-accent);
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.name-line { display: flex; align-items: baseline; }
.nickname, .note { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.note {
  color: var(--bax-warn);
  font-size: 11px;
}

.name a { color: inherit; text-decoration: underline; text-decoration-color: var(--bax-faint); text-underline-offset: 3px; }
.name a:hover { color: var(--bax-accent); text-decoration-color: currentColor; }
.name a:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 2px; border-radius: min(2px, var(--bax-radius-xs)); }

.fetch {
  color: var(--bax-faint);
  font-size: 11px;
}

.fetch.failed {
  color: var(--bax-bad);
  font-weight: 700;
}

.empty {
  margin-top: 6px;
  color: var(--bax-muted);
}

.failure {
  margin-top: 6px;
  font-size: 11px;
  color: var(--bax-bad);
}
</style>
