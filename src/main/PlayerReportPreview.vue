<script setup lang="ts">
import { defineAsyncComponent, onMounted, ref } from "vue";
const ReportView = defineAsyncComponent(() => import("./ReportView.vue"));
defineProps<{matchId:number; playerId:number; playerName:string; map:string | null}>();
const emit = defineEmits<{close:[]}>();
const dialog = ref<HTMLDialogElement>();
onMounted(() => dialog.value?.showModal());
</script>
<template>
  <dialog ref="dialog" aria-label="玩家战报预览" @close="emit('close')" @click.self="dialog?.close()">
    <div class="shell">
      <header><strong>{{ playerName }} 的战报</strong><button autofocus @click="dialog?.close()">返回玩家档案</button></header>
      <ReportView :id="matchId" :perspective-id="playerId" :preview-map="map" />
    </div>
  </dialog>
</template>
<style scoped>
dialog { width:min(1180px,95vw); max-width:95vw; max-height:92vh; padding:0; border:1px solid var(--bax-border); border-radius:var(--bax-radius); background:var(--bax-surface); color:var(--bax-fg); box-shadow:0 20px 80px #0008; }
dialog::backdrop { background:#080c12d9; }
.shell { max-height:90vh; overflow:auto; overscroll-behavior:contain; }
header { position:sticky; top:0; z-index:3; display:flex; align-items:center; justify-content:space-between; padding:12px 18px; background:var(--bax-surface); border-bottom:1px solid var(--bax-border); }
.shell :deep(.card.report) { border:0; margin:0; }
</style>
