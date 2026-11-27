<script setup lang="ts">
import { computed } from "vue";

import { useAppStatus, useSnapshot } from "../shared/engine";
import { errorNames } from "../shared/format";
import { logStatus } from "../shared/match-presentation";
import { useSkin } from "../shared/skin";
import HistoryPanel from "./HistoryPanel.vue";
import MatchPanel from "./MatchPanel.vue";
import { currentTab as tab } from "./navigation";
import PlayersPanel from "./PlayersPanel.vue";
import RemindersPanel from "./RemindersPanel.vue";
import SettingsPanel from "./SettingsPanel.vue";

const snapshot = useSnapshot();
const appStatus = useAppStatus();
useSkin(snapshot);
const panels = { match: MatchPanel, history: HistoryPanel, players: PlayersPanel, reminders: RemindersPanel, settings: SettingsPanel };

const logLine = computed(() => logStatus(snapshot.value?.log).text);

const apiLine = computed(() => {
  const api = snapshot.value?.api;
  if (!api) return "";
  if (api.inFlight + api.queued > 0) return `正在查询 ${api.inFlight + api.queued} 人`;
  if (api.lastError) return `上次查询失败：${errorNames[api.lastError.kind]}`;
  return "";
});
</script>

<template>
  <div class="shell">
    <nav>
      <div class="brand">BAX</div>
      <button :class="{ active: tab === 'match' }" @click="tab = 'match'">当前对局</button>
      <button :class="{ active: tab === 'history' }" @click="tab = 'history'">比赛记录</button>
      <button :class="{ active: tab === 'players' }" @click="tab = 'players'">玩家</button>
      <button :class="{ active: tab === 'reminders' }" @click="tab = 'reminders'">提醒与计时</button>
      <button :class="{ active: tab === 'settings' }" @click="tab = 'settings'">设置</button>
      <div class="status">
        <button v-if="appStatus?.update?.version" @click="tab = 'settings'">新版本 {{ appStatus.update.version }}</button>
        <div :class="{ error: snapshot?.log.error }">{{ logLine }}</div>
        <div v-if="apiLine">{{ apiLine }}</div>
        <div v-if="snapshot?.archive.pending">{{ snapshot.archive.pending }} 场在等战报</div>
      </div>
    </nav>
    <main>
      <div class="panel-content">
        <KeepAlive include="PlayersPanel,RemindersPanel">
          <component :is="panels[tab]" />
        </KeepAlive>
      </div>
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: calc(168px * var(--bax-font-scale)) minmax(0, 1fr);
  height: 100vh;
}

nav {
  display: flex;
  flex-direction: column;
  gap: calc(4px * var(--bax-density));
  padding: calc(16px * var(--bax-density)) 10px;
  background: var(--bax-nav-bg);
  border-right: 1px solid var(--bax-border);
}

.brand {
  padding: 0 8px 12px;
  font-size: calc(18px * var(--bax-font-scale));
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--bax-brand);
}

nav button {
  text-align: left;
  border: none;
  background: transparent;
  color: var(--bax-muted);
}

nav button.active {
  background: var(--bax-nav-active-bg);
  box-shadow: inset 2px 0 0 var(--bax-nav-active-edge);
  color: var(--bax-fg);
}

.status {
  margin-top: auto;
  padding: 0 8px;
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-muted);
}

main {
  overflow: auto;
}

.panel-content {
  padding: calc(20px * var(--bax-density)) calc(24px * var(--bax-density));
}

@media (max-width: 650px) {
  .shell { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
  nav { flex-direction: row; align-items: center; flex-wrap: wrap; padding: 8px; border-right: 0; border-bottom: 1px solid var(--bax-border); }
  .brand { padding: 0 8px; font-size: calc(16px * var(--bax-font-scale)); }
  nav button { padding: 5px 7px; font-size: calc(12px * var(--bax-font-scale)); }
  nav button.active { box-shadow: inset 0 -2px 0 var(--bax-nav-active-edge); }
  .status { display: none; }
  .panel-content { padding: calc(12px * var(--bax-density)); }
}
</style>
