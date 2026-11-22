<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { commands, errorText, useAppStatus, useSnapshot } from "../shared/engine";
import { type DiagSettings, saveSetting, useSetting } from "../shared/settings";
import { normalizeSkin, type SkinId, skins } from "../shared/skins";

const snapshot = useSnapshot();
const status = useAppStatus();
const update = computed(() => status.value?.update);
const updateBusy = computed(() => ["checking", "downloading", "installing"].includes(update.value?.state ?? ""));
const matchActive = computed(() => snapshot.value?.current != null && snapshot.value.current.phase !== "ended" && snapshot.value.current.connection !== "disconnected");
const updateError = ref("");
async function updateAction(action: () => Promise<void>) {
  updateError.value = "";
  try { await action(); } catch (error) { updateError.value = errorText(error); }
}
const ocr = useSetting(snapshot, "ui.ocr");
const appearance = useSetting(snapshot, "ui.overlay");
const alerts = useSetting(snapshot, "ui.alerts");
const hotkeys = useSetting(snapshot, "ui.hotkeys");
const skin = useSetting(snapshot, "ui.skin");
const currentSkin = computed(() => normalizeSkin(skin.value.id));
const message = ref<{ ok: boolean; text: string } | null>(null);

async function run(action: () => Promise<void>, done = "已保存") {
  try {
    await action();
    message.value = { ok: true, text: done };
  } catch (error) {
    message.value = { ok: false, text: errorText(error) };
  }
}

// ---------------------------------------------------------------- 皮肤

const chooseSkin = (id: SkinId) => run(() => saveSetting(skin.value, "ui.skin", { id }), "已切换皮肤");

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

// ---------------------------------------------------------------- 历史日志

const importing = ref(false);
const importResult = ref<{ ok: boolean; text: string } | null>(null);

async function importHistory() {
  importing.value = true;
  importResult.value = null;
  try {
    const summary = await commands.importHistory();
    const failed = summary.failed > 0 ? `，${summary.failed} 份读取失败` : "";
    importResult.value = { ok: true, text: `读了 ${summary.files} 份日志，记录 ${summary.matches} 场比赛${failed}` };
  } catch (error) {
    importResult.value = { ok: false, text: errorText(error) };
  } finally {
    importing.value = false;
  }
}

const saveHotkeys = () =>
  run(() =>
    saveSetting(hotkeys.value, "ui.hotkeys", {
      toggleOverlay: toggleOverlayKey.value.trim(),
      toggleLock: toggleLockKey.value.trim(),
    }),
  );

// ---------------------------------------------------------------- 诊断

const diag = useSetting(snapshot, "ui.diag");
const setDiagLevel = (level: DiagSettings["level"]) =>
  run(() => saveSetting(diag.value, "ui.diag", { level }));

async function exportDiag() {
  try {
    const path = await commands.exportDiag();
    message.value = { ok: true, text: `诊断包已导出：${path}` };
  } catch (error) {
    message.value = { ok: false, text: errorText(error) };
  }
}
</script>

<template>
  <section v-if="update" class="card">
    <h2>应用更新</h2>
    <p>BAX {{ update.edition }} · {{ update.currentVersion }}</p>
    <p role="status" aria-live="polite" :class="update.state === 'error' ? 'error' : 'hint'">{{ update.message }}</p>
    <p v-if="update.state === 'downloading'" class="hint num">
      已下载 {{ (update.downloaded / 1048576).toFixed(1) }} MB<span v-if="update.total"> / {{ (update.total / 1048576).toFixed(1) }} MB</span>
    </p>
    <p v-if="!update.canInstall" class="hint">开发／便携版请从发布页下载对应版本。便携版更新时保留 data 文件夹。</p>
    <p v-else class="hint">启动后自动检查。点击安装后会退出 BAX，更新当前 {{ update.edition }} 版本并保留本地数据。</p>
    <div class="row">
      <button :disabled="updateBusy" @click="updateAction(commands.checkUpdate)">{{ update.state === 'checking' ? '正在检查…' : '检查更新' }}</button>
      <button v-if="update.version && update.canInstall" class="primary" :disabled="updateBusy || matchActive" @click="updateAction(commands.installUpdate)">下载并安装 {{ update.version }}</button>
      <button @click="updateAction(commands.openReleases)">打开发布页</button>
    </div>
    <p v-if="update.version && matchActive && update.canInstall" class="hint">本场比赛结束后可安装更新。</p>
    <p v-if="updateError" class="error" role="alert">{{ updateError }}</p>
    <details v-if="update.notes"><summary>版本说明</summary><p class="release-notes">{{ update.notes }}</p></details>
  </section>
  <section v-if="status?.ocrSupported" class="card">
    <h2>赛前玩家识别</h2>
    <label class="row">
      <input type="checkbox" :checked="ocr.enabled" @change="run(() => saveSetting(ocr, 'ui.ocr', { enabled: !ocr.enabled }))" />
      在日志名单出现前识别游戏画面的昵称与等级
    </label>
    <p class="hint">优先查找本地玩家，再查询在线候选；日志 ID 到达后更新。仅在游戏位于前台时读取画面，截图不保存。</p>
  </section>
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
    <h2 id="skin-heading">外观</h2>
    <div class="skins" role="radiogroup" aria-labelledby="skin-heading">
      <!-- 每个选项带自己的 data-skin，按那个皮肤的变量渲染，当作预览 -->
      <label v-for="option in skins" :key="option.id" class="skin-option" :data-skin="option.id">
        <span class="skin-preview" aria-hidden="true">
          <span class="sp-nav"><i /><i class="on" /><i /></span>
          <span class="sp-main">
            <span class="sp-card">
              <span class="sp-head"><i class="sp-title" /><b class="sp-value num">12:34</b></span>
              <span class="sp-row self"><i class="sp-friend" /><i class="sp-line" /></span>
              <span class="sp-row"><i class="sp-foe" /><i class="sp-line" /></span>
            </span>
            <span class="sp-button">保存</span>
          </span>
        </span>
        <span class="skin-name">
          <input type="radio" name="skin" :value="option.id" :checked="currentSkin === option.id" @change="chooseSkin(option.id)" />
          {{ option.name }}
        </span>
        <span class="skin-description">{{ option.description }}</span>
      </label>
    </div>
    <p class="hint">主窗口与悬浮窗同时切换。</p>
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
      </div>
      <label>吸附边缘</label>
      <label class="check"><input type="checkbox" :checked="appearance.snapEdges" @change="run(() => saveSetting(appearance, 'ui.overlay', { snapEdges: ($event.target as HTMLInputElement).checked }))" />拖动松手后靠近屏幕边缘时贴边，展开收起时保持贴边</label>
      <label>自动展开</label>
      <label class="check"><input type="checkbox" :checked="appearance.autoExpand" @change="run(() => saveSetting(appearance, 'ui.overlay', { autoExpand: ($event.target as HTMLInputElement).checked }))" />随比赛阶段展开与收起</label>
      <label for="collapse-before">准备剩余秒数</label>
      <div class="row"><input id="collapse-before" type="number" min="0" max="180" :disabled="!appearance.autoExpand" :value="appearance.collapseBeforeBattleSecs" @change="run(() => saveSetting(appearance, 'ui.overlay', { collapseBeforeBattleSecs: Math.max(0, Math.min(180, Number(($event.target as HTMLInputElement).value))) }))" /><span class="hint">默认 60 秒时收起</span></div>
      <label>赛后展示</label>
      <span class="hint">比赛结束时展开并展示加分，退出房间时收起。选图、选卡、选点时自动展开。</span>
      <label>动效</label>
      <label class="check"><input type="checkbox" :checked="appearance.animations" @change="run(() => saveSetting(appearance, 'ui.overlay', { animations: ($event.target as HTMLInputElement).checked }))" />展开过渡、均分计数与加分动效</label>
      <label>锁定</label>
      <div class="row">
        <span :class="status?.overlayLocked ? 'ok' : 'hint'">
          {{ status?.overlayLocked ? "已锁定：鼠标穿透到游戏，只有锁按钮可以点" : "未锁定：可以拖动、点击悬浮窗上的按钮" }}
        </span>
        <button @click="run(() => commands.setOverlayLocked(!status?.overlayLocked), '已切换')">
          {{ status?.overlayLocked ? "解锁" : "锁定" }}
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
    <div class="row">
      <button :disabled="importing" @click="importHistory">{{ importing ? "正在导入…" : "导入历史日志" }}</button>
      <span class="hint">把日志目录里以前的日志都读一遍，用于统计以前同场的次数。重复导入不会重复计数。</span>
    </div>
    <p v-if="importResult" :class="importResult.ok ? 'ok' : 'error'">{{ importResult.text }}</p>
  </section>

  <section class="card">
    <h2>诊断</h2>
    <p class="hint">
      BAX 把运行过程记在诊断日志里，出问题时用来排查。日志里有玩家 ID，需要发给别人时请用“导出诊断包”：
      导出时玩家 ID、比赛 ID 和 Windows 用户名会被替换，不包含玩家名字。
    </p>
    <div class="grid">
      <label>位置</label>
      <div class="row">
        <span class="path">{{ status?.diagDir || "未写入" }}</span>
        <button :disabled="!status?.diagDir" @click="run(() => commands.openDiagDir(), '已打开')">打开目录</button>
      </div>
      <label>详细程度</label>
      <div class="row">
        <label class="check">
          <input
            type="radio"
            name="diag-level"
            :checked="diag.level === 'standard'"
            :disabled="!!status?.diagLockedBy"
            @change="setDiagLevel('standard')"
          />
          标准
        </label>
        <label class="check">
          <input
            type="radio"
            name="diag-level"
            :checked="diag.level === 'verbose'"
            :disabled="!!status?.diagLockedBy"
            @change="setDiagLevel('verbose')"
          />
          详细（复现问题时打开）
        </label>
      </div>
    </div>
    <p v-if="status?.diagLockedBy" class="hint">当前档位由{{ status.diagLockedBy }}决定。</p>
    <p v-if="status?.diagError" class="error">{{ status.diagError }}</p>
    <div class="row actions">
      <button class="primary" :disabled="!status?.diagDir" @click="exportDiag">导出诊断包</button>
    </div>
  </section>

  <p v-if="message" class="toast" :class="message.ok ? 'ok' : 'error'">{{ message.text }}</p>
</template>

<style scoped>
.skins { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 12px; margin-bottom: 8px; }
.skin-option { display: flex; flex-direction: column; gap: 6px; min-width: 0; padding: 10px; border: 1px solid var(--bax-border); border-radius: var(--bax-radius); background: var(--bax-app-bg); color: var(--bax-fg); font-family: var(--bax-font); cursor: pointer; }
.skin-option:hover { border-color: color-mix(in srgb, var(--bax-accent) 50%, var(--bax-border)); }
.skin-option:has(input:checked) { border-color: var(--bax-accent); box-shadow: inset 0 0 0 1px var(--bax-accent); }
.skin-option:has(input:focus-visible) { outline: 2px solid var(--bax-accent); outline-offset: 2px; }
.skin-option input { margin: 0; accent-color: var(--bax-accent); }
.skin-name { display: flex; align-items: center; gap: 6px; font-weight: 600; }
.skin-description { color: var(--bax-muted); font-size: 12px; }
.skin-preview { display: grid; grid-template-columns: 28% minmax(0, 1fr); height: 96px; margin-bottom: 4px; border: 1px solid var(--bax-border); border-radius: var(--bax-radius-sm); overflow: hidden; }
.skin-preview i { display: block; }
.sp-nav { display: flex; flex-direction: column; gap: 6px; padding: 10px 6px; background: var(--bax-nav-bg); border-right: 1px solid var(--bax-border); }
.sp-nav i { height: 6px; border-radius: min(2px, var(--bax-radius-xs)); background: var(--bax-faint); }
.sp-nav i.on { background: var(--bax-muted); box-shadow: -3px 0 0 var(--bax-nav-active-edge); }
.sp-main { display: flex; flex-direction: column; gap: 6px; padding: 8px; }
.sp-card { display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid var(--bax-border); border-radius: var(--bax-radius-xs); background: var(--bax-surface); }
.sp-head { display: flex; align-items: center; justify-content: space-between; gap: 6px; }
.sp-title { width: 40%; height: 5px; background: var(--bax-muted); box-shadow: calc(-1 * var(--bax-heading-mark)) 0 0 var(--bax-heading-mark-color); }
.sp-value { color: var(--bax-value); font-size: 13px; line-height: 1; }
.sp-row { display: flex; align-items: center; gap: 6px; padding: 2px 4px; background: transparent; }
.sp-row.self { background: var(--bax-self-bg); box-shadow: inset 2px 0 0 var(--bax-self-edge); }
.sp-friend, .sp-foe { width: 18px; height: 5px; border-radius: min(2px, var(--bax-radius-xs)); background: var(--bax-friend); }
.sp-foe { background: var(--bax-foe); }
.sp-line { flex: 1; height: 4px; background: var(--bax-faint); }
.sp-button { align-self: flex-end; padding: 1px 8px; border-radius: var(--bax-radius-xs); background: var(--bax-accent); color: var(--bax-on-accent); font-size: 10px; font-weight: 600; }
.release-notes { white-space: pre-wrap; overflow-wrap: anywhere; }
input[type="number"] { width: 72px; padding: 5px 8px; border: 1px solid var(--bax-border); border-radius: var(--bax-radius-sm); background: var(--bax-surface-2); color: var(--bax-fg); }
input[type="number"]:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 2px; }
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

.path {
  font-family: var(--bax-mono);
  font-size: 12px;
  word-break: break-all;
}
</style>
