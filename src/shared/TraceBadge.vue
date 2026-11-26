<script setup lang="ts">
defineProps<{ count: number | null }>();
</script>

<template>
  <span v-if="count != null" class="trace-badge" :class="{ marked: count > 0 }"
    :title="`BATrace 社区标记 ${count} 次`" :aria-label="`BATrace 社区标记 ${count} 次`">（标{{ count }}）</span>
</template>

<style scoped>
.trace-badge { flex: none; color: var(--bax-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
.trace-badge.marked { color: var(--bax-warn); }
</style>
