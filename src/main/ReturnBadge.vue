<script setup lang="ts">
const iconUrl = `${import.meta.env.BASE_URL}ui-icons/back-to-base.png`;
</script>
<template>
  <span class="return-badge" role="img" aria-label="已回收" title="Back to base：已回收，回收时间未知" :style="{ maskImage: `url('${iconUrl}')`, WebkitMaskImage: `url('${iconUrl}')` }" />
</template>
<style scoped>
.return-badge { display: inline-block; width: 12px; height: 12px; flex-shrink: 0; background: var(--bax-fg); mask-size: contain; mask-position: center; mask-repeat: no-repeat; }
</style>
