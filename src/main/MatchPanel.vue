<script setup lang="ts">
import { rosterGroups, type RosterRow } from "../shared/roster";
import { computed, reactive, ref, watch } from "vue";

import { commands, errorText, useSnapshot } from "../shared/engine";
import {
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
import PlayerBadges from "../shared/PlayerBadges.vue";
import { useCurrentReport } from "../shared/useCurrentReport";
import { matchSpecs, predictionHint, rosterSummary, rosterWaitingText } from "../shared/match-presentation";
import LogDirectory from "./LogDirectory.vue";

const snapshot = useSnapshot();
const now = useNow(1000);
const current = computed(() => snapshot.value?.current ?? null);
const { report, failure: reportFailure } = useCurrentReport(snapshot);
const specsFor = (p: RosterRow) => matchSpecs(p, current.value, report.value);
const message = ref<string | null>(null);
const refreshing = ref(false);
const expandedPlayers = reactive(new Set<string>());
watch(() => current.value?.matchId, () => expandedPlayers.clear());

function toggleStats(key: string) {
  if (expandedPlayers.has(key)) expandedPlayers.delete(key);
  else expandedPlayers.add(key);
}

const groups = computed(() => rosterGroups(current.value).map(g => ({
  ...g,
  specHeading: current.value?.phase === 'ended' ? (g.key === 'spectators' ? null : '本局专精')
    : g.tone === 'enemy' ? '专精预测' : null,
  title: g.key === 'spectators' ? `旁观者 · ${g.count} 人` : `${g.title} · ${g.count} 人　均分 ${rating(g.averageValue)}（${g.rated} 人有分）`,
})));

const clockLine = computed(() => {
  const m = current.value;
  if (!m?.clock) return m?.rejoined ? "重连进入，没有计时基准，可以在“提醒与计时”里手动校准" : "";
  if (m.phase === "ended") return "";
  const elapsed = now.value - m.clock.startAt;
  return elapsed < 0 ? `距开局 ${mmss(-elapsed)}` : `已开局 ${mmss(elapsed)}`;
});

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

    <p v-if="!current.rosterKnown" class="hint" role="status">{{ rosterWaitingText(current) }}</p>
    <div v-for="group in groups" :key="group.key" class="group">
      <h3>{{ group.title }}</h3>
      <div class="roster-table"><table>
        <colgroup>
          <col />
          <col class="c-specs" />
          <col class="c-elo" />
          <col class="c-level" />
          <col class="c-rate" />
          <col class="c-met" />
          <col class="c-action" />
        </colgroup>
        <thead>
          <tr>
            <th class="left" :colspan="group.specHeading ? 1 : 2">玩家</th>
            <th v-if="group.specHeading" class="spec-heading">{{ group.specHeading }}</th>
            <th>ELO</th>
            <th>等级</th>
            <th>胜率</th>
            <th>以前同场</th>
            <th>排位统计</th>
          </tr>
        </thead>
        <tbody>
          <template v-for="p in group.players" :key="p.key">
            <tr :class="{ self: p.isSelf }">
              <td class="left" :colspan="group.specHeading ? 1 : 2">
                <div class="name-line">
                  <a v-if="p.id != null && !p.provisional" class="name" href="#" title="打开玩家档案" @click.prevent="openPlayer(p.id)">{{ p.name }}</a>
                  <span v-else class="name" title="赛前识别，等待日志确认">{{ p.name }}</span>
                  <PlayerBadges :row="p" />
                </div>
                <div v-if="p.logName && p.logName !== p.name" class="hint">{{ p.logName }}</div>
                <div v-if="p.note" class="note" :title="p.note">✎ {{ p.note }}</div>
              </td>
              <td v-if="group.specHeading"><span class="specs" :title="current.phase === 'ended' ? undefined : predictionHint(p)"><SpecIcon v-for="spec in specsFor(p)" :key="spec.ids[0]" :id="spec.ids[0]!" :label="spec.name" /><span v-if="!specsFor(p).length" class="hint" :title="current.phase === 'ended' && p.team !== 'spectators' ? (report ? '本场战报尚不能确定专精组合' : '等待本场战报') : ''">–</span></span></td>
              <td class="num strong">{{ rating(p.profile?.rating) }}</td>
              <td class="num">{{ p.displayLevel ?? '–' }}</td>
              <td class="num">{{ percent(p.profile?.ranked?.winRate) }}</td>
              <td class="num">
                {{ p.provisional ? '待确认' : `${p.encounters.total} 次` }}
                <div v-if="p.encounters.total" class="hint">友 {{ p.encounters.asTeammate }} / 敌 {{ p.encounters.asOpponent }}</div>
              </td>
              <td>
                <button class="stats-toggle" :aria-label="`${expandedPlayers.has(p.key) ? '收起' : '展开'} ${p.name} 的排位统计`"
                  :aria-expanded="expandedPlayers.has(p.key)" :aria-controls="expandedPlayers.has(p.key) ? `stats-${p.key}` : undefined"
                  @click="toggleStats(p.key)">{{ expandedPlayers.has(p.key) ? '收起' : '展开' }}</button>
              </td>
            </tr>
            <tr v-if="expandedPlayers.has(p.key)" :id="`stats-${p.key}`">
              <td colspan="7" class="details">
                <dl>
                  <div><dt>排名</dt><dd>{{ p.profile?.rank ?? '–' }}</dd></div>
                  <div><dt>排位场次</dt><dd>{{ p.profile?.ratingGames ?? '–' }}</dd></div>
                  <div><dt>离场</dt><dd>{{ p.profile?.ranked?.leaves ?? '–' }}</dd></div>
                  <div><dt>损毁比</dt><dd>{{ ratio(p.profile?.ranked?.killLossRatio) }}</dd></div>
                  <div><dt>价值比</dt><dd>{{ ratio(p.profile?.ranked?.costRatio) }}</dd></div>
                  <div><dt>场均时长</dt><dd>{{ minutes(p.profile?.ranked?.averageMinutes) }}</dd></div>
                </dl>
              </td>
            </tr>
          </template>
        </tbody>
      </table></div>
    </div>
    <p v-if="expandedPlayers.size" class="hint">损毁比与价值比均为排位累计击毁 ÷ 损失，分别按数量和价值计算。</p>
  </section>
  <section v-else class="card">
    <h2>当前对局</h2>
    <LogDirectory />
  </section>
</template>

<style scoped>
.head h2 {
  margin: 0;
}
.roster-table { overflow-x: auto; scrollbar-color: var(--bax-faint) var(--bax-surface); }
.c-elo, .c-rate { width: 4.8em; }
.c-level { width: 3.8em; }
.c-specs { width: 16%; }
.c-action { width: 5.4em; }
.spec-heading { text-align: center; }
.specs { display: flex; align-items: center; justify-content: center; gap: 4px; flex-wrap: wrap; }
.stats-toggle { padding: 3px 6px; }
.details { text-align: left; background: var(--bax-surface-2); }
.details dl { display: grid; grid-template-columns: repeat(auto-fit, minmax(7em, 1fr)); gap: 8px; margin: 4px 0 8px; }
.details dt { color: var(--bax-muted); }
.details dd { margin: 2px 0; font-variant-numeric: tabular-nums; }
.hint a { color: var(--bax-accent); text-underline-offset: 3px; }

.tag {
  padding: 1px 8px;
  border-radius: var(--bax-radius-xs);
  background: var(--bax-surface-2);
  font-size: calc(12px * var(--bax-font-scale));
}

.tag.warn {
  color: var(--bax-warn);
}

.refresh {
  margin-left: auto;
}

/* 为数值列保留宽度，在窄窗口内横向滚动。 */
table {
  width: 100%;
  min-width: 38em;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: calc(13px * var(--bax-font-scale));
}

.c-met {
  width: 17%;
}

.name-line { display: flex; flex-wrap: wrap; align-items: center; gap: 2px 5px; }
td.left .name {
  min-width: 0;
  white-space: nowrap;
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
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-warn);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

th,
td {
  padding: calc(5px * var(--bax-density)) 8px;
  text-align: right;
  overflow-wrap: anywhere;
  border-bottom: 1px solid var(--bax-border);
}

th {
  font-weight: 500;
  color: var(--bax-muted);
  font-size: calc(12px * var(--bax-font-scale));
}

.left {
  text-align: left;
}

td.left {
  white-space: normal;
}

tr.self {
  background: var(--bax-self-bg);
}

tr.self > td:first-child {
  box-shadow: inset 2px 0 0 var(--bax-self-edge);
}

.strong {
  font-weight: 600;
}

</style>
