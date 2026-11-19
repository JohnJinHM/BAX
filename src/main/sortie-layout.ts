import type { Sortie } from "../shared/bindings/Sortie";

export type Entry = Sortie & { key: number };
export type Cluster = { entries: Entry[]; start: number; end: number };

/** 自动窗口取一个单位标记的屏幕宽度；以批次首条锚定，避免链式吞并整场。 */
export function clusterNearby(entries: Entry[], span: number, plotWidth: number, seconds = 0) {
  return clusterEntries(entries, seconds || span * 76 / Math.max(1, plotWidth));
}

export function unitTiles(entries: Entry[]) {
  const groups = new Map<string, Entry[]>();
  for (const entry of entries) {
    const key = `${entry.variantKey ?? entry.unitId}:${entry.refunded}`;
    const group = groups.get(key) ?? [];
    group.push(entry);
    groups.set(key, group);
  }
  return [...groups.entries()].map(([key, units]) => ({ key, unit: units[0], count: units.length }));
}

export function lifeMinimum(iconWidth: number, refunded: boolean) {
  return iconWidth + 10 + (refunded ? 12 : 0);
}

/** 按首个出兵时间划分批次，避免相邻间隔不断延长一个批次。 */
export function clusterEntries(entries: Entry[], seconds: number): Cluster[] {
  const result: Cluster[] = [];
  for (const entry of [...entries].sort((a, b) => a.spawnSecs - b.spawnSecs || a.key - b.key)) {
    const last = result.at(-1);
    if (last && entry.spawnSecs - last.start <= seconds &&
      (seconds > 0 || (last.entries[0].variantKey ?? last.entries[0].unitId) === (entry.variantKey ?? entry.unitId))) {
      last.entries.push(entry);
      last.end = entry.spawnSecs;
    } else {
      // 同秒同型号可能被其他型号隔开。
      const same = seconds === 0 ? [...result].reverse().find((x) => x.start === entry.spawnSecs && (x.entries[0].variantKey ?? x.entries[0].unitId) === (entry.variantKey ?? entry.unitId)) : null;
      if (same) same.entries.push(entry);
      else result.push({ entries: [entry], start: entry.spawnSecs, end: entry.spawnSecs });
    }
  }
  return result;
}

export function packLanes<T extends { x: number; width: number }>(items: T[]) {
  const ends: number[] = [];
  return [...items].sort((a, b) => a.x - b.x).map((item) => {
    let lane = ends.findIndex((end) => end + 4 <= item.x);
    if (lane < 0) lane = ends.length;
    ends[lane] = item.x + item.width;
    return { ...item, lane };
  });
}

export function inRange(entry: Entry, start: number, end: number) {
  return entry.spawnSecs >= start && entry.spawnSecs <= end;
}

/** 最小宽度同时用于碰撞分层与末端收纳，避免图标在时间轴边缘再次被裁切。 */
export function fitMark(x: number, width: number, plotWidth: number, minimum: number) {
  const minWidth = Math.min(plotWidth, minimum);
  const left = Math.max(0, Math.min(x, plotWidth - minWidth));
  return { x: left, width: Math.min(plotWidth - left, Math.max(minWidth, width)) };
}

/** 回收成本不计入净投入，与战报现有统计口径一致。 */
export function netValue(entries: Entry[]) {
  return entries.reduce((total, entry) => total + (entry.refunded ? 0 : entry.cost), 0);
}

export function timeBins(entries: Entry[], start: number, end: number, step: number) {
  const bins = Array.from({ length: Math.max(1, Math.ceil((end - start) / step)) }, (_, i) => ({
    start: start + i * step, end: Math.min(end, start + (i + 1) * step), entries: [] as Entry[],
  }));
  for (const entry of entries) {
    if (!inRange(entry, start, end)) continue;
    bins[Math.min(bins.length - 1, Math.floor((entry.spawnSecs - start) / step))].entries.push(entry);
  }
  return bins;
}
