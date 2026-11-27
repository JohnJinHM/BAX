<script setup lang="ts">
const tags = {
  win: ["胜", "胜利"],
  loss: ["负", "失败"],
  spectator: ["旁", "旁观比赛"],
  cancelled: ["弃", "准备阶段取消"],
  unknown: ["未", "胜负未判定"],
  local: ["本", "本机游戏日志中有这场对局"],
  together: ["同", "你与这名玩家出现在同一场比赛的名单中"],
} as const;
defineProps<{ kind: keyof typeof tags }>();
</script>

<template>
  <span class="match-tag" :class="kind" :title="tags[kind][1]" :aria-label="tags[kind][1]">{{ tags[kind][0] }}</span>
</template>

<style scoped>
.match-tag {
  display: inline-block;
  flex-shrink: 0;
  justify-self: start;
  padding: 0 5px;
  border-radius: min(3px, var(--bax-radius-xs));
  font-size: calc(12px * var(--bax-font-scale));
  font-weight: 600;
  white-space: nowrap;
  color: var(--bax-muted);
  background: color-mix(in srgb, currentColor 15%, transparent);
}
.win { color: var(--bax-good); }
.loss { color: var(--bax-bad); }
.together { color: var(--bax-accent); }
</style>
