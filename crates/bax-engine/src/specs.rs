//! 从实际出兵约束求唯一专精组合，按已有战报统计场次。
use crate::ProfileSummary;
use bax_batrace::model::{MatchInfo, MatchReportResponse};
use bax_core::PlayerId;
use bax_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SpecPreference {
    pub ids: Vec<u32>,
    pub name: String,
    pub games: u32,
}

#[derive(Deserialize)]
struct Catalog {
    countries: BTreeMap<u32, u32>,
    units: BTreeMap<i64, Vec<u32>>,
}
static CATALOG: LazyLock<Catalog> =
    LazyLock::new(|| serde_json::from_str(include_str!("spec-units.json")).expect("专精单位表"));
static NAMES: LazyLock<BTreeMap<u32, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("specializations.json")).expect("专精名称表")
});

fn entry(ids: Vec<u32>, games: u32) -> SpecPreference {
    let name = ids
        .iter()
        .map(|id| {
            NAMES
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("专精 #{id}"))
        })
        .collect::<Vec<_>>()
        .join(" + ");
    SpecPreference { ids, name, games }
}

pub(crate) fn infer(info: &MatchInfo, player: PlayerId) -> Option<Vec<u32>> {
    let (key, data) = info.data.iter().find(|(key, data)| {
        data.id
            .map(|v| v as u64 == player.get())
            .unwrap_or_else(|| *key == &player.get().to_string())
    })?;
    let received: BTreeSet<_> = info
        .data
        .iter()
        .filter(|(id, _)| *id != key)
        .flat_map(|(_, p)| p.transferred_units.keys())
        .collect();
    let units: BTreeSet<_> = data
        .unit_data
        .iter()
        .filter(|(k, _)| !received.contains(k))
        .filter_map(|(_, u)| u.id)
        .chain(data.transferred_units.values().filter_map(|u| u.id))
        .collect();
    infer_units(&units, &CATALOG)
}

fn infer_units(units: &BTreeSet<i64>, catalog: &Catalog) -> Option<Vec<u32>> {
    let constraints: Vec<_> = units
        .iter()
        .filter_map(|id| catalog.units.get(id))
        .collect();
    if constraints.is_empty() {
        return None;
    }
    let mut candidates = vec![];
    for (&a, country) in &catalog.countries {
        for (&b, other_country) in catalog.countries.range((a + 1)..) {
            if country == other_country
                && constraints
                    .iter()
                    .all(|specs| specs.contains(&a) || specs.contains(&b))
            {
                candidates.push(vec![a, b]);
            }
        }
    }
    (candidates.len() == 1).then(|| candidates.remove(0))
}

pub(crate) fn enrich(profile: &mut ProfileSummary, store: &Store, id: PlayerId) {
    let Ok(reports) = store.recent_player_reports(id, 20) else {
        return;
    };
    let mut counts = BTreeMap::<Vec<u32>, u32>::new();
    let mut pairs = BTreeMap::<Vec<u32>, u32>::new();
    profile.favorite_specs.clear();
    profile.favorite_spec_pairs.clear();
    profile.last_specs.clear();
    profile.last_spec_match = reports.first().map(|r| r.0);
    profile.spec_sample_matches = reports.len() as u32;
    profile.spec_identified_matches = 0;
    for (index, (_, raw)) in reports.into_iter().enumerate() {
        let pair = serde_json::from_str::<MatchReportResponse>(&raw)
            .ok()
            .and_then(|r| r.match_info)
            .and_then(|r| infer(&r, id));
        let Some(pair) = pair else {
            continue;
        };
        profile.spec_identified_matches += 1;
        if index == 0 {
            profile.last_specs = pair.iter().map(|&id| entry(vec![id], 1)).collect();
        }
        for &id in &pair {
            *counts.entry(vec![id]).or_default() += 1;
        }
        *pairs.entry(pair).or_default() += 1;
    }
    let rank = |counts: BTreeMap<Vec<u32>, u32>| {
        let mut entries: Vec<_> = counts
            .into_iter()
            .map(|(ids, games)| entry(ids, games))
            .collect();
        entries.sort_by(|a, b| b.games.cmp(&a.games).then_with(|| a.ids.cmp(&b.ids)));
        entries
    };
    profile.favorite_specs = rank(counts);
    profile.favorite_spec_pairs = rank(pairs);
}

pub(crate) fn from_store(raw: Option<&str>, store: &Store, id: PlayerId) -> Option<ProfileSummary> {
    let mut profile = raw.and_then(ProfileSummary::from_json)?;
    enrich(&mut profile, store, id);
    Some(profile)
}

pub(crate) fn copy_usage(old: &ProfileSummary, new: &mut ProfileSummary) {
    new.favorite_specs = old.favorite_specs.clone();
    new.favorite_spec_pairs = old.favorite_spec_pairs.clone();
    new.last_specs = old.last_specs.clone();
    new.last_spec_match = old.last_spec_match;
    new.spec_sample_matches = old.spec_sample_matches;
    new.spec_identified_matches = old.spec_identified_matches;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_unique_compatible_pairs_are_counted() {
        let catalog = Catalog {
            countries: [(1, 1), (2, 1), (3, 1), (4, 2)].into(),
            units: [(10, vec![1]), (20, vec![2]), (30, vec![3]), (40, vec![4])].into(),
        };
        assert_eq!(
            infer_units(&[10, 20, 999].into(), &catalog),
            Some(vec![1, 2])
        );
        assert_eq!(infer_units(&[10].into(), &catalog), None);
        assert_eq!(infer_units(&[10, 20, 30].into(), &catalog), None);
        assert_eq!(infer_units(&[10, 40].into(), &catalog), None);
        assert_eq!(infer_units(&[999].into(), &catalog), None);
    }
    #[test]
    fn transfers_belong_to_original_deployer() {
        let unit_for = |spec| {
            *CATALOG
                .units
                .iter()
                .find(|(_, ids)| ids.as_slice() == [spec])
                .expect("专属单位")
                .0
        };
        let report = serde_json::json!({"Data": {
            "101": {"Id":101,"UnitData":{"a":{"Id":unit_for(12)}},"TransferredUnits":{"b":{"Id":unit_for(15)}}},
            "102": {"Id":102,"UnitData":{"b":{"Id":unit_for(15)},"c":{"Id":unit_for(3)},"d":{"Id":unit_for(4)}}}
        }});
        let report: MatchInfo = serde_json::from_value(report).expect("report");
        assert_eq!(infer(&report, PlayerId(101)), Some(vec![12, 15]));
        assert_eq!(infer(&report, PlayerId(102)), Some(vec![3, 4]));
    }

    #[test]
    fn history_counts_unique_pairs_and_does_not_use_an_older_pair_as_latest() {
        use bax_core::{MatchId, Timestamp};
        let unit_for = |spec| {
            *CATALOG
                .units
                .iter()
                .find(|(_, ids)| ids.as_slice() == [spec])
                .expect("专属单位")
                .0
        };
        let mut store = Store::open_in_memory().expect("store");
        for (id, units) in [
            (900001, vec![unit_for(12), unit_for(15)]),
            (900002, vec![unit_for(12), unit_for(8)]),
            (900003, vec![unit_for(12)]),
        ] {
            let units: BTreeMap<_, _> = units
                .into_iter()
                .enumerate()
                .map(|(i, id)| (i.to_string(), serde_json::json!({"Id":id})))
                .collect();
            let raw = serde_json::json!({"matchInfo":{"StartTime":id,"Data":{"101":{"Id":101,"UnitData":units}}}}).to_string();
            let report: MatchReportResponse = serde_json::from_str(&raw).expect("json");
            store
                .save_report(
                    MatchId(id),
                    &raw,
                    crate::postmatch::report_meta(report.match_info.as_ref().expect("info")),
                    Timestamp::from_millis(10),
                )
                .expect("save");
        }
        let mut summary = ProfileSummary::from_json(
            r#"{"info":{"id":101},"statInfo":{"specExp":{"3":320000,"4":320000}}}"#,
        )
        .expect("profile");
        enrich(&mut summary, &store, PlayerId(101));
        assert_eq!(summary.spec_sample_matches, 3);
        assert_eq!(summary.spec_identified_matches, 2);
        assert_eq!(summary.favorite_specs[0].ids, vec![12]);
        assert_eq!(summary.favorite_specs[0].games, 2);
        assert!(summary.last_specs.is_empty());
        assert_eq!(summary.last_spec_match, Some(MatchId(900003)));
        assert_eq!(
            store
                .recent_player_reports(PlayerId(101), 2)
                .expect("query")
                .len(),
            2
        );
        assert!(
            store
                .recent_player_reports(PlayerId(999), 20)
                .expect("query")
                .is_empty()
        );
    }
}
