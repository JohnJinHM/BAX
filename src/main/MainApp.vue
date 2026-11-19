<script setup lang="ts">
import { computed } from "vue";

import { useSnapshot } from "../shared/engine";
import { errorNames } from "../shared/format";
import HistoryPanel from "./HistoryPanel.vue";
import MatchPanel from "./MatchPanel.vue";
import { currentTab as tab } from "./navigation";
import PlayersPanel from "./PlayersPanel.vue";
import RemindersPanel from "./RemindersPanel.vue";
import SettingsPanel from "./SettingsPanel.vue";

const snapshot = useSnapshot();

const logLine = computed(() => {
  const s = snapshot.value;
  if (!s) return "正在连接…";
  if (s.log.searching) return "正在寻找游戏日志";
  if (s.log.error) return `日志读取失败：${s.log.error}`;
  if (!s.log.dir) return "没有找到游戏日志";
  return s.log.ready ? "日志已就绪" : "正在读取日志";
});

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
        <div :class="{ error: snapshot?.log.error }">{{ logLine }}</div>
        <div v-if="apiLine">{{ apiLine }}</div>
        <div v-if="snapshot?.archive.pending">{{ snapshot.archive.pending }} 场在等战报</div>
      </div>
    </nav>
    <main>
      <MatchPanel v-if="tab === 'match'" />
      <HistoryPanel v-else-if="tab === 'history'" />
      <PlayersPanel v-else-if="tab === 'players'" />
      <RemindersPanel v-else-if="tab === 'reminders'" />
      <SettingsPanel v-else />
    </main>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: 168px minmax(0, 1fr);
  height: 100vh;
}

nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 16px 10px;
  background: var(--bax-surface);
  border-right: 1px solid var(--bax-border);
}

.brand {
  padding: 0 8px 12px;
  font-size: 18px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--bax-accent);
}

nav button {
  text-align: left;
  border: none;
  background: transparent;
  color: var(--bax-muted);
}

nav button.active {
  background: var(--bax-surface-2);
  color: var(--bax-fg);
}

.status {
  margin-top: auto;
  padding: 0 8px;
  font-size: 12px;
  color: var(--bax-muted);
}

main {
  overflow: auto;
  padding: 20px 24px;
}

@media (max-width: 650px) {
  .shell { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
  nav { flex-direction: row; align-items: center; flex-wrap: wrap; padding: 8px; border-right: 0; border-bottom: 1px solid var(--bax-border); }
  .brand { padding: 0 8px; font-size: 16px; }
  nav button { padding: 5px 7px; font-size: 12px; }
  .status { display: none; }
  main { padding: 12px; }
}
</style>
