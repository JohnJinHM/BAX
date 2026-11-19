<script setup lang="ts">
import type { UnitClass } from "../shared/bindings/UnitClass";
import { unitTypes } from "./unit-types";
defineProps<{ kind: UnitClass }>();
const baseUrl = `${import.meta.env.BASE_URL}ui-icons/types/`;
</script>
<template>
  <span class="unit-glyph" :class="{ tagged: unitTypes[kind].tag }" :style="{ '--type-color': unitTypes[kind].color }" aria-hidden="true">
    <span v-if="unitTypes[kind].file" class="game-symbol" :style="{ maskImage: `url('${baseUrl}${unitTypes[kind].file}')`, WebkitMaskImage: `url('${baseUrl}${unitTypes[kind].file}')` }" />
    <span v-if="unitTypes[kind].tag" class="type-tag">{{ unitTypes[kind].tag }}</span>
  </span>
</template>
<style scoped>
.unit-glyph { display: inline-flex; width: 24px; height: 24px; align-items: center; justify-content: center; flex-shrink: 0; }
.game-symbol { display: block; width: 24px; height: 24px; background: currentColor; mask-size: contain; mask-position: center; mask-repeat: no-repeat; flex-shrink: 0; }
.tagged { width: 48px; gap: 2px; border-bottom: 2px solid var(--type-color); }
.tagged .game-symbol { width: 20px; height: 20px; }
.type-tag { white-space: nowrap; color: var(--type-color); font-size: 11px; line-height: 1; }
</style>
