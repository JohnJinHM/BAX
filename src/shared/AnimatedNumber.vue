<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";

const props = withDefaults(defineProps<{ value: number | null | undefined; enabled?: boolean; signed?: boolean }>(), { enabled: true });
const shown = ref<number | null>(null);
const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
let frame = 0;
function update() {
  cancelAnimationFrame(frame);
  const target = props.value;
  if (target == null || !Number.isFinite(target)) { shown.value = null; return; }
  if (!props.enabled || reduced.matches) { shown.value = target; return; }
  const from = shown.value ?? 0;
  const start = performance.now();
  const tick = (now: number) => {
    const t = Math.min(1, (now - start) / 420);
    shown.value = from + (target - from) * (1 - (1 - t) ** 3);
    if (t < 1) frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
}
watch(() => [props.value, props.enabled], update, { immediate: true });
reduced.addEventListener("change", update);
onBeforeUnmount(() => { cancelAnimationFrame(frame); reduced.removeEventListener("change", update); });
function text(n: number | null | undefined) { return n == null ? "–" : `${props.signed && Math.round(n) > 0 ? "+" : ""}${Math.round(n)}`; }
</script>
<template><span class="num" :aria-label="text(value)"><span aria-hidden="true">{{ text(shown) }}</span></span></template>
