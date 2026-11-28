<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";

import type { PlayerDetail } from "../shared/bindings/PlayerDetail";
import type { PlayerSearchHit } from "../shared/bindings/PlayerSearchHit";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { ago, minutes, percent, rating, ratio } from "../shared/format";
import { useNow } from "../shared/useNow";
import type { PlayerFight } from "../shared/bindings/PlayerFight";
import { playerAverages, combatAxes, preferenceAxes } from "./player-analysis";
import { syncFights } from "./player-fight-sync";
import PlayerRadar from "./PlayerRadar.vue";
import PlayerUnitPreferences from "./PlayerUnitPreferences.vue";
import PlayerFightArchive from "./PlayerFightArchive.vue";
import PlayerReportPreview from "./PlayerReportPreview.vue";
import { playerLabels } from "../shared/roster";
import PlayerBadges from "../shared/PlayerBadges.vue";
import { countries } from "../../crates/bax-engine/src/spec-units.json";
import Sparkline from "./Sparkline.vue";
import SpecIcon from "../shared/SpecIcon.vue";

const props = defineProps<{ id: number; searchHit?: PlayerSearchHit | null }>();
const emit = defineEmits<{ profileSaved: [] }>();

const archiveReady = ref(false);
const hasSyncedFights = ref(false);
const fights = ref<PlayerFight[]>([]);
const averages = computed(() => playerAverages(fights.value));
const now = useNow(60_000);
const detail = ref<PlayerDetail | null>(null);
const loading = ref(false);
const missing = ref(false);
const failure = ref<string | null>(null);
const drafts = reactive(new Map<number, string>());
const pendingSaves = reactive(new Set<number>());
const savingFavorites = reactive(new Set<number>());
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
const preview = ref<{id:number; map:string | null} | null>(null);
const refreshingPlayers = reactive(new Set<number>());
const refreshing = computed(() => refreshingPlayers.has(props.id));
const snapshot = useSnapshot();
let loadSequence = 0;

async function load() {
  const sequence = ++loadSequence;
  const requestedId = props.id;
  loading.value = true;
  failure.value = null;
  try {
    const result = await commands.playerDetail(requestedId);
    if (sequence === loadSequence && requestedId === props.id) {
      detail.value = result;
      missing.value = result == null;
    }
  } catch (error) {
    if (sequence === loadSequence) failure.value = errorText(error);
  } finally {
    if (sequence === loadSequence) loading.value = false;
  }
}

watch(() => props.id, () => {
  noteMessage.value = null;
  detail.value = null;
  missing.value = false;
  fights.value = [];
  archiveReady.value = false;
  hasSyncedFights.value = false;
  preview.value = null;
  void load();
}, { immediate: true });

watch(() => props.searchHit, async hit => {
  if (!hit) return;
  try {
    await commands.saveBatracePlayer(hit);
  } catch (error) {
    if (props.id === hit.id) failure.value = `保存玩家失败：${errorText(error)}`;
    return;
  }
  emit("profileSaved");
  if (props.id === hit.id) void load();
  await refresh(hit.id);
}, { immediate: true });
watch(() => snapshot.value?.archive.seq, (next, previous) => {
  if (previous != null && next !== previous) void load();
});
watch(() => {
  const match = snapshot.value?.current;
  return match?.players?.find(p => p.id === props.id)?.traceCount
    ?? match?.pregame?.players.find(p => !p.ambiguous && p.candidates[0]?.id === props.id)?.traceCount;
}, count => {
  if (count != null && count !== detail.value?.traceCount) void load();
});

const title = computed(() => {
  const d = detail.value;
  return d?.name ?? d?.logName ?? props.searchHit?.name ?? `ID ${props.id}`;
});

const historyPoints = computed(() =>
  (detail.value?.history ?? [])
    .filter((p) => p.rating != null)
    .map((p) => ({ at: p.at, value: p.rating as number })),
);
const specCountry: Record<string, number> = countries;
const factions = [{ id: 2, name: "美军" }, { id: 1, name: "俄军" }];
const factionFavorites = computed(() => factions.map(faction => {
  const usage = detail.value?.profile?.favoriteSpecPairs ?? [];
  const pairs = usage.filter(pair => pair.ids.length === 2 && pair.ids.every(id => specCountry[id] === faction.id));
  const top = pairs.filter(pair => pair.games === pairs[0]?.games);
  return { ...faction, top };
}));
const maxSpecGames = computed(() => Math.max(1, ...(detail.value?.profile?.favoriteSpecs.map(s => s.games) ?? [])));

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

async function toggleFavorite() {
  const id = props.id;
  if (!detail.value || savingFavorites.has(id)) return;
  savingFavorites.add(id);
  try {
    await commands.setPlayerFavorite(id, !detail.value.isFavorite);
    emit("profileSaved");
    if (props.id === id) await load();
  } catch (error) {
    if (props.id === id) failure.value = errorText(error);
  } finally {
    savingFavorites.delete(id);
  }
}

async function refresh(id: number) {
  if (refreshingPlayers.has(id)) return;
  refreshingPlayers.add(id);
  if (props.id === id) failure.value = null;
  try {
    // A failed profile lookup must still allow the archive request to finish.
    const profileRequest = commands.refreshPlayer(id).then(async () => {
      emit("profileSaved");
      if (props.id === id) await load();
    });
    const [profile, archive] = await Promise.allSettled([profileRequest, syncFights(id)]);
    if (props.id === id) {
      await load();
      if (props.id === id) {
        const errors = [
          profile.status === "rejected" ? "资料刷新失败："+errorText(profile.reason) : "",
          archive.status === "rejected" ? "对局刷新失败："+errorText(archive.reason) : "",
        ].filter(Boolean);
        if (errors.length) failure.value = errors.join("；");
      }
    }
  } finally {
    refreshingPlayers.delete(id);
  }
}
</script>

<template>
  <section class="card detail">
    <div class="row head">
      <div class="identity">
        <h2>{{ title }}</h2>
        <button v-if="detail" type="button" class="favorite" :aria-pressed="detail.isFavorite"
          :disabled="savingFavorites.has(id)" aria-label="收藏玩家" :title="detail.isFavorite ? '取消收藏' : '收藏玩家'" @click="toggleFavorite">
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path d="m12 3 2.8 5.7 6.3.9-4.5 4.4 1.1 6.2-5.7-3-5.7 3 1.1-6.2-4.5-4.4 6.3-.9Z" />
          </svg>
        </button>
      </div>
      <PlayerBadges v-if="detail" :row="{ profile: detail.profile, traceCount: detail.traceCount, labels: playerLabels(detail.id, snapshot?.current?.labels) }" />
      <span v-if="detail?.isSelf" class="tag">本机</span>
      <span class="hint">ID {{ id }}</span>
      <span v-if="detail?.logName && detail.logName !== detail.name" class="hint">日志名 {{ detail.logName }}</span>
      <span v-if="detail?.steamId || searchHit?.steam_id" class="hint">Steam64 {{ detail?.steamId ?? searchHit?.steam_id }}</span>
      <div class="refresh">
        <small v-if="detail?.firstSeenAt != null && archiveReady && !hasSyncedFights && !refreshing" class="hint first-sync">
          尚未查询近期战局，当前统计仅基于本地战报。
        </small>
        <button title="刷新玩家资料和最近 20 场对局" :disabled="refreshing" @click="refresh(id)">{{ refreshing ? "正在刷新…" : "刷新资料" }}</button>
      </div>
    </div>
    <p v-if="failure" class="error">{{ failure }}</p>
    <p v-if="missing && refreshing" class="hint" role="status">正在获取玩家资料…</p>
    <p v-else-if="missing" class="hint">本地还没有这名玩家的资料。点击“刷新资料”可从 STB 获取。</p>

    <div v-if="detail" class="overview">
    <div class="stats">
      <div><label>ELO</label><span class="num big">{{ rating(detail.profile?.rating) }}</span></div>
      <div><label>等级</label><span class="num">{{ detail.profile?.level ?? "–" }}</span></div>
      <div v-if="detail.traceCount != null && detail.traceCount > 0" :title="detail.traceCheckedAt != null ? `标记查询时间 ${date(detail.traceCheckedAt)}` : undefined"><label>BATrace 标记</label><span class="num">{{ detail.traceCount }} 次</span></div>
      <div><label>排名</label><span class="num">{{ detail.profile?.rank ?? "–" }}</span></div>
      <div><label>排位场次</label><span class="num">{{ detail.profile?.ratingGames ?? "–" }}</span></div>
      <div><label>胜率</label><span class="num">{{ percent(detail.profile?.ranked?.winRate) }}</span></div>
      <div><label>离场</label><span class="num">{{ detail.profile?.ranked?.leaves ?? "–" }}</span></div>
      <div><label>损毁比</label><span class="num">{{ ratio(detail.profile?.ranked?.killLossRatio) }}</span></div>
      <div><label>价值比</label><span class="num">{{ ratio(detail.profile?.ranked?.costRatio) }}</span></div>
      <div><label>场均时长</label><span class="num">{{ minutes(detail.profile?.ranked?.averageMinutes) }}</span></div>
      <div><label>资料时间</label><span>{{ ago(detail.fetchedAt, now) }}</span></div>
    </div>
    <div class="radars">
      <PlayerRadar title="作战效能" :axes="combatAxes" :values="averages.combat" :games="averages.games" :efficiency-games="averages.efficiencyGames" />
      <PlayerRadar title="单位偏好" :axes="preferenceAxes" :values="averages.preferences" :games="averages.preferenceGames" />
      <p v-if="archiveReady && Math.min(averages.efficiencyGames, averages.preferenceGames) < 5" class="sample-warning" role="status">部分统计的有效对局不足 5 场，作战效能与单位偏好可能不准确。可点击「刷新资料」补充近期战报。</p>
    </div>
    </div>
    <p v-else-if="loading" class="hint">正在读取…</p>

    <template v-if="detail">
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

      <h3>专精使用</h3>
      <div class="faction-specs">
        <section v-for="faction in factionFavorites" :key="faction.id">
          <h4>{{ faction.name }}常用专精</h4>
          <div v-for="pair in faction.top" :key="pair.ids.join('-')" class="favorite-pair">
            <span class="pair-icons"><SpecIcon v-for="id in pair.ids" :key="id" :id="id" :label="pair.name" /></span>
            <span>{{ pair.name }}</span><small>{{ pair.games }} 场</small>
          </div>
          <small v-if="!faction.top.length" class="hint">暂无已识别组合</small>
        </section>
      </div>
      <p v-if="detail.profile?.specSampleMatches" class="hint">最近 {{ detail.profile.specSampleMatches }} 场多人战报 · 已识别 {{ detail.profile.specIdentifiedMatches }} 场</p>
      <ul v-if="detail.profile?.favoriteSpecs.length" class="spec-chart" aria-label="专精使用场次">
        <li v-for="spec in detail.profile.favoriteSpecs" :key="spec.ids[0]" :aria-label="`${spec.name}：${spec.games} 场`" class="reading-row">
          <SpecIcon :id="spec.ids[0]!" :label="spec.name" />
          <span class="spec-name">{{ spec.name }}</span>
          <span class="spec-track" aria-hidden="true"><span :style="{ width: `${spec.games / maxSpecGames * 100}%` }" /></span>
          <span class="num spec-count">{{ spec.games }} 场</span>
        </li>
      </ul>
      <p v-else class="hint">暂无可识别的多人战报。</p>

      <PlayerUnitPreferences :fights="fights" />

      <h3>ELO 变化</h3>
      <Sparkline :points="historyPoints" />

    </template>

    <PlayerFightArchive v-model:fights="fights" v-model:ready="archiveReady" v-model:has-synced="hasSyncedFights" :id="id" :encounters="detail?.isSelf ? [] : (detail?.matches ?? [])" @preview="preview = $event" />
    <PlayerReportPreview v-if="preview" :match-id="preview.id" :map="preview.map" :player-id="id" :player-name="title" @close="preview = null" />

  </section>
</template>

<style scoped>
.head h2 {
  margin: 0;
}

.faction-specs {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,200px),1fr));gap:12px;}
.faction-specs h4 {margin:0 0 5px;font-size:inherit;}
.favorite-pair {display:flex;align-items:center;gap:6px;margin:4px 0;}
.pair-icons {display:flex;flex:none;}
.favorite-pair small {white-space:nowrap;color:var(--bax-muted);}
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

.identity { display: inline-flex; align-items: center; gap: 3px; min-width: 0; }
.identity h2 { overflow-wrap: anywhere; }
.favorite { display: grid; place-items: center; flex: none; width: 28px; height: 28px; padding: 0; background: transparent; border: 0; color: var(--bax-muted); }
.favorite path { fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linejoin: round; }
.favorite:hover, .favorite[aria-pressed="true"] { color: var(--bax-accent); }
.favorite[aria-pressed="true"] path { fill: currentColor; }
.favorite:focus-visible { outline: 2px solid var(--bax-accent); }
.refresh { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-left: auto; }
.first-sync { color: var(--bax-warn); }

.detail {container-type:inline-size;}
.overview {display:grid;grid-template-columns:minmax(185px,1fr) minmax(0,2fr);align-items:start;gap:16px;margin:20px 0 8px;}
.radars {min-width:0;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px 16px;}
.sample-warning {grid-column:1/-1;text-align:center;margin:0;font-size:calc(12px * var(--bax-font-scale));line-height:1.45;color:#f4a261;}
@container (max-width:760px) {.overview {grid-template-columns:minmax(0,1fr);}.overview .stats {grid-column:1/-1;grid-template-columns:repeat(auto-fit,minmax(84px,1fr));}}
@container (max-width:470px) {.radars {grid-template-columns:minmax(0,1fr);}.overview .radar {width:100%;}}
.stats {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: calc(12px * var(--bax-density));
  margin-top: 0;
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

</style>
