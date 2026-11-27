<script setup lang="ts">
import { computed, ref } from "vue";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { useSetting } from "../shared/settings";
import { calibratedAxes, combatAxes, preferenceAxes, validRadarBase, type RadarAxis } from "./player-analysis";

const saved = useSetting(useSnapshot(), "ui.radarBaselines");
const groups = [{title:"作战效能",axes:combatAxes},{title:"单位偏好",axes:preferenceAxes}];
const defaults = [...combatAxes,...preferenceAxes];
const current = computed(() => Object.fromEntries(calibratedAxes(defaults,saved.value).map(a => [a.key,a.base])));
const edits = ref<Record<string,string>>({});
const busy = ref(false);
const message = ref("");
const failed = ref(false);
const input = (axis:RadarAxis) => edits.value[axis.key] ?? String(current.value[axis.key]);
const changed = computed(() => defaults.some(a => Number(input(a)) !== current.value[a.key]));
const invalid = computed(() => defaults.some(a => !validRadarBase(a,Number(input(a)))));
const canReset = computed(() => changed.value || defaults.some(a => current.value[a.key] !== a.base));
function edit(axis:RadarAxis,event:Event) {
  edits.value[axis.key] = (event.target as HTMLInputElement).value;
  message.value = "";
}
async function persist(reset:boolean) {
  if (busy.value || (!reset && invalid.value)) return;
  const values:Record<string,number> = {};
  if (!reset) for (const a of defaults) {
    const n = Number(input(a));
    if (n !== a.base) values[a.key] = n;
  }
  busy.value = true;
  try {
    await commands.setUiSetting("ui.radarBaselines",values);
    edits.value = {};
    failed.value = false;
    message.value = reset ? "已恢复默认基准" : "已保存雷达图基准";
  } catch (error) {
    failed.value = true;
    message.value = errorText(error);
  } finally { busy.value = false; }
}
</script>
<template>
  <section class="card radar-settings" aria-labelledby="radar-heading">
    <h2 id="radar-heading">玩家雷达图基准</h2>
    <p class="hint">每项数值对应一条轴上的虚线位置。默认值取自本机归档的 7,311 个玩家场次。</p>
    <form @submit.prevent="persist(false)">
      <fieldset v-for="g in groups" :key="g.title" :disabled="busy">
        <legend>{{ g.title }}</legend>
        <div class="fields">
          <label v-for="a in g.axes" :key="a.key">{{ a.label }}
            <span><input type="number" required step="any" min="0" :max="a.unit === '%' ? 100 : undefined"
              :aria-label="g.title+' · '+a.label" :aria-invalid="!validRadarBase(a,Number(input(a)))"
              :value="input(a)" @input="edit(a,$event)" />{{ a.unit }}</span>
          </label>
        </div>
      </fieldset>
      <div class="actions">
        <button class="primary" type="submit" :disabled="busy || !changed || invalid">保存基准</button>
        <button type="button" :disabled="busy || !canReset" @click="persist(true)">恢复默认基准</button>
        <span v-if="invalid" class="error" role="status">基准须大于 0，百分比不能超过 100%。</span>
        <span v-else role="status" :class="failed ? 'error' : 'hint'">{{ message }}</span>
      </div>
    </form>
  </section>
</template>
<style scoped>
fieldset {padding:0;margin:14px 0;border:0;min-width:0;}
legend {padding:0;margin:0 0 8px;color:var(--bax-fg);font-weight:600;}
.fields {display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:8px 14px;max-width:760px;}
label {display:grid;gap:3px;color:var(--bax-muted);font-size:calc(12px * var(--bax-font-scale));}
label span {display:flex;align-items:center;gap:5px;}
input {width:100%;min-width:0;padding:5px 7px;font:inherit;font-variant-numeric:tabular-nums;color:var(--bax-fg);background:var(--bax-surface-2);border:1px solid var(--bax-faint);border-radius:var(--bax-radius-xs);}
input[aria-invalid=true] {border-color:var(--bax-bad);}
.actions {display:flex;gap:8px;align-items:center;flex-wrap:wrap;}
@media(max-width:740px) {.fields {grid-template-columns:repeat(2,minmax(0,1fr));}}
</style>
