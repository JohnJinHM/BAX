<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
const props = defineProps<{ label: string; options: { value: string | number; label: string }[] }>();
const selected = defineModel<(string | number)[]>({ required: true });
const root = ref<HTMLDetailsElement | null>(null);
const count = computed(() => props.options.filter(o => selected.value.includes(o.value)).length);
function setAll(checked: boolean) {
  const available = new Set(props.options.map(o => o.value));
  selected.value = [...selected.value.filter(value => !available.has(value)), ...(checked ? [...available] : [])];
}
function close(event: PointerEvent) { if (root.value && !root.value.contains(event.target as Node)) root.value.open = false; }
function escape(event: KeyboardEvent) {
  if (event.key === "Escape" && root.value?.open) {
    root.value.open = false; root.value.querySelector("summary")?.focus();
  }
}
onMounted(() => document.addEventListener("pointerdown", close));
onBeforeUnmount(() => document.removeEventListener("pointerdown", close));
</script>

<template>
  <details ref="root" class="check-filter" @keydown="escape">
    <summary class="dropdown-trigger">{{ label }} <span>{{ count === options.length ? '全部' : `${count}/${options.length}` }}</span></summary>
    <div class="check-options" :aria-label="label">
      <div class="actions"><button type="button" @click="setAll(true)">全选</button><button type="button" @click="setAll(false)">清空</button></div>
      <label v-for="option in options" :key="option.value"><input v-model="selected" type="checkbox" :value="option.value" />{{ option.label }}</label>
      <p v-if="!options.length">没有可选{{ label }}</p>
    </div>
  </details>
</template>

<style scoped>
.check-filter { position: relative; font-size: 12px; color: var(--bax-fg); }
summary { display: flex; align-items: center; list-style: none; cursor: pointer; min-height: 32px; padding: 6px 10px; border: 1px solid var(--bax-faint); border-radius: 4px; background: var(--bax-surface-2); }
summary::-webkit-details-marker { display: none; }
summary span { color: var(--bax-muted); margin-left: 8px; }
summary:focus-visible { outline: 2px solid var(--bax-accent); outline-offset: 2px; }
.check-options { position: absolute; top: calc(100% + 4px); left: 0; z-index: 8; min-width: 220px; max-width: min(320px, 75vw); max-height: 280px; overflow: auto; padding: 8px; border: 1px solid var(--bax-faint); border-radius: 4px; background: var(--bax-surface-2); }
.check-options label { display: flex; align-items: center; gap: 8px; padding: 6px 4px; overflow-wrap: anywhere; cursor: pointer; }
.check-options label:hover { background: var(--bax-surface); }
input { flex: none; }
.actions { display: flex; gap: 8px; padding-bottom: 6px; border-bottom: 1px solid var(--bax-border); }
.actions button { padding: 3px 10px; font-size: 12px; }
</style>
