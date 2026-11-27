use bax_store::MatchMode;

pub(crate) fn scenario_is_pve(scenario: Option<&str>) -> bool {
    scenario.is_some_and(|s| s.to_ascii_lowercase().ends_with("_skirmish"))
}

pub(crate) fn live_mode(scenario: Option<&str>, has_ai: bool, spectators: bool) -> MatchMode {
    if has_ai || scenario_is_pve(scenario) {
        MatchMode::Pve
    } else if spectators {
        MatchMode::Custom
    } else {
        MatchMode::Unknown
    }
}
