pub const GRAPH_READ: &str = "graph.read";
pub const ROUTING_SUGGEST: &str = "routing.suggest";
pub const PROFILE_READ: &str = "profile.read";
pub const EFFECTS_MANAGE: &str = "effects.manage";
pub const UI_PANEL_REGISTER: &str = "ui.panel.register";

pub const ALL: &[&str] = &[
    GRAPH_READ,
    ROUTING_SUGGEST,
    PROFILE_READ,
    EFFECTS_MANAGE,
    UI_PANEL_REGISTER,
];

pub fn is_known(capability: &str) -> bool {
    ALL.contains(&capability)
}

pub fn is_granted(granted: &[String], capability: &str) -> bool {
    granted.iter().any(|entry| entry == capability)
}

pub fn filter_granted(requested: &[String], granted: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|cap| is_granted(granted, cap))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_to_granted_subset() {
        let requested = vec!["graph.read".into(), "effects.manage".into()];
        let granted = vec!["graph.read".into()];
        assert_eq!(filter_granted(&requested, &granted), vec!["graph.read"]);
    }

    #[test]
    fn rejects_unknown_capability_names_in_grant_check() {
        assert!(!is_known("routing.apply"));
        assert!(is_known("graph.read"));
    }
}
