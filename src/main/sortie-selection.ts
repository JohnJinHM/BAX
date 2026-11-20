export type Point = { x: number; y: number };
export type SelectionBox = Point & { width: number; height: number };
export type SelectionMark = SelectionBox & { key: number };

export function selectionBox(a: Point, b: Point): SelectionBox {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
}

export function keysInBox(box: SelectionBox, marks: SelectionMark[]) {
  return marks.filter(mark => mark.x < box.x + box.width && mark.x + mark.width > box.x &&
    mark.y < box.y + box.height && mark.y + mark.height > box.y).map(mark => mark.key);
}

export function selectKeys(current: number[], incoming: number[], additive: boolean, toggle = false) {
  if (!additive) return [...new Set(incoming)];
  const result = new Set(current);
  const remove = toggle && incoming.every(key => result.has(key));
  for (const key of incoming) { if (remove) result.delete(key); else result.add(key); }
  return [...result];
}
