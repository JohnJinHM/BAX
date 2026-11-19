//! 画面观测与候选账号。候选不参与 Tracker 或持久化比赛名单。
use crate::{ProfileSummary, Snapshot};
use bax_core::{Connection, MatchId, Phase, PlayerId, Timestamp};
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "ocr"))]
use unicode_normalization::UnicodeNormalization;

#[cfg(feature = "ocr")]
pub(crate) mod worker;

pub(crate) struct Message {
    pub ticket: Ticket,
    pub revision: u64,
    pub view: PregameView,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Observation {
    pub side: String,
    pub row: u8,
    /// 由画面队伍标题颜色判定；未知时不发起 OCR 网络查询。
    #[serde(default)]
    pub friendly: Option<bool>,
    pub name: String,
    pub level: i64,
    #[serde(default)]
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Candidate {
    pub id: PlayerId,
    pub name: String,
    pub level: Option<i64>,
    pub local: bool,
    pub profile: Option<ProfileSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PregamePlayer {
    pub observed: Observation,
    /// 最多三个候选；confirmed 永远由日志名单表达。
    pub candidates: Vec<Candidate>,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PregameView {
    pub players: Vec<PregamePlayer>,
    pub status: String,
}

/// 比赛代次由 ID、发现时间以及日志来源标识共同隔离。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ticket {
    pub id: MatchId,
    pub discovered: Timestamp,
    pub file: Option<String>,
}

pub(crate) fn enabled(snapshot: &Snapshot) -> bool {
    cfg!(feature = "ocr")
        && snapshot
            .ui
            .get("ui.ocr")
            .and_then(|v| v.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
}

pub(crate) fn ticket(snapshot: &Snapshot) -> Option<Ticket> {
    let m = snapshot.current.as_ref()?;
    (enabled(snapshot)
        && snapshot.log.ready
        && !m.roster_known
        && !m.rejoined
        && m.connection == Connection::Connected
        && matches!(
            m.phase,
            Phase::WaitingRoom | Phase::Loading | Phase::Deployment
        ))
    .then(|| Ticket {
        id: m.match_id,
        discovered: m.discovered_at,
        file: snapshot.log.file.clone(),
    })
}

#[cfg(any(test, feature = "ocr"))]
pub(crate) fn normalized(name: &str) -> String {
    name.trim().nfc().flat_map(char::to_lowercase).collect()
}

#[cfg(any(test, feature = "ocr"))]
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ac) in a.chars().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (diag + usize::from(ac != *bc)).min(row[j] + 1).min(old + 1);
            diag = old;
        }
    }
    row[b.len()]
}

/// 昵称优先，等级辅助；大小写不作为决定身份的证据。相同 ID 的别名只占一个候选。
#[cfg(any(test, feature = "ocr"))]
pub(crate) fn rank(
    observed: Observation,
    candidates: Vec<Candidate>,
    truncated: bool,
) -> PregamePlayer {
    let name = normalized(&observed.name);
    let mut scored: Vec<_> = candidates
        .into_iter()
        .filter_map(|c| {
            let d = std::iter::once(name.clone())
                .chain(observed.alternatives.iter().map(|a| normalized(a)))
                .map(|n| distance(&n, &normalized(&c.name)))
                .min()
                .unwrap_or(usize::MAX);
            let allowed = if name.chars().count() >= 5 { 1 } else { 0 };
            if c.id.get() == 0 || d > allowed {
                return None;
            }
            let gap = c
                .level
                .filter(|n| *n >= 0)
                .map(|n| n.abs_diff(observed.level))
                .unwrap_or(u64::MAX);
            Some((d, gap, c))
        })
        .collect();
    scored.sort_by_key(|(d, gap, c)| (*d, *gap, !c.local, c.id.get()));
    let mut ids = std::collections::BTreeSet::new();
    scored.retain(|(_, _, c)| ids.insert(c.id));
    let ambiguous = truncated
        || observed.name.contains(['□', '\u{fffd}'])
        || match scored.as_slice() {
            [] => false,
            [(d, gap, _), rest @ ..] => {
                *d > 0
                    || *gap == u64::MAX
                    || rest
                        .first()
                        .is_some_and(|(d2, gap2, _)| d == d2 && gap2.saturating_sub(*gap) < 3)
            }
        };
    PregamePlayer {
        observed,
        candidates: scored.into_iter().take(3).map(|(_, _, c)| c).collect(),
        ambiguous,
    }
}

#[cfg(any(test, feature = "ocr"))]
pub(crate) fn reject_duplicate_candidates(players: &mut [PregamePlayer]) {
    let mut counts = std::collections::BTreeMap::new();
    for p in players.iter() {
        if let Some(c) = p.candidates.first() {
            *counts.entry(c.id).or_insert(0) += 1;
        }
    }
    for p in players {
        if p.candidates.first().is_some_and(|c| counts[&c.id] > 1) {
            p.ambiguous = true;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn saved_ocr_setting_cannot_override_build_capability() {
        let mut snapshot = crate::Snapshot::initial();
        snapshot
            .ui
            .insert("ui.ocr".into(), serde_json::json!({"enabled": true}));
        assert_eq!(super::enabled(&snapshot), cfg!(feature = "ocr"));
        snapshot
            .ui
            .insert("ui.ocr".into(), serde_json::json!({"enabled": false}));
        assert!(!super::enabled(&snapshot));
    }
    use super::*;
    fn obs(name: &str, level: i64) -> Observation {
        Observation {
            side: "left".into(),
            row: 0,
            friendly: Some(false),
            name: name.into(),
            level,
            alternatives: vec![],
        }
    }
    fn c(id: u64, name: &str, level: Option<i64>, local: bool) -> Candidate {
        Candidate {
            id: PlayerId(id),
            name: name.into(),
            level,
            local,
            profile: None,
        }
    }
    #[test]
    fn level_disambiguates_names_but_case_cannot_break_a_tie() {
        let r = rank(
            obs("Raven", 57),
            vec![
                c(1, "Raven", Some(70), false),
                c(2, "raven", Some(57), false),
            ],
            false,
        );
        assert_eq!(r.candidates[0].id, PlayerId(2));
        assert!(!r.ambiguous);
        let r = rank(
            obs("RAVEN", 100),
            vec![
                c(1, "RAVEN", Some(100), false),
                c(2, "raven", Some(100), true),
            ],
            false,
        );
        assert!(r.ambiguous);
    }
    #[test]
    fn aliases_do_not_create_false_ambiguity_and_fuzzy_is_provisional() {
        let r = rank(
            obs("Sakura", 40),
            vec![
                c(1, "SAKURA", Some(40), true),
                c(1, "Sakura", Some(40), true),
            ],
            false,
        );
        assert_eq!(r.candidates.len(), 1);
        assert!(!r.ambiguous);
        assert!(
            rank(
                obs("Sakur", 40),
                vec![c(1, "Sakura", Some(40), true)],
                false
            )
            .ambiguous
        );
        assert!(rank(obs("Raven", 40), vec![c(1, "Raven", None, true)], false).ambiguous);
        assert!(rank(obs("Raven", 40), vec![c(1, "Raven", Some(40), true)], true).ambiguous);
    }
    #[test]
    fn alternative_scripts_remain_equal_evidence() {
        let mut o = obs("copep", 40);
        o.alternatives = vec!["сорер".into()];
        let r = rank(o.clone(), vec![c(1, "сорер", Some(40), true)], false);
        assert_eq!(r.candidates[0].id, PlayerId(1));
        assert!(!r.ambiguous);
        assert!(
            rank(
                o,
                vec![
                    c(1, "сорер", Some(40), true),
                    c(2, "copep", Some(40), false)
                ],
                false
            )
            .ambiguous
        );
    }

    #[test]
    fn unicode_and_duplicate_slot_guards() {
        assert_eq!(normalized("ЕНОТ"), normalized("енот"));
        assert_ne!(normalized("a"), normalized("а"));
        assert_eq!(normalized("é"), normalized("e\u{301}"));
        let p = rank(obs("桜", 40), vec![c(1, "桜", Some(40), true)], false);
        let mut ps = vec![p.clone(), p];
        reject_duplicate_candidates(&mut ps);
        assert!(ps.iter().all(|p| p.ambiguous));
    }
}
