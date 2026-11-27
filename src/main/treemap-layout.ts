import type { UnitClass } from "../shared/bindings/UnitClass";

export interface TreeTile {
  key: string;
  label: string;
  value: number;
  color?: string;
  share?: string;
  icon?: { id: number; name: string; kind: UnitClass; thumbnail?: string | null };
}
export interface TreeGroup {
  key: string;
  label?: string;
  color: string;
  tiles: TreeTile[];
}
interface Rect { x: number; y: number; width: number; height: number }

/** Bisect the longer side near half the remaining weight. All leaves retain their exact area. */
export function treemap<T extends { value: number }>(items: T[], width: number, height: number) {
  const sorted = items.filter(v => v.value > 0).sort((a, b) => b.value - a.value);
  const output: { item: T; rect: Rect }[] = [];
  function place(start: number, end: number, sum: number, rect: Rect) {
    if (end - start === 1) { output.push({ item: sorted[start], rect }); return; }
    let split = start + 1;
    let part = sorted[start].value;
    while (split < end - 1 && Math.abs(sum / 2 - part - sorted[split].value) < Math.abs(sum / 2 - part)) {
      part += sorted[split++].value;
    }
    const horizontal = rect.width >= rect.height;
    const axis = horizontal ? "width" : "height";
    const offset = horizontal ? "x" : "y";
    const length = rect[axis] * part / sum;
    place(start, split, part, { ...rect, [axis]: length });
    place(split, end, sum - part, { ...rect, [offset]: rect[offset] + length, [axis]: rect[axis] - length });
  }
  if (sorted.length && width > 0 && height > 0) {
    place(0, sorted.length, sorted.reduce((n, v) => n + v.value, 0), { x: 0, y: 0, width, height });
  }
  return output;
}
