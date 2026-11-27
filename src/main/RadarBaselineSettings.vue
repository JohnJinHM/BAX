<script setup lang="ts">
import { computed, ref } from "vue";
import { commands, errorText, useSnapshot } from "../shared/engine";
import { useSetting } from "../shared/settings";
import { baseKey, calibratedAxes, combatAxes, preferenceAxes, validRadarAxis, type RadarAxis } from "./player-analysis";

const groups = [{title:"作战效能",axes:combatAxes},{title:"单位偏好",axes:preferenceAxes}];
const defaults = [...combatAxes,...preferenceAxes];
const saved = useSetting(useSnapshot(),"ui.radarBaselines");
const current = computed(() => Object.fromEntries(calibratedAxes(defaults,saved.value).map(a => [a.key,a])));
const edits = ref<Record<string,string>>({});
const busy = ref(false);
const message = ref("");
const failed = ref(false);
function fields(axis: RadarAxis) {
  const high = {key:baseKey(axis), label:axis.low == null ? "基准" : "高", value:axis.base};
  return axis.low == null ? [high] : [{key:axis.key+".low",label:"低",value:axis.low},high];
}
function input(key:string, fallback:number) { return edits.value[key] ?? String(fallback); }
function numeric(key:string, fallback:number) {
  const text = input(key,fallback);
  return text.trim() === "" ? NaN : Number(text);
}
const drafts = computed(() => Object.fromEntries(defaults.map(a => {
  const axis = current.value[a.key];
  const next = {...axis,base:numeric(baseKey(axis),axis.base)};
  if (axis.low != null) next.low = numeric(axis.key+".low",axis.low);
  return [a.key,next];
})));
const changed = computed(() => defaults.some(a => fields(current.value[a.key]).some(f => numeric(f.key,f.value) !== f.value)));
const invalid = computed(() => defaults.some(a => !validRadarAxis(drafts.value[a.key])));
const canReset = computed(() => changed.value || defaults.some(a => a.base !== current.value[a.key].base || a.low !== current.value[a.key].low));
function edit(key:string,event:Event) {
  edits.value[key] = (event.target as HTMLInputElement).value;
  message.value = "";
}
async function persist(reset:boolean) {
  if (busy.value || (!reset && invalid.value)) return;
  const values:Record<string,number> = {};
  if (!reset) for (const a of defaults) {
    const draft = drafts.value[a.key];
    if (draft.base !== a.base) values[baseKey(a)] = draft.base;
    if (draft.low != null && draft.low !== a.low) values[a.key+".low"] = draft.low;
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
    <p class="hint">作战效能从低值起线性绘制，低值及以下在中心，超过高值时扩大量程。单位偏好按各兵种的参考出兵占比绘制。</p>
    <form @submit.prevent="persist(false)">
      <fieldset v-for="g in groups" :key="g.title" :disabled="busy">
        <legend>{{ g.title }}</legend>
        <div class="fields">
          <div v-for="a in g.axes" :key="a.key" class="axis">
            <strong>{{ a.label }}</strong><small>{{ a.unit }}</small>
            <label v-for="f in fields(current[a.key])" :key="f.key">
              <span v-if="a.low != null">{{ f.label }}</span>
              <input type="number" required step="any" min="0" :max="a.unit === '%' ? 100 : undefined"
                :aria-label="g.title+' · '+a.label+(a.low != null ? ' · '+f.label : '')" :aria-invalid="!validRadarAxis(drafts[a.key])"
                :value="input(f.key,f.value)" @input="edit(f.key,$event)" />
            </label>
          </div>
        </div>
      </fieldset>
      <div class="actions">
        <button class="primary" type="submit" :disabled="busy || !changed || invalid">保存基准</button>
        <button type="button" :disabled="busy || !canReset" @click="persist(true)">恢复默认基准</button>
        <span v-if="invalid" class="error" role="status">低值须大于或等于 0，高值须大于低值；参考占比须大于 0，百分比不能超过 100%。</span>
        <span v-else role="status" :class="failed ? 'error' : 'hint'">{{ message }}</span>
      </div>
    </form>
  </section>
</template>
<style scoped>
fieldset {padding:0;margin:14px 0;border:0;min-width:0;}
legend {padding:0;margin:0 0 8px;color:var(--bax-fg);font-weight:600;}
.fields {display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:8px 14px;max-width:760px;}
.axis {display:grid;align-content:start;grid-template-columns:1fr auto;gap:4px;color:var(--bax-muted);font-size:calc(12px * var(--bax-font-scale));}
.axis small {font-size:inherit;}.axis label {grid-column:1/-1;display:flex;align-items:center;gap:5px;}
label span {flex:none;}
input {width:100%;min-width:0;padding:5px 7px;font:inherit;font-variant-numeric:tabular-nums;color:var(--bax-fg);background:var(--bax-surface-2);border:1px solid var(--bax-faint);border-radius:var(--bax-radius-xs);}
input[aria-invalid=true] {border-color:var(--bax-bad);}
.actions {display:flex;gap:8px;align-items:center;flex-wrap:wrap;}
@media(max-width:740px) {.fields {grid-template-columns:repeat(2,minmax(0,1fr));}}
</style>
