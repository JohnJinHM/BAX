<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { onAccent, parseCssColor, toHex } from "../shared/appearance";
import { count } from "../shared/format";
import { teamColumns, treemap } from "./treemap-layout";
import type { TreeGroup, TreeTile } from "./treemap-layout";
import UnitIcon from "./UnitIcon.vue";

const props = defineProps<{ groups: TreeGroup[]; label: string; unit: string; path: string[]; viewKey: string; detail?: boolean; teamView?: boolean }>();
const emit = defineEmits<{ choose: [group: string, key: string]; navigate: [depth: number] }>();
const surface = ref<HTMLElement>();
const backButton = ref<HTMLButtonElement>();
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
const depth = computed(() => props.path.length - 1);
const layout = computed(() => (props.teamView ? teamColumns : treemap)(weighted.value, width.value, height.value).map(({ item, rect }) => {
  const top = item.label ? Math.min(28, rect.height / 3) : 0;
  return { item, rect, top, tiles: treemap(item.tiles, rect.width, rect.height - top) };
}));
type Box = { x: number; y: number; width: number; height: number };
const style = (r: Box) => ({ left:r.x+"px", top:r.y+"px", width:r.width+"px", height:r.height+"px" });
function paint(color: string, index: number) {
  const rgb = parseCssColor(color);
  if (!rgb) return { backgroundColor:color };
  const peak = Math.max(rgb[0],rgb[1],rgb[2]);
  const shade = [.78,.64,.86,.69][index % 4];
  const channel = (n: number) => Math.max(0, peak - (peak - n) * 1.65) * shade;
  const fill = toHex([channel(rgb[0]),channel(rgb[1]),channel(rgb[2]),1]);
  return { backgroundColor:fill, color:onAccent(fill) };
}
const description = (group: TreeGroup, tile: TreeTile) =>
  [group.label, tile.label, count(tile.value) + " " + props.unit, tile.share].filter(Boolean).join(" · ");

// Store normalized coordinates so returning still lands on the tile after a resize.
type Visit = { group: string; key: string; box: Box };
const visits: Visit[] = [];
let origin: Box = { x:.25, y:.25, width:.5, height:.5 };
let forward = true;
function choose(group: string, key: string, event: MouseEvent) {
  const parent = layout.value.find(g => g.item.key === group);
  const tile = parent?.tiles.find(t => t.item.key === key);
  origin = parent && tile ? {
    x:(parent.rect.x+tile.rect.x)/width.value, y:(parent.rect.y+parent.top+tile.rect.y)/height.value,
    width:tile.rect.width/width.value, height:tile.rect.height/height.value,
  } : {x:.25,y:.25,width:.5,height:.5};
  visits.length = depth.value;
  visits[depth.value] = { group,key,box:origin };
  forward = true;
  emit("choose",group,key);
  if (event.detail === 0) void nextTick(() => backButton.value?.focus({preventScroll:true}));
}
function navigate(target: number) {
  if (target < 0 || target >= depth.value) return;
  const visit = visits[target];
  origin = visit?.box ?? {x:0,y:0,width:1,height:1};
  visits.length = target;
  forward = false;
  emit("navigate",target);
  void nextTick(() => {
    const targets = surface.value?.querySelectorAll<HTMLButtonElement>(".panel:not([inert]) .tile");
    const tile = Array.from(targets ?? []).find(b => b.dataset.group === visit?.group && b.dataset.key === visit?.key);
    (tile ?? surface.value)?.focus({preventScroll:true});
  });
}
function back() {
  let target = visits.length - 1;
  while (target > 0 && !visits[target]) target--;
  navigate(Math.max(0,target));
}
function escape(event: KeyboardEvent) {
  if (!depth.value) return;
  event.preventDefault();
  event.stopPropagation();
  back();
}
function animate(el: Element, entering: boolean, done: () => void) {
  const node = el as HTMLElement;
  node.inert = !entering;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) { done(); return; }
  const zoom = "translate("+origin.x*100+"%,"+origin.y*100+"%) scale("+Math.max(.015,origin.width)+","+Math.max(.015,origin.height)+")";
  const initial = forward ? zoom : "none";
  const end = forward ? "none" : zoom;
  // Fade the labels before zooming. Tile outlines travel with their color fields.
  const frames = entering ? [
    { offset:0, transform:initial, opacity:0, "--bax-tree-ink":"0" },
    { offset:.2, transform:initial, opacity:0, "--bax-tree-ink":"0", easing:"cubic-bezier(.2,.75,.25,1)" },
    { offset:.8, transform:"none", opacity:1, "--bax-tree-ink":"0" },
    { offset:1, transform:"none", opacity:1, "--bax-tree-ink":"1" },
  ] : [
    { offset:0, transform:"none", opacity:1, "--bax-tree-ink":"1" },
    { offset:.2, transform:"none", opacity:1, "--bax-tree-ink":"0", easing:"cubic-bezier(.2,.75,.25,1)" },
    { offset:.8, transform:end, opacity:0, "--bax-tree-ink":"0" },
    { offset:1, transform:end, opacity:0, "--bax-tree-ink":"0" },
  ];
  node.style.zIndex = entering === forward ? "2" : "1";
  const animation = node.animate(frames,{duration:320});
  animation.onfinish = done;
  animation.oncancel = done;
}
</script>
<template>
  <div class="treemap" @keydown.esc="escape">
    <nav :aria-label="label+'层级'">
      <button ref="backButton" type="button" class="back" :disabled="depth === 0" aria-label="返回上一级" @click="back">
        <svg viewBox="0 0 20 20" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.75" aria-hidden="true"><path d="M16 10H4m6-6-6 6 6 6"/></svg>返回
      </button>
      <ol class="path" aria-label="当前位置">
        <li v-for="(part, index) in path" :key="index">
          <span v-if="index" aria-hidden="true">›</span>
          <a v-if="index < depth" href="#" @click.prevent="navigate(index)">{{ part }}</a>
          <span v-else aria-current="page">{{ part }}</span>
        </li>
      </ol>
      <b class="total"><slot name="total" /></b>
    </nav>
    <div ref="surface" class="surface" :aria-label="label" role="group" tabindex="-1">
      <Transition :css="false" @enter="(el,done) => animate(el,true,done)" @leave="(el,done) => animate(el,false,done)">
        <div :key="viewKey" class="panel" :class="{ 'detail-panel': detail }">
          <slot v-if="detail" name="detail" />
          <template v-else>
            <p v-if="weighted.every(g => !g.value)" class="empty">{{ unit === "kg" ? "暂无补给消耗记录" : "暂无可绘制的出兵分数" }}</p>
            <div v-for="g in layout" :key="g.item.key" class="group" :style="{ ...style(g.rect), '--team':g.item.color }">
              <strong v-if="g.item.label" class="group-name" :style="{height:g.top+'px'}"><span>{{ g.item.label }}</span><span v-if="teamView" class="group-total">{{ count(g.item.total ?? g.item.value) }} {{ unit }}</span></strong>
              <div class="tiles" :style="{top:g.top+'px'}">
                <button v-for="({item,rect},index) in g.tiles" :key="item.key" type="button" class="tile"
                  :class="{small:rect.width < 108 || rect.height < 70, tiny:rect.width < 56 || rect.height < 42}"
                  :style="{...style(rect), ...paint(item.color ?? g.item.color,index)}" :data-group="g.item.key" :data-key="item.key"
                  :title="description(g.item,item)" :aria-label="description(g.item,item)" @click="choose(g.item.key,item.key,$event)">
                  <UnitIcon v-if="item.icon && rect.width >= 44 && rect.height >= 58" v-bind="item.icon" />
                  <span class="tile-name">{{ item.label }}</span><b>{{ count(item.value) }} {{ unit }}</b><small v-if="item.share">{{ item.share }}</small>
                </button>
              </div>
            </div>
          </template>
        </div>
      </Transition>
    </div>
    <details v-if="!detail && groups.some(g => g.tiles.length)" :key="viewKey">
      <summary>查看全部数值</summary>
      <div v-for="g in groups" :key="g.key" class="values">
        <strong v-if="g.label">{{ g.label }}<template v-if="teamView"> · {{ count(g.total ?? g.tiles.reduce((n,t) => n+t.value,0)) }} {{ unit }}</template></strong>
        <button v-for="t in g.tiles" :key="t.key" type="button" class="value-row" @click="choose(g.key,t.key,$event)">
          <span><UnitIcon v-if="t.icon" v-bind="t.icon" />{{ t.label }}</span>
          <b>{{ count(t.value) }} {{ unit }}</b><small v-if="t.share">{{ t.share }}</small>
        </button>
      </div>
    </details>
  </div>
</template>
<style>
@property --bax-tree-ink {syntax:"<number>";inherits:true;initial-value:1;}
</style>
<style scoped>
.treemap {font-variant-numeric:tabular-nums;}
nav {display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin:8px 0;}
.back {display:inline-flex;align-items:center;gap:4px;padding:4px 8px;min-height:30px;}
.path {list-style:none;margin:0;padding:0;display:flex;align-items:center;gap:6px;flex-wrap:wrap;overflow-wrap:anywhere;}
.path li {display:inline-flex;align-items:baseline;gap:6px;}.path a {color:var(--bax-accent);text-underline-offset:3px;}
.tile > *, .group-name > *, .empty {opacity:var(--bax-tree-ink);}
.total {margin-left:auto;}
.surface {position:relative;height:clamp(280px,34vw,420px);background:var(--bax-surface-2);overflow:hidden;isolation:isolate;}
.panel {position:absolute;inset:0;transform-origin:top left;background:var(--bax-surface-2);}
.panel[inert] {pointer-events:none;}.detail-panel {overflow:auto;}
.group,.tiles,.tile {position:absolute;overflow:hidden;}.tiles {inset:0;}
.group-name {display:flex;align-items:center;justify-content:space-between;gap:4px;padding:3px 8px;border-left:3px solid var(--team);background:color-mix(in srgb,var(--team) 24%,var(--bax-surface-solid));white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:calc(13px * var(--bax-font-scale));}
.group-name span {overflow:hidden;text-overflow:ellipsis;}.group-total {flex:none;}
.tile {box-sizing:border-box;display:flex;flex-direction:column;justify-content:center;align-items:flex-start;gap:2px;margin:0;padding:6px 10px;border:1px solid var(--bax-surface-solid);border-radius:0;line-height:1.15;cursor:pointer;}
.tile:hover,.tile:focus-visible {outline:2px solid currentColor;outline-offset:-3px;z-index:1;}
.tile-name,.tile small,.tile b {overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:100%;}
.tile b {font-size:clamp(16px,2vw,26px);font-weight:600;}.tile-name {font-size:calc(14px * var(--bax-font-scale));}.tile small {font-size:calc(12px * var(--bax-font-scale));}
.tile.small {padding:2px 4px;gap:0;}.small b,.small .tile-name {font-size:calc(12px * var(--bax-font-scale));}.tile.tiny {padding:1px 2px;}.tiny b,.tiny .tile-name,.tiny small {font-size:calc(11px * var(--bax-font-scale));}
.empty {position:absolute;inset:0;display:grid;place-content:center;color:var(--bax-muted);}
details {margin:8px 0;}summary {cursor:pointer;color:var(--bax-muted);}
.values {margin-top:8px;}.value-row {width:100%;display:flex;align-items:center;gap:8px;padding:4px 8px;border:0;border-bottom:1px solid var(--bax-border);border-radius:0;background:transparent;color:var(--bax-fg);text-align:left;}
.value-row span {margin-right:auto;display:flex;align-items:center;gap:6px;overflow-wrap:anywhere;}.value-row b,.value-row small {white-space:nowrap;}
</style>
