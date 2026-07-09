use crate::config::store::ConfigStore;
use crate::core::models::{
    ActionStatus, DeviceDirection, DeviceKind, DeviceRouteRule, RouteExplanation,
    RouteSource, Rule, RuleCondition, RoutingRulesConfig, RuntimeGraph, SkippedCandidate,
    SimulationResult, Stream, StreamRouteRule,
};
use crate::core::routing_rules::find_device_by_system_name;
use crate::core::stream_identity::{identity_matches, stream_identity_key};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pw_link;
use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ApplyRulesContext<'a> {
    pub manual_overrides: &'a HashSet<crate::core::stream_identity::StreamIdentityKey>,
    pub device_manual_overrides: &'a HashSet<String>,
    pub dry_run: bool,
    pub mock_graph_only: bool,
}

#[derive(Debug, Clone)]
struct CandidateRule {
    key: String,
    rule_id: Option<String>,
    target_system_names: Vec<String>,
    match_reasons: Vec<String>,
    priority: i32,
    source: RouteSource,
}

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
        if stream.app_name != *rule_app {
            return None;
        }
        reasons.push(format!("app_name == {rule_app}"));
    }

    if let Some(rule_exe) = &rule.executable {
        if stream.executable.as_deref() != Some(rule_exe.as_str()) {
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
                if stream.app_name != *value {
                    return None;
                }
                reasons.push(format!("app_name == {value}"));
            }
            RuleCondition::Executable { value } => {
                if stream.executable.as_deref() != Some(value.as_str()) {
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

fn regex_field_value(stream: &Stream, field: &str) -> Option<String> {
    match field {
        "app_name" => Some(stream.app_name.clone()),
        "executable" => stream.executable.clone(),
        "media_name" => stream.media_name.clone(),
        "window_class" => stream.window_class.clone(),
        _ => None,
    }
}

fn collect_stream_candidates(
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
            });
        }
    }

    candidates.sort_by(|left, right| right.priority.cmp(&left.priority));
    candidates
}

fn persisted_rule_display_name() -> String {
    "Manual route".into()
}

pub fn should_track_manual_override(
    stream: &Stream,
    target_system_name: &str,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
) -> bool {
    let explanation =
        evaluate_stream_route(stream, authored_rules, persisted_rules, &HashSet::new());
    match explanation.target_system_name.as_deref() {
        Some(rule_target) => rule_target != target_system_name,
        None => false,
    }
}

pub fn detect_external_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<crate::core::stream_identity::StreamIdentityKey>,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
) {
    for stream in &graph.streams {
        if stream.is_system {
            continue;
        }
        let Some(current_target_id) = &stream.current_target else {
            continue;
        };
        let Some(device) = graph
            .devices
            .iter()
            .find(|device| device.id == *current_target_id)
        else {
            continue;
        };

        if should_track_manual_override(
            stream,
            &device.system_name,
            authored_rules,
            persisted_rules,
        ) {
            overrides.insert(stream_identity_key(stream));
        }
    }
}

fn actual_device_target_system_names(
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

fn device_matches_rule(
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

pub fn detect_external_device_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<String>,
    device_rules: &[DeviceRouteRule],
) {
    for rule in device_rules {
        let Some(source) = find_device_by_system_name(graph, &rule.source_system_name) else {
            continue;
        };
        if source.kind != DeviceKind::Virtual || source.direction != DeviceDirection::Output {
            continue;
        }
        let actual = actual_device_target_system_names(graph, source);
        if actual.is_empty() {
            continue;
        }
        if !device_matches_rule(graph, source, rule) {
            overrides.insert(source.id.clone());
        }
    }
}

pub fn reconcile_device_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<String>,
    device_rules: &[DeviceRouteRule],
) {
    let stale: Vec<String> = overrides
        .iter()
        .filter(|source_id| {
            let Some(source) = graph.devices.iter().find(|device| device.id == **source_id) else {
                return true;
            };
            let Some(rule) = device_rules
                .iter()
                .find(|rule| rule.source_system_name == source.system_name)
            else {
                return true;
            };
            device_matches_rule(graph, source, rule)
        })
        .cloned()
        .collect();

    for source_id in stale {
        overrides.remove(&source_id);
    }
}

pub fn reconcile_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<crate::core::stream_identity::StreamIdentityKey>,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
) {
    let stale: Vec<crate::core::stream_identity::StreamIdentityKey> = overrides
        .iter()
        .filter(|override_key| {
            let Some(stream) = graph
                .streams
                .iter()
                .find(|stream| identity_matches(&stream_identity_key(stream), override_key))
            else {
                return true;
            };
            let Some(current_target_id) = &stream.current_target else {
                return false;
            };
            let Some(device) = graph
                .devices
                .iter()
                .find(|device| device.id == *current_target_id)
            else {
                return false;
            };
            !should_track_manual_override(
                stream,
                &device.system_name,
                authored_rules,
                persisted_rules,
            )
        })
        .cloned()
        .collect();

    for key in stale {
        overrides.remove(&key);
    }
}

pub fn evaluate_stream_route(
    stream: &Stream,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
    manual_overrides: &HashSet<crate::core::stream_identity::StreamIdentityKey>,
) -> RouteExplanation {
    let stream_key = stream_identity_key(stream);
    let overridden = manual_overrides
        .iter()
        .any(|override_key| identity_matches(&stream_key, override_key));

    let candidates = collect_stream_candidates(stream, authored_rules, persisted_rules);

    if overridden {
        return RouteExplanation {
            source: RouteSource::ManualOverride,
            matched_rule_id: None,
            matched_rule_key: None,
            match_reasons: vec!["Live routing differs from saved rules (respected)".into()],
            skipped_candidates: candidates
                .into_iter()
                .map(|candidate| SkippedCandidate {
                    rule_key: candidate.key,
                    reason: "Skipped because of manual override".into(),
                })
                .collect(),
            action_status: ActionStatus::SkippedManualOverride,
            target_system_name: None,
            target_system_names: Vec::new(),
        };
    }

    let Some(winner) = candidates.first().cloned() else {
        return RouteExplanation {
            source: RouteSource::NoRule,
            matched_rule_id: None,
            matched_rule_key: None,
            match_reasons: vec!["No matching routing rule".into()],
            skipped_candidates: Vec::new(),
            action_status: ActionStatus::NoAction,
            target_system_name: None,
            target_system_names: Vec::new(),
        };
    };

    let skipped_candidates = candidates
        .iter()
        .skip(1)
        .map(|candidate| SkippedCandidate {
            rule_key: candidate.key.clone(),
            reason: format!(
                "Lower priority than {} (priority {})",
                winner.key, winner.priority
            ),
        })
        .collect();

    RouteExplanation {
        source: winner.source.clone(),
        matched_rule_id: winner.rule_id.clone(),
        matched_rule_key: Some(winner.key.clone()),
        match_reasons: winner.match_reasons.clone(),
        skipped_candidates,
        action_status: ActionStatus::NoAction,
        target_system_name: winner.target_system_names.first().cloned(),
        target_system_names: winner.target_system_names.clone(),
    }
}

pub fn apply_routing_rules_with_explanations(
    graph: &mut RuntimeGraph,
    ctx: &ApplyRulesContext<'_>,
) -> Result<(), AdapterError> {
    let config = ConfigStore::new()
        .load_config()
        .unwrap_or_else(|_| ConfigStore::default_config());
    let authored_rules = config.rules;
    let persisted_rules = config.routing_rules.stream_rules;

    for stream_id in graph.streams.iter().map(|stream| stream.id.clone()).collect::<Vec<_>>() {
        let Some(stream) = graph.streams.iter().find(|stream| stream.id == stream_id).cloned() else {
            continue;
        };

        let mut explanation =
            evaluate_stream_route(&stream, &authored_rules, &persisted_rules, ctx.manual_overrides);

        if explanation.source == RouteSource::ManualOverride {
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        }

        let target_system_name = explanation
            .target_system_name
            .clone()
            .or_else(|| explanation.target_system_names.first().cloned());
        let Some(target_system_name) = target_system_name else {
            explanation.action_status = ActionStatus::NoAction;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        };

        let Some(target_device) =
            find_device_by_system_name(graph, &target_system_name).cloned()
        else {
            explanation.action_status = ActionStatus::TargetUnavailable;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        };

        if stream.current_target.as_deref() == Some(target_device.id.as_str()) {
            explanation.action_status = ActionStatus::Applied;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        }

        if ctx.dry_run {
            explanation.action_status = ActionStatus::Simulated;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        }

        let apply_result = if ctx.mock_graph_only {
            Ok(())
        } else {
            crate::core::routing::apply_stream_to_sink(graph, &stream, &target_device.id)
                .map_err(|error| AdapterError::Message(error.to_string()))
        };

        match apply_result {
            Ok(()) => {
                explanation.action_status = ActionStatus::Applied;
                if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                    stream_mut.current_target = Some(target_device.id.clone());
                    stream_mut.current_targets.clear();
                    stream_mut.route_explanation = Some(explanation);
                }
            }
            Err(error) => {
                explanation.action_status = ActionStatus::Blocked;
                explanation
                    .match_reasons
                    .push(format!("Apply failed: {error}"));
                if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                    stream_mut.route_explanation = Some(explanation);
                }
            }
        }
    }

    apply_device_rules(graph, &config.routing_rules.device_rules, ctx)?;
    Ok(())
}

fn apply_device_rules(
    graph: &mut RuntimeGraph,
    device_rules: &[DeviceRouteRule],
    ctx: &ApplyRulesContext<'_>,
) -> Result<(), AdapterError> {
    if ctx.dry_run {
        return Ok(());
    }

    for rule in device_rules {
        if let Some(source) = find_device_by_system_name(graph, &rule.source_system_name) {
            if source.kind != DeviceKind::Virtual || source.direction != DeviceDirection::Output {
                continue;
            }
            let source_id = source.id.clone();
            if ctx.device_manual_overrides.contains(&source_id) {
                continue;
            }
            let target_system_names = rule.target_system_names_resolved();
            let target_devices: Vec<_> = target_system_names
                .iter()
                .filter_map(|system_name| find_device_by_system_name(graph, system_name).cloned())
                .collect();
            if target_devices.is_empty() {
                continue;
            }
            let target_ids: Vec<String> = target_devices.iter().map(|device| device.id.clone()).collect();
            let already = if source.is_multi_sink() {
                device_matches_rule(graph, source, rule)
            } else if let Some(target) = target_devices.first() {
                source
                    .current_target
                    .as_ref()
                    .is_some_and(|id| id == &target.id)
                    || pw_link::is_sink_monitor_routed_to(
                        &source.system_name,
                        &target.system_name,
                        target.direction == DeviceDirection::Input,
                    )
            } else {
                false
            };
            let routed = if already {
                true
            } else if ctx.mock_graph_only {
                true
            } else {
                crate::core::routing::apply_sink_targets(graph, &source_id, &target_ids)
                    .is_ok()
            };
            if routed {
                if let Some(device) = graph
                    .devices
                    .iter_mut()
                    .find(|device| device.id == source_id)
                {
                    device.current_targets = target_ids.clone();
                    device.current_target = target_ids.first().cloned();
                }
            }
        }
    }

    Ok(())
}

pub fn simulate_rules(graph: &RuntimeGraph) -> Vec<SimulationResult> {
    let config = ConfigStore::new()
        .load_config()
        .unwrap_or_else(|_| ConfigStore::default_config());

    graph
        .streams
        .iter()
        .map(|stream| {
            let explanation = evaluate_stream_route(
                stream,
                &config.rules,
                &config.routing_rules.stream_rules,
                &HashSet::new(),
            );
            let would_target_device_id = explanation
                .target_system_name
                .as_ref()
                .and_then(|system_name| find_device_by_system_name(graph, system_name))
                .map(|device| device.id.clone());
            SimulationResult {
                stream_id: stream.id.clone(),
                would_target_device_id,
                explanation,
            }
        })
        .collect()
}

pub fn migrate_routing_rules_to_authored(rules: &RoutingRulesConfig) -> Vec<Rule> {
    rules
        .stream_rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let mut conditions = Vec::new();
            if let Some(app_name) = &rule.app_name {
                conditions.push(RuleCondition::AppName {
                    value: app_name.clone(),
                });
            }
            if let Some(executable) = &rule.executable {
                conditions.push(RuleCondition::Executable {
                    value: executable.clone(),
                });
            }
            if let Some(media_name) = &rule.media_name {
                conditions.push(RuleCondition::MediaName {
                    value: media_name.clone(),
                });
            }

            Rule {
                id: format!("migrated-stream-{index}"),
                name: format!(
                    "Migrated: {}",
                    rule.app_name.as_deref().unwrap_or_else(|| {
                        rule.executable.as_deref().unwrap_or("stream")
                    })
                ),
                enabled: true,
                priority: -1_000 - index as i32,
                conditions,
                action: crate::core::models::RuleAction {
                    target_system_name: rule.target_system_name.clone(),
                    target_system_names: rule.target_system_names_resolved(),
                },
                safeguards: Default::default(),
            }
        })
        .collect()
}

pub fn ensure_rules_migrated() -> Result<(), AdapterError> {
    let store = ConfigStore::new();
    let mut config = store
        .load_config()
        .map_err(|error| AdapterError::Message(error.to_string()))?;

    if !config.rules.is_empty() || config.routing_rules.stream_rules.is_empty() {
        return Ok(());
    }

    config.rules = migrate_routing_rules_to_authored(&config.routing_rules);
    config.routing_rules.stream_rules.clear();
    store
        .save_config(&config)
        .map_err(|error| AdapterError::Message(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        Device, DeviceDirection, DeviceKind, RuntimeGraph, Stream, StreamDirection,
    };

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
    fn matching_rule_target_is_not_manual_override() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let rules = vec![Rule {
            id: "firefox".into(),
            name: "Firefox".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::AppName {
                value: "Firefox".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("hdmi".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];

        assert!(!should_track_manual_override(&stream, "hdmi", &rules, &[]));
        assert!(should_track_manual_override(&stream, "headphones", &rules, &[]));
    }

    #[test]
    fn manual_override_blocks_auto_apply_explanation() {
        let stream = sample_stream("Discord", Some("discord"), None);
        let mut overrides = HashSet::new();
        overrides.insert(stream_identity_key(&stream));

        let explanation = evaluate_stream_route(
            &stream,
            &[],
            &[StreamRouteRule {
                app_name: Some("Discord".into()),
                executable: Some("discord".into()),
                media_name: None,
                target_system_name: Some("chat".into()),
                target_system_names: Vec::new(),
            }],
            &overrides,
        );

        assert_eq!(explanation.source, RouteSource::ManualOverride);
        assert_eq!(explanation.action_status, ActionStatus::SkippedManualOverride);
    }

    #[test]
    fn detect_external_manual_override_when_system_differs_from_rule() {
        let stream = Stream {
            id: "slack-playback".into(),
            app_name: "Slack".into(),
            executable: Some("slack".into()),
            window_class: None,
            system_name: Some("Slack".into()),
            direction: crate::core::models::StreamDirection::Playback,
            current_target: Some("headphones".into()),
            current_targets: Vec::new(),
            media_name: None,
            is_system: false,
            route_explanation: None,
        };
        let graph = RuntimeGraph {
            devices: vec![
                Device {
                    id: "headphones".into(),
                    system_name: "alsa-headphones".into(),
                    label: "Headphones".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                },
                Device {
                    id: "speakers".into(),
                    system_name: "alsa-speakers".into(),
                    label: "Speakers".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                },
            ],
            streams: vec![stream],
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
        };
        let persisted = vec![StreamRouteRule {
            app_name: Some("Slack".into()),
            executable: Some("slack".into()),
            media_name: None,
            target_system_name: Some("alsa-speakers".into()),
            target_system_names: Vec::new(),
        }];

        let mut overrides = HashSet::new();
        detect_external_manual_overrides(&graph, &mut overrides, &[], &persisted);
        assert!(overrides.contains(&stream_identity_key(&graph.streams[0])));
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
}
