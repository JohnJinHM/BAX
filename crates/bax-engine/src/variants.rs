//! 用 BA-Units 的游戏数据库解释战报明确给出的选项；不补猜默认配置。
use std::{collections::HashMap, sync::LazyLock};

use bax_core::report::UnitAppearance;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Unit {
    id: i64,
    name: String,
    thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OptionRow {
    id: i64,
    unit: i64,
    r#mod: i64,
    order: i64,
    replace: i64,
    name: Option<String>,
    suffix: Option<String>,
    thumbnail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Data {
    units: Vec<Unit>,
    options: Vec<OptionRow>,
}

static DATA: LazyLock<Data> = LazyLock::new(|| {
    serde_json::from_str(include_str!("unit-variants.json")).expect("内置改型目录")
});
static UNITS: LazyLock<HashMap<i64, &'static Unit>> =
    LazyLock::new(|| DATA.units.iter().map(|u| (u.id, u)).collect());
static OPTIONS: LazyLock<HashMap<i64, &'static OptionRow>> =
    LazyLock::new(|| DATA.options.iter().map(|o| (o.id, o)).collect());

pub(crate) fn resolve(id: i64, option_ids: &[i64]) -> Option<UnitAppearance> {
    let base = UNITS.get(&id)?;
    let mut chosen: Vec<_> = option_ids
        .iter()
        .filter_map(|id| OPTIONS.get(id).copied())
        .collect();
    // 选项必须属于这个基础单位；未知或版本不匹配时保留基础名称。
    chosen.retain(|o| o.unit == id);
    chosen.sort_by_key(|o| (o.order, o.r#mod, o.id));
    chosen.dedup_by_key(|o| o.id);
    let mut unit = *base;
    for o in &chosen {
        if let Some(replacement) = UNITS.get(&o.replace) {
            unit = replacement;
        }
    }
    let mut name = unit.name.clone();
    let mut thumbnail = unit.thumbnail.clone();
    for o in chosen {
        if let Some(replacement) = o.name.as_ref().filter(|s| !s.is_empty()) {
            name.clone_from(replacement);
        } else if let Some(suffix) = &o.suffix {
            name.push_str(suffix);
        }
        if o.thumbnail.is_some() {
            thumbnail.clone_from(&o.thumbnail);
        }
    }
    Some(UnitAppearance { name, thumbnail })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_options_resolve_suffix_replacement_and_thumbnail() {
        assert_eq!(resolve(3, &[]).expect("BMP").name, "BMP-3");
        assert_eq!(resolve(3, &[470, 470]).expect("BMP").name, "BMP-3M");
        let epokha = resolve(3, &[471]).expect("BMP");
        assert_eq!(epokha.name, "BMP-3M Epokha");
        assert_eq!(
            epokha.thumbnail.as_deref(),
            Some("RU_BMP3_EPOKHA-Label.webp")
        );
        assert_eq!(resolve(130, &[1840]).expect("SSO").name, UNITS[&406].name);
        assert_eq!(resolve(3, &[413, 999_999]).expect("基础单位").name, "BMP-3");
        assert!(resolve(999_999, &[470]).is_none());
    }

    #[test]
    fn option_order_does_not_change_the_label() {
        for unit in &DATA.units {
            let ids: Vec<_> = DATA
                .options
                .iter()
                .filter(|o| o.unit == unit.id)
                .map(|o| o.id)
                .collect();
            let reversed: Vec<_> = ids.iter().rev().copied().collect();
            assert_eq!(resolve(unit.id, &ids), resolve(unit.id, &reversed));
        }
    }
}
