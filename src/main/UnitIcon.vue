<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { UnitClass } from "../shared/bindings/UnitClass";
import { unitIconFile } from "./unit-icon";
import UnitGlyph from "./UnitGlyph.vue";

const props = defineProps<{ id: number; name: string; kind: UnitClass; unitOnly?: boolean }>();
const baseUrl = import.meta.env.BASE_URL;
const file = computed(() => unitIconFile(props.id, props.name));
const failed = ref(false);
watch(file, () => { failed.value = false; });
</script>
<template>
  <img v-if="file && !failed" :src="`${baseUrl}unit-icons/${encodeURIComponent(file)}`" alt="" class="unit-icon" :class="{ white: unitOnly }" loading="lazy" @error="failed = true" />
  <span v-else-if="unitOnly" class="missing-unit" :title="`${name}：暂无单位图标`">{{ name }}</span>
  <UnitGlyph v-else :kind="kind" />
</template>
<style scoped>
.unit-icon { width: 48px; height: 26px; object-fit: contain; flex-shrink: 0; }
.white { filter: brightness(0) invert(1); }
.missing-unit { display: block; width: 44px; color: white; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
