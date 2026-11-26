<script setup lang="ts">
defineProps<{ count: number | null }>();
</script>

<template>
  <span v-if="count != null && count > 0" class="trace-badge"
    :title="`BATrace 社区标记 ${count} 次`" :aria-label="`BATrace 社区标记 ${count} 次`">（标{{ count }}）</span>
</template>

<style scoped>
.trace-badge { flex: none; color: var(--bax-warn); font-variant-numeric: tabular-nums; white-space: nowrap; }
</style>
