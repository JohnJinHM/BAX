<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { count } from "../shared/format";
import { treemap } from "./treemap-layout";
import type { TreeGroup, TreeTile } from "./treemap-layout";
import UnitIcon from "./UnitIcon.vue";

const props = defineProps<{ groups: TreeGroup[]; label: string; unit: string; drill?: boolean }>();
const emit = defineEmits<{ choose: [group: string, key: string] }>();
const surface = ref<HTMLElement>();
const width = ref(0);
const height = ref(0);
let observer: ResizeObserver;
onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    width.value = entry.contentRect.width;
    height.value = entry.contentRect.height;
  });
  observer.observe(surface.value!);
});
onBeforeUnmount(() => observer.disconnect());
const weighted = computed(() => props.groups.map(g => ({ ...g, value: g.tiles.reduce((n, t) => n + t.value, 0) })));
const layout = computed(() => treemap(weighted.value, width.value, height.value).map(({ item, rect }) => {
  const top = item.label ? Math.min(28, rect.height / 3) : 0;
  const tiles = treemap(item.tiles, rect.width, rect.height - top);
  return { item, rect, top, tiles };
}));
const style = (r: { x: number; y: number; width: number; height: number }) =>
  ({ left: r.x + "px", top: r.y + "px", width: r.width + "px", height: r.height + "px" });
const description = (group: TreeGroup, tile: TreeTile) =>
  [group.label, tile.label, count(tile.value) + " " + props.unit, tile.share].filter(Boolean).join(" · ");
</script>
<template>
  <div class="treemap">
    <div ref="surface" class="surface" :aria-label="label" role="group">
      <p v-if="weighted.every(g => !g.value)" class="empty">{{ unit === "点" ? "暂无补给消耗记录" : "暂无可绘制的出兵分数" }}</p>
      <div v-for="g in layout" :key="g.item.key" class="group" :style="{ ...style(g.rect), '--team': g.item.color }">
        <strong v-if="g.item.label" class="group-name" :style="{height:g.top+'px'}">{{ g.item.label }}</strong>
        <div class="tiles" :style="{ top:g.top+'px' }">
          <component :is="drill ? 'button' : 'div'" v-for="({item,rect}, index) in g.tiles" :key="item.key"
            class="tile" :class="{ small:rect.width < 108 || rect.height < 70, tiny:rect.width < 56 || rect.height < 42 }"
            :style="{...style(rect), '--tone':item.color ?? g.item.color, '--tint': (22 + index % 4 * 6)+'%'}"
            :title="description(g.item,item)" :aria-label="description(g.item,item)" @click="drill && emit('choose',g.item.key,item.key)">
            <UnitIcon v-if="item.icon && rect.width >= 44 && rect.height >= 58" v-bind="item.icon" />
            <span class="tile-name">{{ item.label }}</span>
            <b>{{ count(item.value) }}</b><small v-if="item.share">{{ item.share }}</small>
          </component>
        </div>
      </div>
    </div>
    <details v-if="groups.some(g => g.tiles.length)">
      <summary>查看全部数值</summary>
      <div v-for="g in groups" :key="g.key" class="values">
        <strong v-if="g.label">{{ g.label }}</strong>
        <component :is="drill ? 'button' : 'div'" v-for="t in g.tiles" :key="t.key" class="value-row" @click="drill && emit('choose',g.key,t.key)">
          <span><UnitIcon v-if="t.icon" v-bind="t.icon" />{{ t.label }}</span>
          <b>{{ count(t.value) }} {{ unit }}</b><small v-if="t.share">{{ t.share }}</small>
        </component>
      </div>
    </details>
  </div>
</template>
<style scoped>
.treemap { font-variant-numeric:tabular-nums; }
.surface { position:relative; height:clamp(230px,32vw,360px); background:var(--bax-surface-2); overflow:hidden; }
.group,.tiles,.tile { position:absolute; overflow:hidden; }
.tiles { inset:0; }.group-name { display:block; padding:3px 8px; background:color-mix(in srgb,var(--team) 14%,var(--bax-surface-solid)); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-size:calc(13px * var(--bax-font-scale)); }
.tile { box-sizing:border-box; display:flex; flex-direction:column; justify-content:center; align-items:flex-start; gap:2px; margin:0; padding:6px 10px; border:1px solid var(--bax-surface-solid); border-radius:0; line-height:1.15; color:var(--bax-fg); background:color-mix(in srgb,var(--tone) var(--tint),var(--bax-surface-solid)); }
button.tile {cursor:pointer;}button.tile:hover,button.tile:focus-visible { background:color-mix(in srgb,var(--tone) 45%,var(--bax-surface-solid)); outline:2px solid var(--tone); outline-offset:-3px; z-index:1; }
.tile-name,.tile small,.tile b { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:100%; }
.tile b {font-size:clamp(16px,2vw,26px);font-weight:600;}.tile-name {font-size:calc(14px * var(--bax-font-scale));}.tile small {font-size:calc(12px * var(--bax-font-scale));}
.tile.small {padding:2px 4px;gap:0;}.small b,.small .tile-name {font-size:calc(12px * var(--bax-font-scale));}.tile.tiny {padding:1px 2px;}.tiny b,.tiny .tile-name,.tiny small {font-size:calc(11px * var(--bax-font-scale));}
.empty {position:absolute;inset:0;display:grid;place-content:center;color:var(--bax-muted);}
details {margin:8px 0;}summary {cursor:pointer;color:var(--bax-muted);}
.values {margin-top:8px;}.value-row {width:100%; display:flex; align-items:center; gap:8px; padding:4px 8px; border:0; border-bottom:1px solid var(--bax-border); border-radius:0; background:transparent; color:var(--bax-fg);text-align:left;}
.value-row span {margin-right:auto; display:flex;align-items:center;gap:6px;overflow-wrap:anywhere;}.value-row b,.value-row small {white-space:nowrap;}
</style>
