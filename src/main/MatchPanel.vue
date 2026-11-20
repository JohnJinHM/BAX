<script setup lang="ts">
import { rosterGroups, type RosterRow } from "../shared/roster";
import { computed, ref } from "vue";

import { commands, errorText, useSnapshot } from "../shared/engine";
import {
  ago,
  fetchText,
  minutes,
  mmss,
  percent,
  phaseNames,
  rating,
  ratio,
} from "../shared/format";
import { useNow } from "../shared/useNow";
import { openMatch, openPlayer } from "./navigation";
import SpecIcon from "../shared/SpecIcon.vue";
import { useCurrentReport } from "../shared/useCurrentReport";
import { matchSpecs, rosterSummary } from "../shared/match-presentation";

const snapshot = useSnapshot();
const now = useNow(1000);
const current = computed(() => snapshot.value?.current ?? null);
const { report, failure: reportFailure } = useCurrentReport(snapshot);
const specsFor = (p: RosterRow) => matchSpecs(p, current.value, report.value);
const message = ref<string | null>(null);
const refreshing = ref(false);

const groups = computed(() => rosterGroups(current.value).map(g => ({
  ...g, title: g.key === 'spectators' ? `旁观者 · ${g.count} 人` : `${g.title} · ${g.count} 人　均分 ${rating(g.averageValue)}（${g.rated} 人有分）`,
})));

const clockLine = computed(() => {
  const m = current.value;
  if (!m?.clock) return m?.rejoined ? "重连进入，没有计时基准，可以在“提醒与计时”里手动校准" : "";
  if (m.phase === "ended") return "";
  const elapsed = now.value - m.clock.startAt;
  return elapsed < 0 ? `距开局 ${mmss(-elapsed)}` : `已开局 ${mmss(elapsed)}`;
});

function encounters(p: RosterRow): string {
  const e = p.encounters;
  return e.total === 0 ? "–" : `${e.total}（友 ${e.asTeammate} / 敌 ${e.asOpponent}）`;
}

async function refresh() {
  message.value = null;
  refreshing.value = true;
  try {
    await commands.refreshPlayers();
  } catch (error) {
    message.value = errorText(error);
  } finally {
    refreshing.value = false;
  }
}
</script>

<template>
  <section v-if="current" class="card">
    <div class="row head">
      <h2>比赛 {{ current.matchId }}</h2>
      <span class="tag">{{ phaseNames[current.phase] }}</span>
      <span v-if="current.rejoined" class="tag">重连</span>
      <span v-if="current.connection !== 'connected'" class="tag warn">
        {{ current.connection === "reconnecting" ? "重连中" : "已断开" }}
      </span>
      <span class="hint">{{ current.map ?? "地图未知" }}</span>
      <span class="hint">{{ rosterSummary(current, report) }}</span>
      <span class="hint">{{ clockLine }}</span>
      <button class="refresh" :disabled="refreshing" @click="refresh">刷新资料</button>
    </div>
    <p v-if="message || reportFailure" class="error">{{ message || reportFailure }}</p>
    <p v-if="current.phase === 'ended'" class="hint">
      <template v-if="report || snapshot?.archive.lastArchived === current.matchId">
        战报已归档：<a href="#" @click.prevent="openMatch(current.matchId)">查看战后分析</a>
      </template>
      <template v-else>比赛已结束。BATrace 收录战报后会自动归档，通常要几分钟：<a href="#" @click.prevent="openMatch(current.matchId)">比赛记录</a></template>
    </p>

    <p v-if="!current.rosterKnown" class="hint" role="status">{{ current.mode === 'pve' ? 'PvE · OCR 已暂停，等待日志名单' : current.pregame.players.length ? current.pregame.status : "等待玩家名单" }}</p>
    <div v-for="group in groups" :key="group.key" class="group">
      <h3>{{ group.title }}</h3>
      <div class="roster-table"><table>
        <colgroup>
          <col class="c-name" />
          <col class="c-specs" />
          <col v-for="n in 9" :key="n" />
          <col class="c-met" />
          <col class="c-fetched" />
        </colgroup>
        <thead>
          <tr>
            <th class="left">玩家</th>
            <th>{{ current.phase === 'ended' ? '本局专精' : '上场专精' }}</th>
            <th>ELO</th>
            <th>等级</th>
            <th>排名</th>
            <th>排位场次</th>
            <th>胜率</th>
            <th>离场</th>
            <th>损毁比</th>
            <th>价值比</th>
            <th>场均时长</th>
            <th>以前同场</th>
            <th>资料</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in group.players" :key="p.key" :class="{ self: p.isSelf }">
            <td class="left">
              <a v-if="p.id != null && !p.provisional" class="name" href="#" title="打开玩家档案" @click.prevent="openPlayer(p.id)">{{ p.name }}</a>
              <span v-else class="name" title="赛前识别，等待日志确认">{{ p.name }}</span>
              <div v-if="p.logName && p.logName !== p.name" class="hint">{{ p.logName }}</div>
              <div v-if="p.note" class="note" :title="p.note">✎ {{ p.note }}</div>
            </td>
            <td><span class="specs"><SpecIcon v-for="spec in specsFor(p)" :key="spec.ids[0]" :id="spec.ids[0]!" :label="spec.name" /><span v-if="!specsFor(p).length" class="hint" :title="current.phase === 'ended' && p.team !== 'spectators' ? (report ? '本场战报尚不能确定专精组合' : '等待本场战报') : ''">–</span></span></td>
            <td class="num strong">{{ rating(p.profile?.rating) }}</td>
            <td class="num">{{ p.displayLevel ?? "–" }}</td>
            <td class="num">{{ p.profile?.rank ?? "–" }}</td>
            <td class="num">{{ p.profile?.ratingGames ?? "–" }}</td>
            <td class="num">{{ percent(p.profile?.ranked?.winRate) }}</td>
            <td class="num">{{ p.profile?.ranked?.leaves ?? "–" }}</td>
            <td class="num">{{ ratio(p.profile?.ranked?.killLossRatio) }}</td>
            <td class="num">{{ ratio(p.profile?.ranked?.costRatio) }}</td>
            <td class="num">{{ minutes(p.profile?.ranked?.averageMinutes) }}</td>
            <td class="num">{{ encounters(p) }}</td>
            <td>
              <span :class="{ stale: p.profile && !p.profileCurrent }">{{ p.provisional ? "待日志确认" : ago(p.profileFetchedAt, now) }}</span>
              <span v-if="fetchText(p.fetch)" class="state" :class="p.fetch.state"> · {{ fetchText(p.fetch) }}</span>
            </td>
          </tr>
        </tbody>
      </table></div>
    </div>
    <p class="hint">
      损毁比与价值比是排位累计的击毁 ÷ 损失（数量与价值）。灰色的资料时间表示资料不是这场比赛开始之后取得的。
    </p>
  </section>
  <section v-else class="card">
    <h2>还没有比赛</h2>
    <p class="hint">打开游戏进入对局后，这里会显示双方玩家。</p>
  </section>
</template>

<style scoped>
.head h2 {
  margin: 0;
}
.roster-table { overflow-x: auto; scrollbar-color: var(--bax-faint) var(--bax-surface); }
.roster-table table { min-width: 1160px; }
.c-specs { width: 82px; }
.specs { display: flex; align-items: center; justify-content: center; gap: 6px; }
.hint a { color: var(--bax-accent); text-underline-offset: 3px; }

.tag {
  padding: 1px 8px;
  border-radius: 4px;
  background: var(--bax-surface-2);
  font-size: 12px;
}

.tag.warn {
  color: var(--bax-warn);
}

.refresh {
  margin-left: auto;
}

/* 固定列宽：两队的表格上下对齐 */
table {
  width: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: 13px;
}

.c-name {
  width: 22%;
}

.c-met {
  width: 150px;
}

.c-fetched {
  width: 130px;
}

td.left .name {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  color: inherit;
  text-decoration: none;
}

td.left .name:hover {
  color: var(--bax-accent);
  text-decoration: underline;
}

.note {
  font-size: 12px;
  color: var(--bax-warn);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

th,
td {
  padding: 5px 8px;
  text-align: right;
  white-space: nowrap;
  border-bottom: 1px solid var(--bax-border);
}

th {
  font-weight: 500;
  color: var(--bax-muted);
  font-size: 12px;
}

.left {
  text-align: left;
}

td.left {
  white-space: normal;
}

tr.self {
  background: rgb(76 194 255 / 0.08);
}

.strong {
  font-weight: 600;
}

.stale {
  color: var(--bax-faint);
}

.state.failed {
  color: var(--bax-bad);
}

.state.fetching,
.state.queued {
  color: var(--bax-muted);
}
</style>
