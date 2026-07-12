use crate::core::models::{
    Device, DeviceDirection, DeviceKind, DeviceRouteRule, FallbackPolicy, RouteSource, Rule,
    RuleCondition, RuntimeGraph, Stream, StreamDirection, StreamRouteRule,
};
use crate::core::routing_rules::find_device_by_system_name;
use crate::core::rules::CandidateRule;
use crate::pipewire::pw_link;
use regex::Regex;
use std::collections::HashSet;

pub fn default_category(stream: &Stream) -> Option<&'static str> {
    let executable = stream.executable.as_deref().unwrap_or("");
    let app_lower = stream.app_name.to_lowercase();

    if executable.contains("steam") || app_lower.contains("steam") {
        return Some("Game");
    }
    if executable.contains("spotify") || app_lower.contains("spotify") {
        return Some("Music");
    }
    if executable.contains("discord") || app_lower.contains("discord") {
        return Some("Chat");
    }
    if executable.contains("firefox")
        || executable.contains("chromium")
        || executable.contains("chrome")
        || app_lower.contains("firefox")
        || app_lower.contains("chromium")
    {
        return Some("Browser");
    }
    if executable.contains("obs") || app_lower.contains("obs") {
        return Some("Streaming");
    }

    None
}

pub fn stream_matches_persisted_rule(stream: &Stream, rule: &StreamRouteRule) -> Option<Vec<String>> {
    if rule.app_name.is_none() && rule.executable.is_none() && rule.media_name.is_none() {
        return None;
    }

    let mut reasons = Vec::new();

    if let Some(rule_app) = &rule.app_name {
        if !eq_ignore_ascii_case(&stream.app_name, rule_app) {
            return None;
        }
        reasons.push(format!("app_name == {rule_app}"));
    }

    if let Some(rule_exe) = &rule.executable {
        if stream
            .executable
            .as_deref()
            .is_none_or(|executable| !eq_ignore_ascii_case(executable, rule_exe))
        {
            return None;
        }
        reasons.push(format!("executable == {rule_exe}"));
    }

    match (&rule.media_name, &stream.media_name) {
        (Some(rule_media), Some(stream_media)) => {
            if rule_media != stream_media {
                return None;
            }
            reasons.push(format!("media_name == {rule_media}"));
        }
        (Some(_rule_media), None) => {
            return None;
        }
        (None, Some(stream_media)) => {
            reasons.push(format!("media_name == {stream_media} (rule wildcard)"));
        }
        (None, None) => {}
    }

    Some(reasons)
}

pub fn stream_matches_authored_rule(stream: &Stream, rule: &Rule) -> Option<Vec<String>> {
    if !rule.enabled || rule.conditions.is_empty() {
        return None;
    }

    let mut reasons = Vec::new();

    for condition in &rule.conditions {
        match condition {
            RuleCondition::AppName { value } => {
                if !eq_ignore_ascii_case(&stream.app_name, value) {
                    return None;
                }
                reasons.push(format!("app_name == {value}"));
            }
            RuleCondition::Executable { value } => {
                if stream
                    .executable
                    .as_deref()
                    .is_none_or(|executable| !eq_ignore_ascii_case(executable, value))
                {
                    return None;
                }
                reasons.push(format!("executable == {value}"));
            }
            RuleCondition::WindowClass { value } => {
                let Some(window_class) = &stream.window_class else {
                    return None;
                };
                if window_class != value {
                    return None;
                }
                reasons.push(format!("window_class == {value}"));
            }
            RuleCondition::MediaName { value } => {
                if stream.media_name.as_deref() != Some(value.as_str()) {
                    return None;
                }
                reasons.push(format!("media_name == {value}"));
            }
            RuleCondition::Direction { value } => {
                if stream.direction != *value {
                    return None;
                }
                reasons.push(format!("direction == {:?}", value).to_lowercase());
            }
            RuleCondition::Category { value } => {
                let category = default_category(stream)?;
                if !category.eq_ignore_ascii_case(value) {
                    return None;
                }
                reasons.push(format!("category == {value}"));
            }
            RuleCondition::Identity { value } => {
                if !stream_matches_identity(stream, value) {
                    return None;
                }
                reasons.push(format!("identity == {value}"));
            }
            RuleCondition::Regex { field, pattern } => {
                let haystack = regex_field_value(stream, field)?;
                let regex = Regex::new(pattern).ok()?;
                if !regex.is_match(&haystack) {
                    return None;
                }
                reasons.push(format!("{field} matches /{pattern}/"));
            }
        }
    }

    Some(reasons)
}

fn eq_ignore_ascii_case(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn stream_matches_identity(stream: &Stream, value: &str) -> bool {
    if eq_ignore_ascii_case(&stream.app_name, value) {
        return true;
    }
    if stream
        .executable
        .as_deref()
        .is_some_and(|executable| eq_ignore_ascii_case(executable, value))
    {
        return true;
    }
    if stream
        .system_name
        .as_deref()
        .is_some_and(|system_name| eq_ignore_ascii_case(system_name, value))
    {
        return true;
    }
    false
}

fn regex_field_value(stream: &Stream, field: &str) -> Option<String> {
    match field {
        "app_name" => Some(stream.app_name.clone()),
        "executable" => stream.executable.clone(),
        "media_name" => stream.media_name.clone(),
        "window_class" => stream.window_class.clone(),
        _ => None,
    }
}

pub(crate) fn collect_stream_candidates(
    stream: &Stream,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
) -> Vec<CandidateRule> {
    let mut candidates = Vec::new();

    for rule in authored_rules {
        if let Some(reasons) = stream_matches_authored_rule(stream, rule) {
            candidates.push(CandidateRule {
                key: rule.name.clone(),
                rule_id: Some(rule.id.clone()),
                target_system_names: rule.action.target_system_names_resolved(),
                match_reasons: reasons,
                priority: rule.priority,
                source: RouteSource::AuthoredRule,
                fallback_policy: rule.safeguards.fallback_policy.clone(),
            });
        }
    }

    for (index, rule) in persisted_rules.iter().enumerate() {
        if let Some(reasons) = stream_matches_persisted_rule(stream, rule) {
            candidates.push(CandidateRule {
                key: persisted_rule_display_name(),
                rule_id: None,
                target_system_names: rule.target_system_names_resolved(),
                match_reasons: reasons,
                priority: -1_000 - index as i32,
                source: RouteSource::PersistedRule,
                fallback_policy: FallbackPolicy::KeepCurrent,
            });
        }
    }

    candidates.sort_by(|left, right| right.priority.cmp(&left.priority));
    candidates
}

pub(crate) fn find_safe_default_device(graph: &RuntimeGraph, direction: StreamDirection) -> Option<Device> {
    let device_direction = match direction {
        StreamDirection::Playback => DeviceDirection::Output,
        StreamDirection::Capture => DeviceDirection::Input,
    };

    let mut physical = graph
        .devices
        .iter()
        .filter(|device| {
            device.kind == DeviceKind::Physical
                && device.direction == device_direction
                && !device.system_name.starts_with("pipe-deck-feed-")
        })
        .cloned()
        .collect::<Vec<_>>();
    physical.sort_by(|left, right| left.label.cmp(&right.label));
    physical.into_iter().next()
}

pub(crate) fn resolve_target_device(
    graph: &RuntimeGraph,
    stream: &Stream,
    winner: &CandidateRule,
) -> Option<(Device, Option<String>)> {
    for system_name in &winner.target_system_names {
        if let Some(device) = find_device_by_system_name(graph, system_name) {
            return Some((device.clone(), None));
        }
    }

    match winner.fallback_policy {
        FallbackPolicy::KeepCurrent => None,
        FallbackPolicy::SafeDefault => find_safe_default_device(graph, stream.direction.clone()).map(|device| {
            (
                device.clone(),
                Some(format!(
                    "Target unavailable; fell back to safe default ({})",
                    device.label
                )),
            )
        }),
    }
}

fn persisted_rule_display_name() -> String {
    "Manual route".into()
}

pub(crate) fn actual_device_target_system_names(
    graph: &RuntimeGraph,
    source: &crate::core::models::Device,
) -> HashSet<String> {
    let from_graph: HashSet<String> = source
        .resolved_targets()
        .iter()
        .filter_map(|id| {
            graph
                .devices
                .iter()
                .find(|device| device.id == *id)
                .map(|device| device.system_name.clone())
        })
        .collect();
    if !from_graph.is_empty() {
        return from_graph;
    }

    pw_link::list_all_monitor_routes_for_source(&source.system_name)
        .into_iter()
        .collect()
}

pub(crate) fn device_matches_rule(
    graph: &RuntimeGraph,
    source: &crate::core::models::Device,
    rule: &DeviceRouteRule,
) -> bool {
    let expected: HashSet<String> = rule.target_system_names_resolved().into_iter().collect();
    if expected.is_empty() {
        return true;
    }
    actual_device_target_system_names(graph, source) == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Rule, RuleCondition, StreamDirection};

    fn sample_stream(app_name: &str, executable: Option<&str>, media_name: Option<&str>) -> Stream {
        Stream {
            id: "stream-1".into(),
            app_name: app_name.into(),
            executable: executable.map(str::to_string),
            window_class: None,
            system_name: None,
            direction: StreamDirection::Playback,
            current_target: None,
            current_targets: Vec::new(),
            media_name: media_name.map(str::to_string),
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
        }
    }

    #[test]
    fn persisted_rule_matches_executable_only() {
        let stream = sample_stream("Discord Canary", Some("discord"), None);
        let rule = StreamRouteRule {
            app_name: None,
            executable: Some("discord".into()),
            media_name: None,
            target_system_name: Some("chat".into()),
            target_system_names: Vec::new(),
        };

        assert!(stream_matches_persisted_rule(&stream, &rule).is_some());
    }

    #[test]
    fn persisted_rule_requires_all_specified_fields() {
        let stream = sample_stream("Soundux", Some("soundux"), Some("miniaudio"));
        let matching = StreamRouteRule {
            app_name: Some("Soundux".into()),
            executable: None,
            media_name: Some("miniaudio".into()),
            target_system_name: Some("sink".into()),
            target_system_names: Vec::new(),
        };
        let non_matching = StreamRouteRule {
            app_name: Some("Soundux".into()),
            executable: None,
            media_name: Some("other".into()),
            target_system_name: Some("sink".into()),
            target_system_names: Vec::new(),
        };

        assert!(stream_matches_persisted_rule(&stream, &matching).is_some());
        assert!(stream_matches_persisted_rule(&stream, &non_matching).is_none());
    }

    #[test]
    fn authored_category_rule_matches_games() {
        let stream = sample_stream("Steam", Some("steam"), None);
        let rule = Rule {
            id: "game".into(),
            name: "Games".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::Category {
                value: "Game".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("game_sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        };

        assert!(stream_matches_authored_rule(&stream, &rule).is_some());
    }

    #[test]
    fn regex_condition_matches_app_name() {
        let stream = sample_stream("My Custom App", None, None);
        let rule = Rule {
            id: "regex".into(),
            name: "Custom apps".into(),
            enabled: true,
            priority: 5,
            conditions: vec![RuleCondition::Regex {
                field: "app_name".into(),
                pattern: "Custom.*".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("custom_sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        };

        assert!(stream_matches_authored_rule(&stream, &rule).is_some());
    }

    #[test]
    fn identity_matches_app_name_or_executable() {
        let stream = sample_stream("pw-play", None, None);
        let rule = Rule {
            id: "pw-play".into(),
            name: "Player".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::Identity {
                value: "pw-play".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("hdmi".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        };

        assert!(stream_matches_authored_rule(&stream, &rule).is_some());

        let by_exe = sample_stream("Player", Some("pw-play"), None);
        assert!(stream_matches_authored_rule(&by_exe, &rule).is_some());
    }
}
