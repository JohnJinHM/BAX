<script setup lang="ts">
import { computed, ref, watch } from "vue";
const props = defineProps<{ id: number; label: string }>();
const failed = ref(false);
watch(() => props.id, () => { failed.value = false; });
const available = computed(() => [3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 15, 16].includes(props.id));
</script>

<template>
  <img v-if="available && !failed" class="spec-icon" :src="`/spec-icons/${id}.png`" :alt="label" :title="label" @error="failed = true" />
  <span v-else class="spec-icon fallback" role="img" :aria-label="label" :title="label">{{ id }}</span>
</template>

<style scoped>
.spec-icon { display: inline-block; width: 28px; height: 28px; object-fit: contain; flex-shrink: 0; vertical-align: middle; }
.fallback { text-align: center; line-height: 28px; color: var(--bax-muted); font-size: 11px; }
</style>
