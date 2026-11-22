<script setup lang="ts">
import { computed, ref, watch } from "vue";

import type { ReminderRule } from "../shared/bindings/ReminderRule";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { mmss, parseMmss } from "../shared/format";

const snapshot = useSnapshot();

interface Draft {
  id: number;
  time: string;
  label: string;
  enabled: boolean;
}

const drafts = ref<Draft[]>([]);
const dirty = ref(false);
const message = ref<{ ok: boolean; text: string } | null>(null);

// 保存之前，引擎推来的新快照不覆盖正在编辑的内容
watch(
  () => snapshot.value?.reminders,
  (rules) => {
    if (!rules || dirty.value) return;
    drafts.value = rules.map((r) => ({
      id: r.id,
      time: mmss(r.atSecs * 1000),
      label: r.label,
      enabled: r.enabled,
    }));
  },
  { immediate: true },
);

const states = computed(() => {
  const map = new Map<number, string>();
  for (const r of snapshot.value?.reminders ?? []) {
    if (r.outcome === "fired") map.set(r.id, "本场已提醒");
    else if (r.outcome === "missed") map.set(r.id, "本场已错过");
    else if (r.fireAt != null) map.set(r.id, "本场等待中");
  }
  return map;
});

function edit() {
  dirty.value = true;
  message.value = null;
}

function add() {
  const id = Math.max(0, ...drafts.value.map((d) => d.id)) + 1;
  drafts.value.push({ id, time: "", label: "", enabled: true });
  edit();
}

function remove(index: number) {
  drafts.value.splice(index, 1);
  edit();
}

async function save() {
  const rules: ReminderRule[] = [];
  for (const d of drafts.value) {
    const ms = parseMmss(d.time);
    if (ms == null || ms < 0) {
      message.value = { ok: false, text: `“${d.time}”不是有效的时间，格式为 分:秒，例如 10:00` };
      return;
    }
    const secs = Math.round(ms / 1000);
    // 不填提示文字时用时间本身
    const label = d.label.trim() || `开局 ${mmss(secs * 1000)}`;
    rules.push({ id: d.id, atSecs: secs, label, enabled: d.enabled });
  }
  try {
    await commands.setReminders(rules);
    dirty.value = false;
    message.value = { ok: true, text: "已保存" };
  } catch (error) {
    message.value = { ok: false, text: errorText(error) };
  }
}

function discard() {
  dirty.value = false;
  message.value = null;
  const rules = snapshot.value?.reminders ?? [];
  drafts.value = rules.map((r) => ({ id: r.id, time: mmss(r.atSecs * 1000), label: r.label, enabled: r.enabled }));
}

// ---------------------------------------------------------------- 校准

const calibration = ref("");
const calibrationMode = ref<"elapsed" | "deployment">("elapsed");
const calibrationMessage = ref<{ ok: boolean; text: string } | null>(null);
const canCalibrate = computed(() => {
  const m = snapshot.value?.current;
  return !!m && m.phase !== "ended";
});

async function calibrate() {
  const ms = parseMmss(calibration.value);
  if (ms == null || ms < 0) {
    calibrationMessage.value = { ok: false, text: "格式为 分:秒，例如 12:34" };
    return;
  }
  try {
    await commands.calibrate(calibrationMode.value === "elapsed" ? ms : -ms);
    calibrationMessage.value = { ok: true, text: "已校准" };
  } catch (error) {
    calibrationMessage.value = { ok: false, text: errorText(error) };
  }
}
</script>

<template>
  <section class="card">
    <h2>计时提醒</h2>
    <p class="hint">默认不设提醒。需要时添加一条：时间从正式开局（准备阶段结束）算起，到点时响铃并在悬浮窗上闪烁；对局中途才打开 BAX 时，已经过去的提醒不会补发。</p>
    <p v-if="!drafts.length" class="hint">还没有提醒。</p>
    <table v-else>
      <thead>
        <tr>
          <th>启用</th>
          <th>开局后</th>
          <th>提示文字</th>
          <th></th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(d, index) in drafts" :key="d.id">
          <td><input v-model="d.enabled" type="checkbox" @change="edit" /></td>
          <td><input v-model="d.time" type="text" class="time num" placeholder="10:00" @input="edit" /></td>
          <td><input v-model="d.label" type="text" class="label" placeholder="第 10 分钟" @input="edit" /></td>
          <td class="hint">{{ states.get(d.id) ?? "" }}</td>
          <td><button @click="remove(index)">删除</button></td>
        </tr>
      </tbody>
    </table>
    <div class="row actions">
      <button @click="add">添加提醒</button>
      <button class="primary" :disabled="!dirty" @click="save">保存</button>
      <button :disabled="!dirty" @click="discard">放弃修改</button>
      <span v-if="message" :class="message.ok ? 'ok' : 'error'">{{ message.text }}</span>
    </div>
  </section>

  <section class="card">
    <h2>计时校准</h2>
    <p class="hint">
      重连进入的比赛没有计时基准，可以对照游戏画面填写此刻的时间。
    </p>
    <div class="row">
      <select v-model="calibrationMode">
        <option value="elapsed">游戏里已开局</option>
        <option value="deployment">准备阶段还剩</option>
      </select>
      <input v-model="calibration" type="text" class="time num" placeholder="12:34" @keyup.enter="calibrate" />
      <button class="primary" :disabled="!canCalibrate" @click="calibrate">校准</button>
      <span v-if="!canCalibrate" class="hint">比赛进行中才能校准</span>
      <span v-if="calibrationMessage" :class="calibrationMessage.ok ? 'ok' : 'error'">{{ calibrationMessage.text }}</span>
    </div>
  </section>
</template>

<style scoped>
.card + .card {
  margin-top: 16px;
}

table {
  border-collapse: collapse;
  margin: 8px 0;
}

th {
  text-align: left;
  font-weight: 500;
  font-size: 12px;
  color: var(--bax-muted);
  padding: 4px 8px 4px 0;
}

td {
  padding: 4px 8px 4px 0;
}

.time {
  width: 80px;
}

.label {
  width: 220px;
}

select {
  padding: 5px 8px;
  border: 1px solid var(--bax-border);
  border-radius: var(--bax-radius-sm);
  background: var(--bax-surface-2);
}

.actions {
  margin-top: 4px;
}
</style>
