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

/// Capabilities the host actually gates something on today. All five v1 capabilities
/// are enforced as of PD-021 — `effects.manage` was the last one, implemented via a
/// queued-request model rather than giving the plugin host a direct `AudioBackend`
/// reference (see PD-021 in `docs/architecture/Decisions.md`).
pub const ENFORCED: &[&str] = &[
    GRAPH_READ,
    UI_PANEL_REGISTER,
    PROFILE_READ,
    ROUTING_SUGGEST,
    EFFECTS_MANAGE,
];

pub fn is_known(capability: &str) -> bool {
    ALL.contains(&capability)
}

pub fn is_enforced(capability: &str) -> bool {
    ENFORCED.contains(&capability)
}

pub fn describe(capability: &str) -> &'static str {
    match capability {
        GRAPH_READ => "Receive graph.updated notifications",
        ROUTING_SUGGEST => "Return route suggestions (no apply)",
        PROFILE_READ => "Read active profile metadata",
        EFFECTS_MANAGE => "Manage filter chains on pipe-deck-* devices",
        UI_PANEL_REGISTER => "Register a nav panel in the host UI",
        _ => "Unknown capability",
    }
}

pub fn all_metadata() -> Vec<crate::core::models::CapabilityInfo> {
    ALL.iter()
        .map(|capability| crate::core::models::CapabilityInfo {
            id: (*capability).to_string(),
            description: describe(capability).to_string(),
            enforced: is_enforced(capability),
        })
        .collect()
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

    #[test]
    fn all_v1_capabilities_are_enforced() {
        assert!(is_enforced(GRAPH_READ));
        assert!(is_enforced(UI_PANEL_REGISTER));
        assert!(is_enforced(ROUTING_SUGGEST));
        assert!(is_enforced(PROFILE_READ));
        assert!(is_enforced(EFFECTS_MANAGE));
    }

    #[test]
    fn all_metadata_covers_every_known_capability() {
        let metadata = all_metadata();
        assert_eq!(metadata.len(), ALL.len());
        assert!(metadata.iter().all(|info| info.enforced));
    }
}
