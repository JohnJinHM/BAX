import type { MatchView } from './bindings/MatchView';
import type { PlayerView } from './bindings/PlayerView';
import type { Team } from './bindings/Team';

/** Rendering only: matching and identity authority remain in the Rust engine. */
export type RosterRow = Omit<PlayerView, 'id' | 'name'> & {
  id: number | null;
  name: string;
  key: string;
  provisional: boolean;
  displayLevel: number | null;
};
export interface RosterGroup {
  key: string;
  title: string;
  tone: 'mine' | 'enemy' | 'alpha' | 'bravo' | 'waiting';
  players: RosterRow[];
  averageValue: number | null;
  rated: number;
  count: number;
}
function group(key: string, title: string, tone: RosterGroup['tone'], players: RosterRow[]): RosterGroup {
  const ratings = players.flatMap(p => p.profile?.rating != null ? [p.profile.rating] : []);
  return { key, title, tone, players, count: players.length, rated: ratings.length,
    averageValue: ratings.length ? ratings.reduce((a, b) => a + b, 0) / ratings.length : null };
}
function confirmed(p: PlayerView): RosterRow {
  return { ...p, name: p.name?.trim() || p.logName?.trim() || '资料读取中', key: `player-${p.id}`,
    provisional: false, displayLevel: p.profile?.level ?? null };
}
export function rosterGroups(m: MatchView | null): RosterGroup[] {
  if (!m) return [];
  if (m.rosterKnown) {
    const order: Team[] = m.selfTeam === 'bravo' ? ['bravo', 'alpha', 'spectators'] : ['alpha', 'bravo', 'spectators'];
    return order.map(team => {
      const mine = m.selfTeam === 'alpha' || m.selfTeam === 'bravo';
      const title = team === 'spectators' ? '旁观者' : mine ? (team === m.selfTeam ? '我方' : '敌方') : team === 'alpha' ? 'Alpha' : 'Bravo';
      const tone = team === 'spectators' ? 'waiting' : mine ? (team === m.selfTeam ? 'mine' : 'enemy') : team;
      return group(team, title, tone, m.players.filter(p => p.team === team).map(confirmed));
    }).filter(g => g.count);
  }
  if (m.pregame.players.length) {
    const groups = new Map<string, RosterGroup>();
    for (const p of m.pregame.players) {
      const friendly = p.observed.friendly;
      const key = friendly === true ? 'mine' : friendly === false ? 'enemy' : p.observed.side;
      const title = friendly === true ? '我方' : friendly === false ? '敌方' : p.observed.side === 'left' ? '左侧（队伍待确认）' : '右侧（队伍待确认）';
      if (!groups.has(key)) groups.set(key, group(key, title, friendly == null ? 'waiting' : friendly ? 'mine' : 'enemy', []));
      const candidate = p.ambiguous ? null : p.candidates[0];
      groups.get(key)!.players.push({ id: null, key: `ocr-${p.observed.side}-${p.observed.row}`,
        name: p.observed.name, logName: null, team: null, isSelf: false, provisional: true,
        displayLevel: p.observed.level, profile: candidate?.profile ?? null, profileFetchedAt: null,
        profileCurrent: false, fetch: { state: 'idle' }, note: null,
        encounters: { total: 0, asTeammate: 0, asOpponent: 0, firstAt: null, lastAt: null } });
    }
    return [...groups.values()].sort((a,b) => (a.key === 'mine' ? -1 : a.key === 'enemy' ? 1 : 0) - (b.key === 'mine' ? -1 : b.key === 'enemy' ? 1 : 0))
      .map(g => group(g.key, g.title, g.tone, g.players));
  }
  return m.players.length ? [group('waiting', '等待室（队伍待确认）', 'waiting', m.players.map(confirmed))] : [];
}
