use crate::{
    ProfileSummary, Snapshot,
    labels::{self, Facts, History, Labels, Member},
    postmatch,
};
use bax_batrace::{
    BatraceClient,
    model::{MatchInfo, MatchReportResponse, UnitsResponse},
};
use bax_core::{Connection, MatchId, Phase, Team, Timestamp, report::UnitKind};
use bax_store::Store;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Request {
    id: MatchId,
    discovered: Timestamp,
    members: Vec<Member>,
}
impl Request {
    pub(crate) fn from_snapshot(s: &Snapshot) -> Option<Self> {
        let m = s.current.as_ref()?;
        if !s.log.ready
            || m.phase == Phase::Ended
            || m.connection != Connection::Connected
            || matches!(m.mode, crate::MatchMode::Pve | crate::MatchMode::Custom)
        {
            return None;
        }
        let recent =
            |p: Option<&ProfileSummary>| p.map(|p| p.recent_matches.clone()).unwrap_or_default();
        let mut members: Vec<_> = if m.roster_known || m.pregame.players.is_empty() {
            m.players
                .iter()
                .filter(|p| p.team != Some(Team::Spectators))
                .map(|p| Member {
                    id: p.id,
                    side: p.team.map(|t| format!("{t:?}")),
                    enemy: m.self_team.is_some_and(|t| {
                        matches!(t, Team::Alpha | Team::Bravo) && p.team.is_some_and(|v| v != t)
                    }),
                    recent: recent(p.profile.as_ref()),
                    name: p.name.clone().unwrap_or_else(|| p.id.to_string()),
                })
                .collect()
        } else {
            m.pregame
                .players
                .iter()
                .filter_map(|p| {
                    let id = p.matched_id()?;
                    let candidate = p.candidates.first()?;
                    Some(Member {
                        id,
                        side: p.observed.friendly.map(|b| b.to_string()),
                        enemy: p.observed.friendly == Some(false),
                        recent: recent(candidate.profile.as_ref()),
                        name: candidate.name.clone(),
                    })
                })
                .collect()
        };
        members.sort_by_key(|m| m.id);
        let duplicates: BTreeSet<_> = members
            .windows(2)
            .filter(|w| w[0].id == w[1].id)
            .map(|w| w[0].id)
            .collect();
        members.retain(|m| !duplicates.contains(&m.id));
        (!members.is_empty()).then_some(Self {
            id: m.match_id,
            discovered: m.discovered_at,
            members,
        })
    }
}
pub(crate) enum Update {
    Labels(u64, Labels),
    Archived(crate::reports::ReportEvent),
}
type FactsMap = BTreeMap<MatchId, Option<Facts>>;
fn histories(
    request: &Request,
    local: &BTreeMap<bax_core::PlayerId, Vec<MatchId>>,
    facts: &FactsMap,
) -> BTreeMap<bax_core::PlayerId, History> {
    request
        .members
        .iter()
        .map(|m| {
            let mut h = History {
                matches: vec![],
                first_three_known: !m.recent.is_empty(),
            };
            let mut seen = BTreeSet::new();
            let mut possible = 0;
            let ids = if m.recent.is_empty() {
                &local[&m.id]
            } else {
                &m.recent
            };
            for id in ids {
                if *id == request.id || !seen.insert(*id) {
                    continue;
                }
                match facts.get(id) {
                    Some(Some(f)) if f.players.contains_key(&m.id) => {
                        h.matches.push(*id);
                        possible += 1
                    }
                    Some(None) => continue,
                    _ => {
                        if h.matches.len() < 3 {
                            h.first_three_known = false
                        }
                        possible += 1
                    }
                }
                if possible == 15 {
                    break;
                }
            }
            (m.id, h)
        })
        .collect()
}
fn local_facts(
    store: &Store,
    id: MatchId,
    catalog: &HashMap<i64, UnitKind>,
) -> Option<Option<Facts>> {
    let raw = store.report(id).ok()??;
    let response: MatchReportResponse = serde_json::from_str(&raw.raw).ok()?;
    Some(ranked_facts(store, id, &response.match_info?, catalog))
}

fn ranked_facts(
    store: &Store,
    id: MatchId,
    info: &MatchInfo,
    catalog: &HashMap<i64, UnitKind>,
) -> Option<Facts> {
    if store.report_mode(id).ok()? != Some(crate::MatchMode::Ranked) {
        return None;
    }
    let mut facts = Facts::ranked(info, catalog)?;
    if (facts.map.is_none() || facts.start.is_none())
        && let Ok(Some(log)) = store.match_record(id)
    {
        facts.map = facts.map.or(log.map);
        facts.start = facts
            .start
            .or_else(|| log.clock.map(|c| c.start_at.as_millis().div_euclid(1000)));
    }
    Some(facts)
}
pub(crate) async fn scan(
    path: PathBuf,
    client: BatraceClient,
    clock: Arc<dyn crate::Clock>,
    req: Request,
    seq: u64,
    tx: mpsc::UnboundedSender<Update>,
    online: bool,
) {
    let Ok(mut store) = Store::open(&path) else {
        return;
    };
    let units = store
        .reference(postmatch::UNITS_KEY)
        .ok()
        .flatten()
        .and_then(|r| serde_json::from_str::<UnitsResponse>(&r.raw).ok());
    let mut catalog = units.as_ref().map(postmatch::catalog).unwrap_or_default();
    let mut local = BTreeMap::new();
    for m in &req.members {
        let ids = if m.recent.is_empty() {
            store
                .recent_player_units(m.id, 100)
                .unwrap_or_default()
                .into_iter()
                .map(|v| v.0)
                .collect()
        } else {
            m.recent.clone()
        };
        local.insert(m.id, ids);
    }
    let ids: BTreeSet<_> = local
        .values()
        .flatten()
        .copied()
        .filter(|id| *id != req.id)
        .collect();
    let mut facts = FactsMap::new();
    let load = |facts: &mut FactsMap, store: &Store, catalog: &HashMap<i64, UnitKind>| {
        let mut visited = BTreeSet::new();
        for list in local.values() {
            let mut possible = 0;
            let mut seen = BTreeSet::new();
            for id in list {
                if *id == req.id || !seen.insert(*id) {
                    continue;
                }
                if visited.insert(*id)
                    && let Some(f) = local_facts(store, *id, catalog)
                {
                    facts.insert(*id, f);
                }
                if !matches!(facts.get(id), Some(None)) {
                    possible += 1;
                }
                if possible >= 15 {
                    break;
                }
            }
        }
    };
    load(&mut facts, &store, &catalog);
    let publish = |facts: &FactsMap| {
        let h = histories(&req, &local, facts);
        let _ = tx.send(Update::Labels(
            seq,
            labels::evaluate(&req.members, &h, facts),
        ));
    };
    publish(&facts);
    if !online || ids.is_empty() {
        return;
    }
    if catalog.is_empty()
        && let Ok(r) = client.units().await
    {
        catalog = postmatch::catalog(&r.data);
        let _ = store.set_reference(postmatch::UNITS_KEY, &r.raw, clock.now());
        load(&mut facts, &store, &catalog);
        publish(&facts);
    }
    // Round robin across players. Only IDs from STB history are fetched.
    let mut attempted = BTreeSet::new();
    for index in 0..req
        .members
        .iter()
        .map(|m| m.recent.len())
        .max()
        .unwrap_or(0)
    {
        let mut changed = false;
        for m in &req.members {
            let Some(&id) = m.recent.get(index) else {
                continue;
            };
            if id == req.id || facts.contains_key(&id) || attempted.contains(&id) {
                continue;
            }
            let ranked_before = m.recent[..index]
                .iter()
                .copied()
                .filter(|i| *i != req.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|i| !matches!(facts.get(i), Some(None)))
                .count();
            if ranked_before >= 15 {
                continue;
            }
            attempted.insert(id);
            if let Some(f) = local_facts(&store, id, &catalog) {
                facts.insert(id, f);
                changed = true;
                continue;
            }
            match client.match_report(id).await {
                Ok(r) => {
                    if r.data.players().any(|(p, _)| p == m.id) {
                        match store.save_report_with_players(
                            id,
                            &r.raw,
                            postmatch::report_meta(&r.data),
                            clock.now(),
                        ) {
                            Ok(players) => {
                                let _ = tx.send(Update::Archived(crate::reports::ReportEvent {
                                    pending: store.pending_reports().unwrap_or_default(),
                                    archived: Some((id, players)),
                                }));
                            }
                            Err(e) => tracing::warn!(error=%e,"标签战报归档失败"),
                        }
                        facts.insert(id, ranked_facts(&store, id, &r.data, &catalog));
                        changed = true;
                    }
                }
                Err(e) if e.is_transient() => {
                    publish(&facts);
                    return;
                }
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
        if changed {
            publish(&facts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::Stats;
    use bax_core::PlayerId;

    #[test]
    fn unresolved_prefix_blocks_air_but_known_custom_games_do_not() {
        let id = PlayerId(1);
        let ranked = || {
            let mut f = Facts::default();
            f.players.insert(id, Stats::default());
            Some(f)
        };
        let member = Member {
            id,
            name: "fixture".into(),
            side: None,
            enemy: false,
            recent: vec![
                MatchId(10),
                MatchId(9),
                MatchId(8),
                MatchId(8),
                MatchId(7),
                MatchId(6),
            ],
        };
        let req = Request {
            id: MatchId(10),
            discovered: Timestamp::from_secs(1),
            members: vec![member],
        };
        let mut facts = [
            (MatchId(8), ranked()),
            (MatchId(7), ranked()),
            (MatchId(6), ranked()),
        ]
        .into();
        let local = BTreeMap::new();
        let result = histories(&req, &local, &facts);
        assert_eq!(result[&id].matches.len(), 3);
        assert!(!result[&id].first_three_known);
        facts.insert(MatchId(9), None);
        assert!(histories(&req, &local, &facts)[&id].first_three_known);
    }
}
