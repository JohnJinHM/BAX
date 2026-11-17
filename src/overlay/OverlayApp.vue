<script setup lang="ts">
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

import type { PlayerView } from "../shared/bindings/PlayerView";
import type { Team } from "../shared/bindings/Team";
import { commands, errorText, useAppStatus, useSnapshot } from "../shared/engine";
import { fetchText, mmss, percent, phaseNames, rating, teamNames } from "../shared/format";
import { saveSetting, useSetting } from "../shared/settings";
import { useNow } from "../shared/useNow";

const snapshot = useSnapshot();
const status = useAppStatus();
const appearance = useSetting(snapshot, "ui.overlay");
const now = useNow(250);

const current = computed(() => snapshot.value?.current ?? null);
const locked = computed(() => status.value?.overlayLocked ?? false);
const failure = ref<string | null>(null);

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

interface TeamBlock {
  team: Team;
  title: string;
  average: string;
  rated: number;
  count: number;
  players: PlayerView[];
}

const teams = computed<TeamBlock[]>(() => {
  const m = current.value;
  if (!m || !m.rosterKnown) return [];
  const mine = m.selfTeam === "alpha" || m.selfTeam === "bravo" ? m.selfTeam : null;
  const order: Team[] = mine === "bravo" ? ["bravo", "alpha"] : ["alpha", "bravo"];
  return order.map((team) => {
    const summary = m.teams.find((t) => t.team === team);
    return {
      team,
      title: mine ? (team === mine ? "我方" : "敌方") : teamNames[team],
      average: rating(summary?.averageRating),
      rated: summary?.rated ?? 0,
      count: summary?.players ?? 0,
      players: m.players.filter((p) => p.team === team),
    };
  });
});

const waiting = computed(() => {
  const m = current.value;
  return m && !m.rosterKnown ? m.players : [];
});

const emptyText = computed(() => {
  const s = snapshot.value;
  if (!s) return "正在连接…";
  if (s.log.searching) return "正在寻找游戏日志…";
  if (s.log.error) return `日志读取失败：${s.log.error}`;
  if (!s.log.dir) return "没有找到游戏日志，请在主窗口设置";
  if (!s.log.ready) return "正在读取日志…";
  return "等待对局…";
});

function playerTitle(p: PlayerView): string {
  const parts = [`ID ${p.id}`];
  if (p.logName && p.logName !== p.name) parts.push(`日志名 ${p.logName}`);
  const e = p.encounters;
  if (e.total > 0) parts.push(`以前同场 ${e.total} 次（队友 ${e.asTeammate}，对手 ${e.asOpponent}）`);
  if (p.profile && !p.profileCurrent) parts.push("资料不是这场比赛取得的");
  const state = fetchText(p.fetch);
  if (state) parts.push(state);
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

const toggleExpanded = () =>
  run(() => saveSetting(appearance.value, "ui.overlay", { expanded: !appearance.value.expanded }));

// ---------------------------------------------------------------- 窗口大小跟随内容

const panel = ref<HTMLElement | null>(null);
let observer: ResizeObserver | null = null;
let lastSize = "";

function fitWindow() {
  const el = panel.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const width = Math.ceil(rect.width);
  const height = Math.ceil(rect.height);
  const key = `${width}x${height}`;
  if (key === lastSize || width === 0 || height === 0) return;
  lastSize = key;
  void getCurrentWindow().setSize(new LogicalSize(width, height));
}

onMounted(() => {
  observer = new ResizeObserver(fitWindow);
  if (panel.value) observer.observe(panel.value);
});
onUnmounted(() => observer?.disconnect());
watch(appearance, () => requestAnimationFrame(fitWindow));
</script>

<template>
  <div
    ref="panel"
    class="panel"
    :class="{ unlocked: !locked, alerting: activeAlert }"
    :style="{ '--bax-opacity': appearance.opacity, zoom: appearance.scale }"
  >
    <header :data-tauri-drag-region="locked ? undefined : ''">
      <span class="phase" :class="current?.phase" :data-tauri-drag-region="locked ? undefined : ''">
        {{ current ? phaseNames[current.phase] : "BAX" }}
      </span>
      <span class="map" :data-tauri-drag-region="locked ? undefined : ''">{{ current?.map ?? "" }}</span>
      <span class="timer num" :class="{ countdown: (elapsed ?? 0) < 0 }">{{ timer }}</span>
      <span v-if="!locked" class="tools">
        <button :title="appearance.expanded ? '收起' : '展开'" @click="toggleExpanded">
          {{ appearance.expanded ? "▴" : "▾" }}
        </button>
        <button title="打开主窗口" @click="run(commands.openMainWindow)">☰</button>
        <button title="锁定：鼠标穿透到游戏" @click="run(() => commands.setOverlayLocked(true))">🔒</button>
        <button title="隐藏" @click="run(() => commands.setOverlayVisible(false))">✕</button>
      </span>
    </header>

    <div v-if="activeAlert" class="alert">⏰ {{ activeAlert.label }}</div>
    <div v-else-if="nextReminder && current?.phase !== 'ended'" class="next">
      {{ nextReminder.label }} · 还有 <span class="num">{{ mmss(nextReminder.inMs) }}</span>
    </div>

    <template v-if="current">
      <section v-for="block in teams" :key="block.team" class="team" :class="block.team">
        <div class="team-head">
          <span class="team-title">{{ block.title }}</span>
          <span class="team-avg num">均分 {{ block.average }}</span>
          <span class="team-count num">{{ block.rated }}/{{ block.count }}</span>
        </div>
        <ul v-if="appearance.expanded">
          <li
            v-for="p in block.players"
            :key="p.id"
            :class="{ self: p.isSelf, stale: p.profile && !p.profileCurrent }"
            :title="playerTitle(p)"
          >
            <span class="elo num">{{ rating(p.profile?.rating) }}</span>
            <span class="lv num">{{ p.profile?.level != null ? `Lv${p.profile.level}` : "" }}</span>
            <span class="wr num">{{ percent(p.profile?.ranked?.winRate) }}</span>
            <span class="met num">{{ p.encounters.total > 0 ? `遇${p.encounters.total}` : "" }}</span>
            <span class="name">{{ p.name ?? `ID ${p.id}` }}</span>
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
      <section v-if="waiting.length && appearance.expanded" class="team waiting">
        <div class="team-head">
          <span class="team-title">等待室</span>
          <span class="team-count num">{{ waiting.length }} 人</span>
        </div>
        <ul>
          <li v-for="p in waiting" :key="p.id" :title="playerTitle(p)">
            <span class="elo num">{{ rating(p.profile?.rating) }}</span>
            <span class="lv num">{{ p.profile?.level != null ? `Lv${p.profile.level}` : "" }}</span>
            <span class="wr num">{{ percent(p.profile?.ranked?.winRate) }}</span>
            <span class="met num">{{ p.encounters.total > 0 ? `遇${p.encounters.total}` : "" }}</span>
            <span class="name">{{ p.name ?? `ID ${p.id}` }}</span>
          </li>
        </ul>
      </section>
    </template>
    <div v-else class="empty">{{ emptyText }}</div>

    <div v-if="failure" class="failure">{{ failure }}</div>
  </div>
</template>

<style scoped>
.panel {
  width: 340px;
  padding: 8px 10px 10px;
  border-radius: var(--bax-radius);
  background: rgb(var(--bax-bg-rgb) / var(--bax-opacity));
  border: 1px solid var(--bax-border);
  font-size: 13px;
  line-height: 1.35;
}

.panel.unlocked {
  border: 1px dashed rgb(76 194 255 / 0.6);
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
  border-radius: 4px;
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
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--bax-muted);
  cursor: pointer;
  font-size: 12px;
}

.tools button:hover {
  background: var(--bax-surface-2);
  color: var(--bax-fg);
}

.alert {
  margin-top: 6px;
  padding: 4px 8px;
  border-radius: 4px;
  background: rgb(242 193 78 / 0.18);
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
}

.team.alpha .team-title {
  color: var(--bax-alpha);
}

.team.bravo .team-title {
  color: var(--bax-bravo);
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
  border-radius: 4px;
}

li.self {
  background: rgb(76 194 255 / 0.1);
}

.elo {
  font-weight: 600;
  text-align: right;
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
