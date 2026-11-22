<script setup lang="ts">
// 简单的折线：ELO 变化趋势。完整的图表到 0.4 引入图表库后再做。
import { computed } from "vue";

const props = defineProps<{
  points: { at: number; value: number }[];
  width?: number;
  height?: number;
}>();

const width = computed(() => props.width ?? 560);
const height = computed(() => props.height ?? 96);
const pad = 6;

const shape = computed(() => {
  const pts = props.points;
  if (pts.length < 2) return null;
  const xs = pts.map((p) => p.at);
  const ys = pts.map((p) => p.value);
  const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
  const [y0, y1] = [Math.min(...ys), Math.max(...ys)];
  const sx = (x: number) => pad + ((x - x0) / Math.max(1, x1 - x0)) * (width.value - pad * 2);
  const sy = (y: number) => height.value - pad - ((y - y0) / Math.max(1, y1 - y0)) * (height.value - pad * 2);
  const line = pts.map((p) => `${sx(p.at).toFixed(1)},${sy(p.value).toFixed(1)}`).join(" ");
  const last = pts[pts.length - 1];
  return { line, min: y0, max: y1, lastX: sx(last.at), lastY: sy(last.value) };
});
</script>

<template>
  <div class="spark">
    <svg v-if="shape" :viewBox="`0 0 ${width} ${height}`" :width="width" :height="height" role="img" aria-label="ELO 变化">
      <polyline :points="shape.line" fill="none" stroke="var(--bax-accent)" stroke-width="2" stroke-linejoin="round" />
      <circle :cx="shape.lastX" :cy="shape.lastY" r="3.5" fill="var(--bax-accent)" />
    </svg>
    <div v-if="shape" class="range num">最低 {{ Math.round(shape.min) }} · 最高 {{ Math.round(shape.max) }}</div>
    <div v-else class="hint">记录少于两次，还画不出变化。</div>
  </div>
</template>

<style scoped>
svg {
  display: block;
  max-width: 100%;
  height: auto;
}

.range {
  margin-top: 4px;
  font-size: calc(12px * var(--bax-font-scale));
  color: var(--bax-muted);
}
</style>
