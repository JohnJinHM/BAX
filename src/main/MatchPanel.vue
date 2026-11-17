<script setup lang="ts">
import { computed, ref } from "vue";

import type { PlayerView } from "../shared/bindings/PlayerView";
import type { Team } from "../shared/bindings/Team";
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
  teamNames,
} from "../shared/format";
import { useNow } from "../shared/useNow";

const snapshot = useSnapshot();
const now = useNow(1000);
const current = computed(() => snapshot.value?.current ?? null);
const message = ref<string | null>(null);
const refreshing = ref(false);

const groups = computed(() => {
  const m = current.value;
  if (!m) return [];
  if (!m.rosterKnown) return [{ key: "waiting", title: "等待室（队伍未定）", players: m.players }];
  const order: Team[] = m.selfTeam === "bravo" ? ["bravo", "alpha", "spectators"] : ["alpha", "bravo", "spectators"];
  return order
    .map((team) => {
      const players = m.players.filter((p) => p.team === team);
      const summary = m.teams.find((t) => t.team === team);
      const mine = m.selfTeam === team ? "（本方）" : "";
      const avg = summary ? `  均分 ${rating(summary.averageRating)}（${summary.rated}/${summary.players} 人有分）` : "";
      return { key: team, title: `${teamNames[team]}${mine}${avg}`, players };
    })
    .filter((g) => g.players.length > 0);
});

const clockLine = computed(() => {
  const m = current.value;
  if (!m?.clock) return m?.rejoined ? "重连进入，没有计时基准，可以在“提醒与计时”里手动校准" : "";
  const elapsed = now.value - m.clock.startAt;
  const source = { logEstimate: "日志推算", manual: "手动校准", report: "战报" }[m.clock.source];
  if (m.phase === "ended") return `计时来源：${source}`;
  return `${elapsed < 0 ? `距开局 ${mmss(-elapsed)}` : `已开局 ${mmss(elapsed)}`}（${source}）`;
});

function encounters(p: PlayerView): string {
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
      <span class="hint">{{ clockLine }}</span>
      <button class="refresh" :disabled="refreshing" @click="refresh">刷新资料</button>
    </div>
    <p v-if="message" class="error">{{ message }}</p>

    <div v-for="group in groups" :key="group.key" class="group">
      <h3>{{ group.title }}</h3>
      <table>
        <colgroup>
          <col class="c-name" />
          <col v-for="n in 9" :key="n" />
          <col class="c-met" />
          <col class="c-fetched" />
        </colgroup>
        <thead>
          <tr>
            <th class="left">玩家</th>
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
          <tr v-for="p in group.players" :key="p.id" :class="{ self: p.isSelf }">
            <td class="left">
              <div class="name">{{ p.name ?? "（无名）" }}</div>
              <div class="hint">ID {{ p.id }}<template v-if="p.logName && p.logName !== p.name"> · {{ p.logName }}</template></div>
            </td>
            <td class="num strong">{{ rating(p.profile?.rating) }}</td>
            <td class="num">{{ p.profile?.level ?? "–" }}</td>
            <td class="num">{{ p.profile?.rank ?? "–" }}</td>
            <td class="num">{{ p.profile?.ratingGames ?? "–" }}</td>
            <td class="num">{{ percent(p.profile?.ranked?.winRate) }}</td>
            <td class="num">{{ p.profile?.ranked?.leaves ?? "–" }}</td>
            <td class="num">{{ ratio(p.profile?.ranked?.killLossRatio) }}</td>
            <td class="num">{{ ratio(p.profile?.ranked?.costRatio) }}</td>
            <td class="num">{{ minutes(p.profile?.ranked?.averageMinutes) }}</td>
            <td class="num">{{ encounters(p) }}</td>
            <td>
              <span :class="{ stale: p.profile && !p.profileCurrent }">{{ ago(p.profileFetchedAt, now) }}</span>
              <span v-if="fetchText(p.fetch)" class="state" :class="p.fetch.state"> · {{ fetchText(p.fetch) }}</span>
            </td>
          </tr>
        </tbody>
      </table>
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
  overflow: hidden;
  text-overflow: ellipsis;
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
