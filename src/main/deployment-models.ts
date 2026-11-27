import type { UnitRow } from "../shared/bindings/UnitRow";
import type { UnitMetrics } from "../shared/bindings/UnitMetrics";

export const modelKey = (id: number | null, name: string) => JSON.stringify([id, name]);

/** Merge equipment variants that have the same resolved model name in the treemap. */
export function deploymentModels(units: UnitRow[]) {
  const models = new Map<string, UnitRow>();
  for (const unit of units) {
    const key = modelKey(unit.unitId, unit.unitName);
    const row = models.get(key);
    if (!row) {
      models.set(key, { ...unit, metrics: { ...unit.metrics } });
      continue;
    }
    for (const field of ["count","kills","dead","refunded","damage","deployedScore","value","transferred"] as const) {
      row[field] += unit[field];
    }
    for (const field of Object.keys(unit.metrics) as (keyof UnitMetrics)[]) {
      row.metrics[field] += unit.metrics[field];
    }
    if (unit.firstSpawnSecs != null) row.firstSpawnSecs = Math.min(row.firstSpawnSecs ?? Infinity, unit.firstSpawnSecs);
    if (unit.lastSpawnSecs != null) row.lastSpawnSecs = Math.max(row.lastSpawnSecs ?? -Infinity, unit.lastSpawnSecs);
  }
  return models;
}
