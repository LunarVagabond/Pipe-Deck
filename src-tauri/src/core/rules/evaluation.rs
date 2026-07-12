use crate::config::store::ConfigStore;
use crate::core::models::{
    ActionStatus, DeviceDirection, DeviceKind, DeviceRouteRule, RouteExplanation, RouteSource,
    Rule, RuntimeGraph, SimulationResult, Stream, StreamRouteRule,
};
use crate::core::routing_rules::find_device_by_system_name;
use crate::core::rules::matching::{
    collect_missing_metadata_skips, collect_stream_candidates, device_matches_rule,
    resolve_target_device,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::{stream_display_label, stream_identity_key};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pw_link;
use std::collections::HashSet;

pub fn evaluate_stream_route(
    stream: &Stream,
    authored_rules: &[Rule],
    persisted_rules: &[StreamRouteRule],
    manual_overrides: &HashSet<crate::core::stream_identity::StreamIdentityKey>,
) -> RouteExplanation {
    let stream_key = stream_identity_key(stream);
    let overridden = manual_overrides
        .iter()
        .any(|override_key| crate::core::stream_identity::identity_matches(&stream_key, override_key));

    let candidates = collect_stream_candidates(stream, authored_rules, persisted_rules);

    if overridden {
        return RouteExplanation {
            source: RouteSource::ManualOverride,
            matched_rule_id: None,
            matched_rule_key: None,
            match_reasons: vec!["Live routing differs from saved rules (respected)".into()],
            skipped_candidates: candidates
                .into_iter()
                .map(|candidate| crate::core::models::SkippedCandidate {
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
            skipped_candidates: collect_missing_metadata_skips(stream, authored_rules),
            action_status: ActionStatus::NoAction,
            target_system_name: None,
            target_system_names: Vec::new(),
        };
    };

    let skipped_candidates = candidates
        .iter()
        .skip(1)
        .map(|candidate| crate::core::models::SkippedCandidate {
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

        if let Some(limit) = ctx.limit_to_identities {
            let key = stream_identity_key(&stream);
            if !limit.contains(&key) {
                continue;
            }
        }

        let mut explanation =
            evaluate_stream_route(&stream, &authored_rules, &persisted_rules, ctx.manual_overrides);

        if explanation.source == RouteSource::ManualOverride {
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        }

        let candidates = collect_stream_candidates(&stream, &authored_rules, &persisted_rules);
        let Some(winner) = candidates.first() else {
            explanation.action_status = ActionStatus::NoAction;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        };

        let Some((target_device, fallback_note)) = resolve_target_device(graph, &stream, winner) else {
            explanation.action_status = ActionStatus::TargetUnavailable;
            if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                stream_mut.route_explanation = Some(explanation);
            }
            continue;
        };

        if let Some(note) = fallback_note {
            explanation.match_reasons.push(note);
            explanation.target_system_name = Some(target_device.system_name.clone());
            explanation.target_system_names = vec![target_device.system_name.clone()];
        }

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

pub fn simulate_rules(
    graph: &RuntimeGraph,
    recent_cache: &crate::core::recent_streams::RecentStreamCache,
) -> Vec<SimulationResult> {
    let config = ConfigStore::new()
        .load_config()
        .unwrap_or_else(|_| ConfigStore::default_config());

    let mut streams: Vec<Stream> = graph.streams.clone();
    streams.extend(recent_cache.synthetic_streams(&graph.streams));

    streams
        .into_iter()
        .filter(|stream| !stream.is_system)
        .map(|stream| {
            let is_recent = stream.id.starts_with("recent-");
            let mut explanation = evaluate_stream_route(
                &stream,
                &config.rules,
                &config.routing_rules.stream_rules,
                &HashSet::new(),
            );
            let candidates =
                collect_stream_candidates(&stream, &config.rules, &config.routing_rules.stream_rules);
            let resolved = candidates
                .first()
                .and_then(|winner| resolve_target_device(graph, &stream, winner));
            let would_target_device_id = resolved.as_ref().map(|(device, _)| device.id.clone());
            if let Some((device, fallback_note)) = resolved {
                if let Some(note) = fallback_note {
                    explanation.match_reasons.push(note);
                    explanation.target_system_name = Some(device.system_name.clone());
                    explanation.target_system_names = vec![device.system_name.clone()];
                    explanation.action_status = ActionStatus::Simulated;
                }
            }
            SimulationResult {
                stream_id: stream.id.clone(),
                stream_label: stream_display_label(&stream),
                is_recent,
                would_target_device_id,
                explanation,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        ActionStatus, Device, DeviceDirection, DeviceKind, FallbackPolicy, RuleCondition,
        StreamDirection,
    };
    use crate::core::rules::CandidateRule;
    use crate::core::stream_identity::stream_identity_key;

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
    fn no_match_surfaces_missing_window_class_metadata() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let rule = Rule {
            id: "window-class-rule".into(),
            name: "Window class rule".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::WindowClass {
                value: "firefox".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        };

        let explanation = evaluate_stream_route(&stream, &[rule], &[], &HashSet::new());

        assert_eq!(explanation.source, RouteSource::NoRule);
        assert_eq!(explanation.skipped_candidates.len(), 1);
        assert_eq!(explanation.skipped_candidates[0].rule_key, "Window class rule");
        assert!(explanation.skipped_candidates[0]
            .reason
            .contains("window_class"));
    }

    #[test]
    fn no_matching_rules_leaves_skipped_candidates_empty() {
        let stream = sample_stream("Firefox", Some("firefox"), None);

        let explanation = evaluate_stream_route(&stream, &[], &[], &HashSet::new());

        assert_eq!(explanation.source, RouteSource::NoRule);
        assert!(explanation.skipped_candidates.is_empty());
    }

    fn graph_with_outputs() -> RuntimeGraph {
        RuntimeGraph {
            devices: vec![
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
                    mix_source_ids: Vec::new(),
                },
                Device {
                    id: "hdmi".into(),
                    system_name: "hdmi-out".into(),
                    label: "HDMI".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_source_ids: Vec::new(),
                },
            ],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        }
    }

    #[test]
    fn keep_current_skips_when_rule_target_missing() {
        let graph = graph_with_outputs();
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let winner = CandidateRule {
            key: "Firefox".into(),
            rule_id: Some("rule-1".into()),
            target_system_names: vec!["missing-sink".into()],
            match_reasons: vec!["app_name == Firefox".into()],
            priority: 10,
            source: RouteSource::AuthoredRule,
            fallback_policy: FallbackPolicy::KeepCurrent,
        };

        assert!(resolve_target_device(&graph, &stream, &winner).is_none());
    }

    #[test]
    fn safe_default_falls_back_to_physical_output() {
        let graph = graph_with_outputs();
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let winner = CandidateRule {
            key: "Firefox".into(),
            rule_id: Some("rule-1".into()),
            target_system_names: vec!["missing-sink".into()],
            match_reasons: vec!["app_name == Firefox".into()],
            priority: 10,
            source: RouteSource::AuthoredRule,
            fallback_policy: FallbackPolicy::SafeDefault,
        };

        let (device, note) = resolve_target_device(&graph, &stream, &winner).expect("fallback");
        assert_eq!(device.id, "hdmi");
        assert!(note.is_some());
    }

    #[test]
    fn authored_rule_beats_persisted_rule_on_priority() {
        let stream = sample_stream("Discord", Some("discord"), None);
        let authored = vec![crate::core::models::Rule {
            id: "authored".into(),
            name: "Chat rule".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::AppName {
                value: "Discord".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("chat-sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];
        let persisted = vec![StreamRouteRule {
            app_name: Some("Discord".into()),
            executable: None,
            media_name: None,
            target_system_name: Some("headphones".into()),
            target_system_names: Vec::new(),
        }];

        let explanation = evaluate_stream_route(&stream, &authored, &persisted, &HashSet::new());
        assert_eq!(explanation.source, RouteSource::AuthoredRule);
        assert_eq!(
            explanation.target_system_name.as_deref(),
            Some("chat-sink")
        );
    }

    #[test]
    fn disabled_authored_rule_is_skipped() {
        let stream = sample_stream("Discord", Some("discord"), None);
        let authored = vec![crate::core::models::Rule {
            id: "disabled".into(),
            name: "Disabled".into(),
            enabled: false,
            priority: 100,
            conditions: vec![RuleCondition::AppName {
                value: "Discord".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("chat-sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];

        let explanation = evaluate_stream_route(&stream, &authored, &[], &HashSet::new());
        assert_eq!(explanation.source, RouteSource::NoRule);
    }

    #[test]
    fn multiple_authored_rules_highest_priority_wins() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let authored = vec![
            crate::core::models::Rule {
                id: "low".into(),
                name: "Low".into(),
                enabled: true,
                priority: 5,
                conditions: vec![RuleCondition::AppName {
                    value: "Firefox".into(),
                }],
                action: crate::core::models::RuleAction {
                    target_system_name: Some("speakers".into()),
                    target_system_names: Vec::new(),
                },
                safeguards: Default::default(),
            },
            crate::core::models::Rule {
                id: "high".into(),
                name: "High".into(),
                enabled: true,
                priority: 50,
                conditions: vec![RuleCondition::AppName {
                    value: "Firefox".into(),
                }],
                action: crate::core::models::RuleAction {
                    target_system_name: Some("hdmi".into()),
                    target_system_names: Vec::new(),
                },
                safeguards: Default::default(),
            },
        ];

        let explanation = evaluate_stream_route(&stream, &authored, &[], &HashSet::new());
        assert_eq!(explanation.matched_rule_key.as_deref(), Some("High"));
        assert_eq!(explanation.target_system_name.as_deref(), Some("hdmi"));
        assert_eq!(explanation.skipped_candidates.len(), 1);
    }

    #[test]
    fn capture_stream_matches_direction_rule() {
        let stream = Stream {
            id: "capture-1".into(),
            app_name: "OBS".into(),
            executable: Some("obs".into()),
            window_class: None,
            system_name: Some("obs-capture".into()),
            direction: StreamDirection::Capture,
            current_target: None,
            current_targets: Vec::new(),
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
        };
        let authored = vec![crate::core::models::Rule {
            id: "capture".into(),
            name: "Capture mic".into(),
            enabled: true,
            priority: 10,
            conditions: vec![
                RuleCondition::AppName {
                    value: "OBS".into(),
                },
                RuleCondition::Direction {
                    value: StreamDirection::Capture,
                },
            ],
            action: crate::core::models::RuleAction {
                target_system_name: Some("virtual-mic".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];

        let explanation = evaluate_stream_route(&stream, &authored, &[], &HashSet::new());
        assert_eq!(explanation.source, RouteSource::AuthoredRule);
        assert_eq!(
            explanation.target_system_name.as_deref(),
            Some("virtual-mic")
        );
    }

    #[test]
    fn limit_to_identities_skips_other_streams() {
        use crate::config::store::ConfigStore;
        use std::fs;
        use std::sync::{Mutex, OnceLock};

        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-rules-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);

        let store = ConfigStore::new();
        store.ensure_layout().expect("config layout");
        let mut config = ConfigStore::default_config();
        config.rules = vec![crate::core::models::Rule {
            id: "discord".into(),
            name: "Discord".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::AppName {
                value: "Discord".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("hdmi-out".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];
        store.save_config(&config).expect("save config");

        let mut graph = graph_with_outputs();
        graph.streams.push(sample_stream("Discord", Some("discord"), None));
        graph.streams.push(sample_stream("Firefox", Some("firefox"), None));
        let discord_key = stream_identity_key(
            graph.streams.iter().find(|s| s.app_name == "Discord").unwrap(),
        );
        let mut limit = HashSet::new();
        limit.insert(discord_key);
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: true,
            mock_graph_only: true,
            limit_to_identities: Some(&limit),
        };
        apply_routing_rules_with_explanations(&mut graph, &ctx).expect("simulate");
        let discord = graph.streams.iter().find(|s| s.app_name == "Discord").unwrap();
        assert_eq!(
            discord
                .route_explanation
                .as_ref()
                .and_then(|e| e.matched_rule_key.as_deref()),
            Some("Discord"),
        );
        let firefox = graph.streams.iter().find(|s| s.app_name == "Firefox").unwrap();
        assert!(firefox.route_explanation.is_none());

        let _ = fs::remove_dir_all(&temp_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }
}
