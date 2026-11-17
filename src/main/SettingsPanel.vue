<script setup lang="ts">
import { ref, watch } from "vue";

import { commands, errorText, useAppStatus, useSnapshot } from "../shared/engine";
import { saveSetting, useSetting } from "../shared/settings";

const snapshot = useSnapshot();
const status = useAppStatus();
const appearance = useSetting(snapshot, "ui.overlay");
const alerts = useSetting(snapshot, "ui.alerts");
const hotkeys = useSetting(snapshot, "ui.hotkeys");
const message = ref<{ ok: boolean; text: string } | null>(null);

async function run(action: () => Promise<void>, done = "已保存") {
  try {
    await action();
    message.value = { ok: true, text: done };
  } catch (error) {
    message.value = { ok: false, text: errorText(error) };
  }
}

// ---------------------------------------------------------------- 日志目录

const logDir = ref("");
watch(
  () => snapshot.value?.log.dir,
  (dir) => {
    logDir.value = dir ?? "";
  },
  { immediate: true },
);

const saveLogDir = () => run(() => commands.setLogDir(logDir.value.trim() || null), "已切换日志目录");
const autoLogDir = () => run(() => commands.setLogDir(null), "已改回自动定位");

// ---------------------------------------------------------------- 快捷键

const toggleOverlayKey = ref("");
const toggleLockKey = ref("");
watch(
  hotkeys,
  (value) => {
    toggleOverlayKey.value = value.toggleOverlay;
    toggleLockKey.value = value.toggleLock;
  },
  { immediate: true },
);

const saveHotkeys = () =>
  run(() =>
    saveSetting(hotkeys.value, "ui.hotkeys", {
      toggleOverlay: toggleOverlayKey.value.trim(),
      toggleLock: toggleLockKey.value.trim(),
    }),
  );
</script>

<template>
  <section class="card">
    <h2>游戏日志</h2>
    <p class="hint">通常能自动找到，位于 Steam 库的 steamapps\common\broken_arrow\GameLogs。</p>
    <div class="row">
      <input v-model="logDir" type="text" class="wide" placeholder="GameLogs 目录" />
      <button class="primary" @click="saveLogDir">使用这个目录</button>
      <button @click="autoLogDir">自动定位</button>
    </div>
  </section>

  <section class="card">
    <h2>悬浮窗</h2>
    <div class="grid">
      <label>背景不透明度</label>
      <div class="row">
        <input
          type="range"
          min="0.3"
          max="1"
          step="0.02"
          :value="appearance.opacity"
          @change="run(() => saveSetting(appearance, 'ui.overlay', { opacity: Number(($event.target as HTMLInputElement).value) }))"
        />
        <span class="num">{{ Math.round(appearance.opacity * 100) }}%</span>
      </div>
      <label>缩放</label>
      <div class="row">
        <input
          type="range"
          min="0.8"
          max="1.5"
          step="0.05"
          :value="appearance.scale"
          @change="run(() => saveSetting(appearance, 'ui.overlay', { scale: Number(($event.target as HTMLInputElement).value) }))"
        />
        <span class="num">{{ Math.round(appearance.scale * 100) }}%</span>
      </div>
      <label>显示</label>
      <div class="row">
        <button @click="run(() => commands.setOverlayVisible(!status?.overlayVisible), '已切换')">
          {{ status?.overlayVisible ? "隐藏悬浮窗" : "显示悬浮窗" }}
        </button>
        <button @click="run(() => commands.setOverlayLocked(!status?.overlayLocked), '已切换')">
          {{ status?.overlayLocked ? "解锁（可拖动、可点击）" : "锁定（鼠标穿透）" }}
        </button>
      </div>
      <label>提醒时</label>
      <div class="row">
        <label class="check">
          <input
            type="checkbox"
            :checked="alerts.sound"
            @change="run(() => saveSetting(alerts, 'ui.alerts', { sound: ($event.target as HTMLInputElement).checked }))"
          />
          响铃
        </label>
        <label class="check">
          <input
            type="checkbox"
            :checked="alerts.showOverlay"
            @change="run(() => saveSetting(alerts, 'ui.alerts', { showOverlay: ($event.target as HTMLInputElement).checked }))"
          />
          显示被隐藏的悬浮窗
        </label>
      </div>
    </div>
  </section>

  <section class="card">
    <h2>快捷键</h2>
    <p class="hint">格式如 Ctrl+Alt+B。留空表示不使用。</p>
    <div class="grid">
      <label>显示 / 隐藏悬浮窗</label>
      <input v-model="toggleOverlayKey" type="text" />
      <label>锁定 / 解锁悬浮窗</label>
      <input v-model="toggleLockKey" type="text" />
    </div>
    <div class="row actions">
      <button class="primary" @click="saveHotkeys">保存快捷键</button>
    </div>
    <p v-for="error in status?.hotkeyErrors ?? []" :key="error" class="error">{{ error }}</p>
  </section>

  <section class="card">
    <h2>数据</h2>
    <p class="hint">长期数据保存在 {{ status?.dataDir ?? "…" }}</p>
  </section>

  <p v-if="message" class="toast" :class="message.ok ? 'ok' : 'error'">{{ message.text }}</p>
</template>

<style scoped>
.card + .card {
  margin-top: 16px;
}

.wide {
  flex: 1;
  min-width: 320px;
}

.grid {
  display: grid;
  grid-template-columns: 150px 1fr;
  gap: 10px 16px;
  align-items: center;
}

.grid > label {
  color: var(--bax-muted);
  font-size: 13px;
}

.check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-right: 12px;
}

.actions {
  margin-top: 12px;
}

.toast {
  margin-top: 16px;
}
</style>
