<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { commands, errorText, useSnapshot } from "../shared/engine";
import { logStatus } from "../shared/match-presentation";

defineProps<{ settings?: boolean }>();
const snapshot = useSnapshot();
const log = computed(() => snapshot.value?.log);
const status = computed(() => logStatus(log.value));
const path = ref("");
const busy = ref("");
const failure = ref("");
watch(() => log.value?.dir, (dir, old) => {
  if (!path.value || path.value === old) path.value = dir ?? "";
}, { immediate: true });

async function change(mode: "browse" | "manual" | "auto") {
  if (busy.value) return;
  failure.value = "";
  busy.value = mode === "auto" ? "正在自动定位…" : "正在选择目录…";
  try {
    const dir = mode === "browse" ? await commands.pickLogDir() : mode === "manual" ? path.value.trim() : null;
    if (mode !== "auto" && !dir) return;
    busy.value = dir ? "正在检查目录…" : "正在自动定位…";
    await commands.setLogDir(dir);
    path.value = dir ?? log.value?.dir ?? "";
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    busy.value = "";
  }
}
</script>

<template>
  <p v-if="settings" class="hint">通常位于 Steam 库的 steamapps\common\broken_arrow\GameLogs。</p>
  <div class="row">
    <button v-if="settings || status.needsDirectory" :disabled="!!busy" @click="change('browse')">选择 GameLogs 目录</button>
    <button v-if="settings" :disabled="!!busy" @click="change('auto')">自动定位</button>
    <p role="status" class="log-status" :class="log?.error ? 'error' : 'hint'">
      {{ busy || status.text }}<template v-if="!settings && !busy && log?.ready && log.file && !log.error">，等待进入对局。</template>
      <span v-if="settings && log?.dir && !busy" class="path">{{ log.dir }}</span>
    </p>
  </div>
  <details v-if="settings" class="manual">
    <summary>手动输入目录路径</summary>
    <form class="row" @submit.prevent="change('manual')">
      <input v-model="path" aria-label="GameLogs 目录路径" type="text" placeholder="GameLogs 目录" />
      <button :disabled="!!busy || !path.trim()">使用这个目录</button>
    </form>
  </details>
  <p v-if="failure" role="alert" class="error">{{ failure }}</p>
</template>

<style scoped>
.log-status { flex: 1 1 12em; min-width: 0; margin: 0; line-height: 1.35; overflow-wrap: anywhere; }
.path { display: block; font-family: var(--bax-mono); }
.manual { margin-top: 10px; }
.manual summary { cursor: pointer; }
.manual form { margin-top: 6px; }
.manual input { flex: 1; min-width: 12em; }
</style>
