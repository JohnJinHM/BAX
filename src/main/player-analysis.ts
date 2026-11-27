import type { PlayerFight } from "../shared/bindings/PlayerFight";
import type { OpeningUnit } from "../shared/bindings/OpeningUnit";
import type { UnitClass } from "../shared/bindings/UnitClass";

// 本机 786 份战报、7,311 个有出兵的玩家场次。各轴独立标定，口径见 battle-analytics.md。
export const combatAxes = [
  {label:"击毁", base:32.895, unit:"个"}, {label:"占领", base:2.44, unit:"次"},
  {label:"存活", base:48.129, unit:"%"}, {label:"补给", base:2030.05, unit:"点"},
];
export const preferenceAxes: {kind: UnitClass; label: string; base: number; unit: string}[] = [
  {kind:"recon", label:"侦察", base:20.039, unit:"%"}, {kind:"infantry", label:"步兵", base:28.549, unit:"%"},
  {kind:"ifv", label:"步战", base:6.8, unit:"%"}, {kind:"tank", label:"装甲", base:8.564, unit:"%"},
  {kind:"artillery", label:"火炮", base:5.904, unit:"%"}, {kind:"airDefense", label:"防空", base:7.692, unit:"%"},
  {kind:"helicopter", label:"直升机", base:10.505, unit:"%"}, {kind:"jet", label:"固定翼", base:11.947, unit:"%"},
];
const playing = (f: PlayerFight) => f.team === "alpha" || f.team === "bravo";
export function playerAverages(fights: PlayerFight[]) {
  const combat = combatAxes.map(() => 0);
  const preferences = preferenceAxes.map(() => 0);
  let games = 0, preferenceGames = 0;
  for (const f of fights.filter(playing)) {
    const total = f.composition.reduce((sum,c) => sum+c.count,0);
    if (!total) continue;
    const perf = f.performance;
    games++;
    combat[0] += perf.kills;
    combat[1] += perf.objectives;
    combat[2] += 100*Math.max(0,1-perf.losses/total);
    combat[3] += perf.suppliedAllies;
    const counts = preferenceAxes.map(a => f.composition.find(c => c.class === a.kind)?.count ?? 0);
    const eligible = counts.reduce((a,b) => a+b,0);
    if (eligible) {
      preferenceGames++;
      counts.forEach((n,i) => { preferences[i] += n*100/eligible; });
    }
  }
  return {
    games, combat:combat.map(n => games ? n/games : 0),
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
