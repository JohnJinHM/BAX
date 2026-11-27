import type { PlayerFight } from "../shared/bindings/PlayerFight";
import type { OpeningUnit } from "../shared/bindings/OpeningUnit";
import type { UnitClass } from "../shared/bindings/UnitClass";

// 效能轴以低、高值标定。单位偏好单独使用一个参考占比。
export interface RadarAxis { key:string; label:string; base:number; low?:number; unit:string }
export const combatAxes: RadarAxis[] = [
  {key:"kills", label:"击毁", low:10, base:50, unit:"单位"},
  {key:"objectives", label:"占领", low:1, base:3, unit:"次"},
  {key:"survival", label:"存活", low:30, base:70, unit:"%"},
  {key:"efficiency", label:"效率", low:200, base:340, unit:"分/分钟"},
];
export const preferenceAxes: (RadarAxis & {key:UnitClass})[] = [
  {key:"recon", label:"侦察", base:20, unit:"%"}, {key:"infantry", label:"步兵", base:28, unit:"%"},
  {key:"ifv", label:"步战", base:6, unit:"%"}, {key:"tank", label:"装甲", base:10, unit:"%"},
  {key:"artillery", label:"火炮", base:6, unit:"%"}, {key:"airDefense", label:"防空", base:8, unit:"%"},
  {key:"helicopter", label:"直升机", base:10, unit:"%"}, {key:"jet", label:"固定翼", base:12, unit:"%"},
];
export function validRadarAxis(axis: RadarAxis) {
  const low = axis.low ?? 0;
  return Number.isFinite(low) && low >= 0 && Number.isFinite(axis.base) && axis.base > low &&
    Number.isFinite(Number.MAX_SAFE_INTEGER / (axis.base-low)) && (axis.unit !== "%" || axis.base <= 100);
}

export function baseKey(axis: RadarAxis) {
  return axis.low == null ? axis.key : axis.key+".high";
}

export function calibratedAxes(axes: RadarAxis[], saved: Record<string, number>) {
  return axes.map(axis => {
    const candidate = {...axis, base:saved[baseKey(axis)] ?? axis.base};
    if (axis.low != null) candidate.low = saved[axis.key+".low"] ?? axis.low;
    return validRadarAxis(candidate) ? candidate : axis;
  });
}

export function referenceValue(axis: RadarAxis) {
  return axis.low == null ? axis.base : axis.low + (axis.base - axis.low) / 2;
}

export function axisRatio(axis: RadarAxis, value: number) {
  const low = axis.low ?? 0;
  return Math.max(0,value-low) / (axis.base-low);
}
const playing = (f: PlayerFight) => f.mode === "ranked" && (f.team === "alpha" || f.team === "bravo");
export function playerAverages(fights: PlayerFight[]) {
  const combat = combatAxes.map(() => 0);
  const preferences = preferenceAxes.map(() => 0);
  let games = 0, preferenceGames = 0, efficiencyGames = 0;
  for (const f of fights.filter(playing)) {
    const total = f.composition.reduce((sum,c) => sum+c.count,0);
    const perf = f.performance;
    if (!total || !perf) continue;
    games++;
    combat[0] += perf.kills;
    combat[1] += perf.objectives;
    combat[2] += 100*Math.max(0,1-perf.losses/total);
    if (perf.spawnedScore != null && perf.spawnedScore >= 0 && f.durationSecs != null && f.durationSecs > 0) {
      combat[3] += perf.spawnedScore * 60 / f.durationSecs;
      efficiencyGames++;
    }
    const counts = preferenceAxes.map(a => f.composition.find(c => c.class === a.key)?.count ?? 0);
    const eligible = counts.reduce((a,b) => a+b,0);
    if (eligible) {
      preferenceGames++;
      counts.forEach((n,i) => { preferences[i] += n*100/eligible; });
    }
  }
  return {
    games, efficiencyGames,
    combat:combat.map((n,i) => {
      const samples = i === 3 ? efficiencyGames : games;
      return samples ? n/samples : null;
    }),
    preferenceGames, preferences:preferences.map(n => preferenceGames ? n/preferenceGames : 0),
  };
}

export function preferredModels(fights: PlayerFight[], opening: boolean) {
  const recent = fights.filter(playing).slice(0,20);
  const samples = recent.map(f => opening ? f.opening : f.deployedUnits.length ? f.deployedUnits : null)
    .filter((units): units is OpeningUnit[] => units != null);
  const counts = new Map<number, { unit: OpeningUnit; games: number }>();
  for (const units of samples) for (const unit of units) {
    const entry = counts.get(unit.id);
    if (entry) entry.games++;
    else counts.set(unit.id, {unit,games:1});
  }
  return { samples:samples.length, top:[...counts.values()].sort((a,b) =>
    b.games-a.games || a.unit.name.localeCompare(b.unit.name) || a.unit.id-b.unit.id).slice(0,5) };
}
