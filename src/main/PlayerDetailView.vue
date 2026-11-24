<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";

import type { PlayerDetail } from "../shared/bindings/PlayerDetail";
import type { Relation } from "../shared/bindings/Relation";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { ago, minutes, mmss, percent, phaseNames, rating, ratio } from "../shared/format";
import { useNow } from "../shared/useNow";
import { openMatch } from "./navigation";
import Sparkline from "./Sparkline.vue";
import SpecIcon from "../shared/SpecIcon.vue";

const props = defineProps<{ id: number }>();

const now = useNow(60_000);
const detail = ref<PlayerDetail | null>(null);
const loading = ref(false);
const missing = ref(false);
const failure = ref<string | null>(null);
const drafts = reactive(new Map<number, string>());
const pendingSaves = reactive(new Set<number>());
const noteDirty = computed(() => drafts.has(props.id));
const savingNote = computed(() => pendingSaves.has(props.id));
const noteMessage = ref<{ ok: boolean; text: string } | null>(null);
const noteDraft = computed({
  get: () => drafts.get(props.id) ?? detail.value?.note?.body ?? "",
  set(body: string) {
    drafts.set(props.id, body);
    noteMessage.value = null;
  },
});
const refreshing = ref(false);
const snapshot = useSnapshot();
let loadSequence = 0;

async function load() {
  const sequence = ++loadSequence;
  const requestedId = props.id;
  loading.value = true;
  failure.value = null;
  try {
    const result = await commands.playerDetail(requestedId);
    if (sequence !== loadSequence || requestedId !== props.id) return;
    detail.value = result;
    missing.value = result == null;
  } catch (error) {
    if (sequence === loadSequence) failure.value = errorText(error);
  } finally {
    if (sequence === loadSequence) loading.value = false;
  }
}

watch(
  () => props.id,
  () => {
    noteMessage.value = null;
    detail.value = null;
    void load();
  },
  { immediate: true },
);
watch(() => snapshot.value?.archive.seq, (next, previous) => {
  if (previous != null && next !== previous) void load();
});

const title = computed(() => {
  const d = detail.value;
  return d?.name ?? d?.logName ?? `ID ${props.id}`;
});

const historyPoints = computed(() =>
  (detail.value?.history ?? [])
    .filter((p) => p.rating != null)
    .map((p) => ({ at: p.at, value: p.rating as number })),
);
const maxSpecGames = computed(() => Math.max(1, ...(detail.value?.profile?.favoriteSpecs.map(s => s.games) ?? [])));

const relationNames: Record<Relation, string> = { teammate: "队友", opponent: "对手", other: "其他" };

function date(at: number): string {
  return new Date(at).toLocaleString("zh-CN", { hour12: false, month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

async function saveNote() {
  if (savingNote.value || !noteDirty.value) return;
  const id = props.id;
  const body = noteDraft.value;
  pendingSaves.add(id);
  try {
    await commands.setNote(id, body);
    if (props.id === id) await load();
    if (drafts.get(id) === body) {
      drafts.delete(id);
      if (props.id === id) noteMessage.value = { ok: true, text: body.trim() ? "已保存" : "已删除" };
    }
  } catch (error) {
    if (props.id === id) noteMessage.value = { ok: false, text: errorText(error) };
  } finally {
    pendingSaves.delete(id);
  }
}

async function refresh() {
  refreshing.value = true;
  failure.value = null;
  try {
    await commands.refreshPlayer(props.id);
    await load();
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    refreshing.value = false;
  }
}
</script>

<template>
  <section class="card detail">
    <div class="row head">
      <h2>{{ title }}</h2>
      <span v-if="detail?.isSelf" class="tag">本机</span>
      <span class="hint">ID {{ id }}</span>
      <span v-if="detail?.logName && detail.logName !== detail.name" class="hint">日志名 {{ detail.logName }}</span>
      <span v-if="detail?.steamId" class="hint">Steam {{ detail.steamId }}</span>
      <button class="refresh" :disabled="refreshing" @click="refresh">{{ refreshing ? "正在刷新…" : "刷新资料" }}</button>
    </div>
    <p v-if="failure" class="error">{{ failure }}</p>
    <p v-if="missing" class="hint">长期库里还没有这名玩家。可以点“刷新资料”从 BATrace 查一次。</p>

    <template v-if="detail">
      <div class="stats">
        <div><label>ELO</label><span class="num big">{{ rating(detail.profile?.rating) }}</span></div>
        <div><label>等级</label><span class="num">{{ detail.profile?.level ?? "–" }}</span></div>
        <div><label>排名</label><span class="num">{{ detail.profile?.rank ?? "–" }}</span></div>
        <div><label>排位场次</label><span class="num">{{ detail.profile?.ratingGames ?? "–" }}</span></div>
        <div><label>胜率</label><span class="num">{{ percent(detail.profile?.ranked?.winRate) }}</span></div>
        <div><label>离场</label><span class="num">{{ detail.profile?.ranked?.leaves ?? "–" }}</span></div>
        <div><label>损毁比</label><span class="num">{{ ratio(detail.profile?.ranked?.killLossRatio) }}</span></div>
        <div><label>价值比</label><span class="num">{{ ratio(detail.profile?.ranked?.costRatio) }}</span></div>
        <div><label>场均时长</label><span class="num">{{ minutes(detail.profile?.ranked?.averageMinutes) }}</span></div>
        <div><label>资料时间</label><span>{{ ago(detail.fetchedAt, now) }}</span></div>
      </div>

      <h3>专精使用</h3>
      <div v-if="detail.profile?.lastSpecs.length" class="last-specs">
        <span class="hint">上次专精</span>
        <span v-for="spec in detail.profile.lastSpecs" :key="spec.ids[0]"><SpecIcon :id="spec.ids[0]!" :label="spec.name" /> {{ spec.name }}</span>
      </div>
      <p v-if="detail.profile?.specSampleMatches" class="hint">最近已有 {{ detail.profile.specSampleMatches }} 场战报 · 已识别 {{ detail.profile.specIdentifiedMatches }} 场</p>
      <ul v-if="detail.profile?.favoriteSpecs.length" class="spec-chart" aria-label="专精使用场次">
        <li v-for="spec in detail.profile.favoriteSpecs" :key="spec.ids[0]" :aria-label="`${spec.name}：${spec.games} 场`" class="reading-row">
          <SpecIcon :id="spec.ids[0]!" :label="spec.name" />
          <span class="spec-name">{{ spec.name }}</span>
          <span class="spec-track" aria-hidden="true"><span :style="{ width: `${spec.games / maxSpecGames * 100}%` }" /></span>
          <span class="num spec-count">{{ spec.games }} 场</span>
        </li>
      </ul>
      <p v-else class="hint">暂无可识别的专精战报。</p>

      <h3>备注</h3>
      <textarea
        v-model="noteDraft"
        aria-label="玩家备注"
        rows="3"
        maxlength="2000"
        placeholder="只保存在本机，对局时显示在悬浮窗上"
      />
      <div class="row">
        <button class="primary" :disabled="!noteDirty || savingNote" @click="saveNote">{{ savingNote ? "保存中…" : "保存备注" }}</button>
        <button v-if="noteDirty" :disabled="savingNote" @click="drafts.delete(id); noteMessage = null">放弃修改</button>
        <span v-if="noteDirty" class="hint">未保存</span>
        <span class="hint num">{{ noteDraft.length }}/2000</span>
        <span v-if="detail.note && !noteDirty" class="hint">更新于 {{ ago(detail.note.updatedAt, now) }}</span>
        <span v-if="noteMessage" :class="noteMessage.ok ? 'ok' : 'error'">{{ noteMessage.text }}</span>
      </div>

      <h3>ELO 变化</h3>
      <Sparkline :points="historyPoints" />

      <h3>
        同场 {{ detail.encounters.total }} 场：队友 {{ detail.encounters.asTeammate }}，对手 {{ detail.encounters.asOpponent }}
      </h3>
      <table v-if="detail.matches.length">
        <thead>
          <tr>
            <th class="left">时间</th>
            <th class="left">地图</th>
            <th class="left">关系</th>
            <th class="left">当时的名字</th>
            <th>时长</th>
            <th>状态</th>
            <th>比赛</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="m in detail.matches" :key="m.matchId">
            <td class="left num">{{ date(m.at) }}</td>
            <td class="left">{{ m.map ?? "–" }}</td>
            <td class="left" :class="m.relation">{{ relationNames[m.relation] }}</td>
            <td class="left">{{ m.logName ?? "–" }}</td>
            <td class="num">{{ m.durationMs != null ? mmss(m.durationMs) : "–" }}</td>
            <td>{{ phaseNames[m.phase] }}</td>
            <td class="num"><a href="#" class="match" title="打开这场比赛" @click.prevent="openMatch(m.matchId)">{{ m.matchId }}</a></td>
          </tr>
        </tbody>
      </table>
      <p v-else class="hint">还没有同场记录。</p>
    </template>
    <p v-else-if="loading" class="hint">正在读取…</p>
  </section>
</template>

<style scoped>
.head h2 {
  margin: 0;
}

.last-specs { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 18px; }
.last-specs > span { display: inline-flex; align-items: center; gap: 6px; }
.spec-chart { list-style: none; padding: 0; margin: 12px 0 24px; max-width: 760px; }
.spec-chart li { display: grid; grid-template-columns: 28px minmax(100px, 1fr) minmax(60px, 2fr) 44px; align-items: center; gap: 10px; margin: 8px 0; }
.spec-name { overflow-wrap: anywhere; font-size: calc(13px * var(--bax-font-scale)); }
.spec-track { display: block; height: 8px; border-radius: min(3px, var(--bax-radius-xs)); background: var(--bax-surface-2); }
.spec-track > span { display: block; height: 100%; border-radius: inherit; background: var(--bax-accent); }
.spec-count { text-align: right; font-size: calc(12px * var(--bax-font-scale)); }

.tag {
  padding: 1px 8px;
  border-radius: var(--bax-radius-xs);
  background: color-mix(in srgb, var(--bax-accent) 15%, transparent);
  color: var(--bax-accent);
  font-size: calc(12px * var(--bax-font-scale));
}

.refresh {
  margin-left: auto;
}

.stats {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: calc(12px * var(--bax-density));
  margin-top: calc(16px * var(--bax-density));
}

.stats div {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stats label {
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-muted);
}

.big {
  font-size: calc(20px * var(--bax-font-scale));
  font-weight: 600;
  color: var(--bax-value);
}

textarea {
  width: 100%;
  padding: 8px;
  border: 1px solid var(--bax-border);
  border-radius: var(--bax-radius-sm);
  background: var(--bax-surface-2);
  color: inherit;
  font: inherit;
  resize: vertical;
}

textarea:focus {
  outline: none;
  border-color: var(--bax-accent);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: calc(13px * var(--bax-font-scale));
}

th,
td {
  padding: calc(4px * var(--bax-density)) 8px;
  text-align: right;
  white-space: nowrap;
  border-bottom: 1px solid var(--bax-border);
}

th {
  font-weight: 500;
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-muted);
}

.left {
  text-align: left;
}

a.match {
  color: var(--bax-muted);
}

a.match:hover {
  color: var(--bax-accent);
}

td.teammate {
  color: var(--bax-good);
}

td.opponent {
  color: var(--bax-bravo);
}
</style>
