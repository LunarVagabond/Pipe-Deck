use crate::config::store::ConfigStore;
use crate::core::models::{
    ActionStatus, DeviceDirection, DeviceKind, DeviceRouteRule, FallbackPolicy, RouteExplanation,
    RouteSource, Rule, RuntimeGraph, SimulationResult, Stream, StreamDirection, StreamRouteRule,
    VirtualRole,
};
use crate::core::routing_rules::find_device_by_system_name;
use crate::core::rules::matching::{
    collect_missing_metadata_skips, collect_stream_candidates, device_matches_rule,
    find_safe_default_device, resolve_target_device,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::{stream_display_label, stream_identity_key};
use crate::backend::BackendError;
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
            fallback_applied: false,
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
            fallback_applied: false,
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
        fallback_applied: false,
    }
}

pub fn apply_routing_rules_with_explanations(
    graph: &mut RuntimeGraph,
    ctx: &ApplyRulesContext<'_>,
) -> Result<(), BackendError> {
    let config = ConfigStore::new()
        .load_config()
        .unwrap_or_else(|_| ConfigStore::default_config());
    let authored_rules = config.rules;
    let persisted_rules = config.routing_rules.stream_rules;

    for stream_id in graph.streams.iter().map(|stream| stream.id.clone()).collect::<Vec<_>>() {
        let Some(stream) = graph.streams.iter().find(|stream| stream.id == stream_id).cloned() else {
            continue;
        };

        if let Some(limit) = ctx.limit_to_stream_ids {
            if !limit.contains(&stream.id) {
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
            explanation.fallback_applied = true;
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
                .map_err(|error| BackendError::Message(error.to_string()))
        };

        match apply_result {
            Ok(()) => {
                explanation.action_status = ActionStatus::Applied;
                if let Some(stream_mut) = graph.streams.iter_mut().find(|item| item.id == stream_id) {
                    stream_mut.current_target = Some(target_device.id.clone());
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
) -> Result<(), BackendError> {
    if ctx.dry_run {
        return Ok(());
    }

    // A single linear pass isn't enough once virtual outputs can chain into
    // other virtual outputs (PD-026): if rule A wants device X routed into Y
    // and rule B (evaluated later in the same pass) wants Y routed into Z,
    // X's pass sees Y's *stale, not-yet-updated* `current_target` (whatever
    // it was before this refresh) rather than Y's true end state. If that
    // stale value happens to point back toward X, `apply_sink_targets`'s
    // cycle guard (`split_sink::would_create_cycle`) reports a false-positive
    // cycle and silently drops X's route for this pass — the route was never
    // actually cyclic, only the mid-pass snapshot looked that way. Repeating
    // the pass until nothing changes (bounded by the rule count, since each
    // full pass can resolve at least one more link in any dependency chain)
    // lets a later rule's now-correct target be visible to an earlier one on
    // the next iteration, without changing behavior for the common
    // single-rule-per-refresh case (which converges in one pass either way).
    let max_passes = device_rules.len().max(1);
    for _ in 0..max_passes {
        let mut changed = false;
        apply_device_rules_pass(graph, device_rules, ctx, &mut changed);
        if !changed {
            break;
        }
    }

    Ok(())
}

fn apply_device_rules_pass(
    graph: &mut RuntimeGraph,
    device_rules: &[DeviceRouteRule],
    ctx: &ApplyRulesContext<'_>,
    changed: &mut bool,
) {
    for rule in device_rules {
        if let Some(source) = find_device_by_system_name(graph, &rule.source_system_name) {
            // A terminal Output (virtual) (#287) can never fan out — skip it
            // here the same way an ineligible device already was, rather
            // than attempting `apply_sink_targets` every refresh only to
            // have it rejected by the backend's own gate. Without this, a
            // rule whose source is a terminal Output retries and logs a
            // failure on every graph refresh forever, since nothing about
            // that failure is ever going to change.
            if source.kind != DeviceKind::Virtual
                || source.direction != DeviceDirection::Output
                || source.virtual_role != Some(VirtualRole::Bus)
            {
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
                if rule.safeguards.fallback_policy == FallbackPolicy::SafeDefault {
                    if let Some(fallback_device) = find_safe_default_device(graph, StreamDirection::Playback)
                    {
                        let fallback_id = fallback_device.id.clone();
                        let fallback_system_name = fallback_device.system_name.clone();
                        let fallback_is_input = fallback_device.direction == DeviceDirection::Input;
                        if fallback_id != source_id {
                            let routed = if ctx.mock_graph_only {
                                true
                            } else {
                                match crate::core::routing::apply_sink_targets(
                                    graph,
                                    &source_id,
                                    std::slice::from_ref(&fallback_id),
                                ) {
                                    Ok(()) => {
                                        if let Err(error) = crate::core::routing::verify_route_applied(
                                            ctx.backend,
                                            &source.system_name,
                                            &fallback_system_name,
                                            fallback_is_input,
                                            std::time::Duration::from_millis(750),
                                        ) {
                                            eprintln!("device rule fallback route verification failed: {error}");
                                        }
                                        true
                                    }
                                    Err(error) => {
                                        eprintln!(
                                            "device rule fallback route failed for {source_id} -> {fallback_id}: {error}"
                                        );
                                        false
                                    }
                                }
                            };
                            if routed {
                                if let Some(device) =
                                    graph.devices.iter_mut().find(|device| device.id == source_id)
                                {
                                    if device.current_targets != vec![fallback_id.clone()] {
                                        *changed = true;
                                    }
                                    device.current_targets = vec![fallback_id.clone()];
                                    device.current_target = Some(fallback_id);
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let target_ids: Vec<String> = target_devices.iter().map(|device| device.id.clone()).collect();
            let already = if source.is_multi_sink() {
                device_matches_rule(graph, source, rule, ctx.backend)
            } else if let Some(target) = target_devices.first() {
                source
                    .current_target
                    .as_ref()
                    .is_some_and(|id| id == &target.id)
                    || ctx.backend.is_routed_to(
                        &source.system_name,
                        &target.system_name,
                        target.direction == DeviceDirection::Input,
                    )
            } else {
                false
            };
            let routed = if already || ctx.mock_graph_only {
                true
            } else {
                match crate::core::routing::apply_sink_targets(graph, &source_id, &target_ids) {
                    Ok(()) => {
                        for target in &target_devices {
                            if let Err(error) = crate::core::routing::verify_route_applied(
                                ctx.backend,
                                &source.system_name,
                                &target.system_name,
                                target.direction == DeviceDirection::Input,
                                std::time::Duration::from_millis(750),
                            ) {
                                eprintln!("device rule route verification failed: {error}");
                            }
                        }
                        true
                    }
                    Err(error) => {
                        eprintln!("device rule route failed for {source_id} -> {target_ids:?}: {error}");
                        false
                    }
                }
            };
            if routed {
                if let Some(device) = graph
                    .devices
                    .iter_mut()
                    .find(|device| device.id == source_id)
                {
                    if device.current_targets != target_ids {
                        *changed = true;
                    }
                    device.current_targets = target_ids.clone();
                    device.current_target = target_ids.first().cloned();
                }
            }
        }
    }
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
            if let Some((device, Some(note))) = resolved {
                explanation.match_reasons.push(note);
                explanation.target_system_name = Some(device.system_name.clone());
                explanation.target_system_names = vec![device.system_name.clone()];
                explanation.action_status = ActionStatus::Simulated;
                explanation.fallback_applied = true;
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
        RuleSafeguards, StreamDirection,
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
                    virtual_role: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                },
                Device {
                    id: "hdmi".into(),
                    system_name: "hdmi-out".into(),
                    label: "HDMI".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    virtual_role: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
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
    fn limit_to_stream_ids_skips_other_streams() {
        use crate::config::store::ConfigStore;
        use std::fs;

        let _guard = crate::config::store::lock_config_dir_env();
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
        let mut discord = sample_stream("Discord", Some("discord"), None);
        discord.id = "discord-stream".into();
        let mut firefox = sample_stream("Firefox", Some("firefox"), None);
        firefox.id = "firefox-stream".into();
        graph.streams.push(discord);
        graph.streams.push(firefox);
        let mut limit = HashSet::new();
        limit.insert("discord-stream".to_string());
        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: true,
            mock_graph_only: true,
            limit_to_stream_ids: Some(&limit),
            backend: &backend,
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

    #[test]
    fn device_rule_with_terminal_output_source_is_skipped_not_retried() {
        // #287 follow-up: a persisted device rule whose source has since
        // become (or was created as) a terminal Output (virtual) must never
        // be attempted — it can structurally never fan out. Before this
        // fix, `apply_device_rules_pass` only checked kind/direction, so a
        // terminal-Output source fell through to `apply_sink_targets` on
        // every single graph refresh, which the backend's own Bus-only gate
        // rejects every time — spamming an identical failure log forever
        // instead of being skipped once, up front, like any other
        // ineligible device.
        use crate::config::store::ConfigStore;
        use crate::core::models::{DeviceRouteRule, VirtualRole};
        use std::fs;

        let _guard = crate::config::store::lock_config_dir_env();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-rules-terminal-output-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);

        let store = ConfigStore::new();
        store.ensure_layout().expect("config layout");
        let mut config = ConfigStore::default_config();
        config.routing_rules.device_rules = vec![DeviceRouteRule {
            source_system_name: "pipe-deck-terminal".into(),
            target_system_name: Some("hdmi-out".into()),
            target_system_names: Vec::new(),
            safeguards: Default::default(),
        }];
        store.save_config(&config).expect("save config");

        let mut graph = graph_with_outputs();
        graph.devices.push(Device {
            id: "terminal-output".into(),
            system_name: "pipe-deck-terminal".into(),
            label: "Terminal".into(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Output,
            sink_mode: None,
            virtual_role: Some(VirtualRole::Output),
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        });

        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: false,
            mock_graph_only: true,
            limit_to_stream_ids: None,
            backend: &backend,
        };
        apply_routing_rules_with_explanations(&mut graph, &ctx).expect("apply rules");

        let terminal = graph.devices.iter().find(|d| d.id == "terminal-output").unwrap();
        assert!(
            terminal.current_targets.is_empty(),
            "a terminal Output must never be marked routed by a device rule"
        );

        let _ = fs::remove_dir_all(&temp_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }

    #[test]
    fn apply_marks_fallback_applied_when_safe_default_used() {
        use crate::config::store::ConfigStore;
        use std::fs;

        let _guard = crate::config::store::lock_config_dir_env();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-rules-fallback-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);

        let store = ConfigStore::new();
        store.ensure_layout().expect("config layout");
        let mut config = ConfigStore::default_config();
        config.rules = vec![crate::core::models::Rule {
            id: "firefox".into(),
            name: "Firefox".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::AppName {
                value: "Firefox".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("missing-sink".into()),
                target_system_names: Vec::new(),
            },
            safeguards: RuleSafeguards {
                fallback_policy: FallbackPolicy::SafeDefault,
            },
        }];
        store.save_config(&config).expect("save config");

        let mut graph = graph_with_outputs();
        graph.streams.push(sample_stream("Firefox", Some("firefox"), None));
        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: false,
            mock_graph_only: true,
            limit_to_stream_ids: None,
            backend: &backend,
        };
        apply_routing_rules_with_explanations(&mut graph, &ctx).expect("apply");

        let firefox = graph.streams.iter().find(|s| s.app_name == "Firefox").unwrap();
        let explanation = firefox.route_explanation.as_ref().expect("explanation present");
        assert!(explanation.fallback_applied);

        let _ = fs::remove_dir_all(&temp_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }

    #[test]
    fn apply_does_not_mark_fallback_applied_when_target_resolves_directly() {
        use crate::config::store::ConfigStore;
        use std::fs;

        let _guard = crate::config::store::lock_config_dir_env();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-rules-no-fallback-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &temp_dir);

        let store = ConfigStore::new();
        store.ensure_layout().expect("config layout");
        let mut config = ConfigStore::default_config();
        config.rules = vec![crate::core::models::Rule {
            id: "firefox".into(),
            name: "Firefox".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::AppName {
                value: "Firefox".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("hdmi-out".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];
        store.save_config(&config).expect("save config");

        let mut graph = graph_with_outputs();
        graph.streams.push(sample_stream("Firefox", Some("firefox"), None));
        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: false,
            mock_graph_only: true,
            limit_to_stream_ids: None,
            backend: &backend,
        };
        apply_routing_rules_with_explanations(&mut graph, &ctx).expect("apply");

        let firefox = graph.streams.iter().find(|s| s.app_name == "Firefox").unwrap();
        let explanation = firefox.route_explanation.as_ref().expect("explanation present");
        assert!(!explanation.fallback_applied);

        let _ = fs::remove_dir_all(&temp_dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }

    #[test]
    fn persisted_rule_never_falls_back_even_with_missing_target() {
        let graph = graph_with_outputs();
        let stream = sample_stream("Soundux", None, Some("miniaudio"));
        let persisted = vec![StreamRouteRule {
            app_name: Some("Soundux".into()),
            executable: None,
            media_name: Some("miniaudio".into()),
            target_system_name: Some("missing-sink".into()),
            target_system_names: Vec::new(),
        }];

        let candidates = collect_stream_candidates(&stream, &[], &persisted);
        let winner = candidates.first().expect("persisted rule matches");
        assert_eq!(winner.fallback_policy, FallbackPolicy::KeepCurrent);
        assert!(resolve_target_device(&graph, &stream, winner).is_none());
    }

    #[test]
    fn tied_priority_candidates_resolve_to_first_declared_rule() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let authored = vec![
            crate::core::models::Rule {
                id: "first".into(),
                name: "First".into(),
                enabled: true,
                priority: 10,
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
                id: "second".into(),
                name: "Second".into(),
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
            },
        ];

        let explanation = evaluate_stream_route(&stream, &authored, &[], &HashSet::new());
        assert_eq!(explanation.matched_rule_key.as_deref(), Some("First"));
    }

    #[test]
    fn invalid_regex_condition_is_treated_as_non_match_without_panicking() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let authored = vec![crate::core::models::Rule {
            id: "bad-regex".into(),
            name: "Bad regex".into(),
            enabled: true,
            priority: 10,
            conditions: vec![RuleCondition::Regex {
                field: "app_name".into(),
                pattern: "(unterminated".into(),
            }],
            action: crate::core::models::RuleAction {
                target_system_name: Some("speakers".into()),
                target_system_names: Vec::new(),
            },
            safeguards: Default::default(),
        }];

        let explanation = evaluate_stream_route(&stream, &authored, &[], &HashSet::new());
        assert_eq!(explanation.source, RouteSource::NoRule);
    }

    fn graph_with_virtual_sink() -> RuntimeGraph {
        let mut graph = graph_with_outputs();
        graph.devices.push(Device {
            id: "chat-sink".into(),
            system_name: "pipe-deck-chat".into(),
            label: "Chat Mix".into(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Output,
            sink_mode: None,
            virtual_role: Some(VirtualRole::Bus),
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        });
        graph
    }

    #[test]
    fn device_rule_falls_back_to_safe_default_when_target_missing() {
        let mut graph = graph_with_virtual_sink();
        let device_rules = vec![DeviceRouteRule {
            source_system_name: "pipe-deck-chat".into(),
            target_system_name: Some("missing-target".into()),
            target_system_names: Vec::new(),
            safeguards: RuleSafeguards {
                fallback_policy: FallbackPolicy::SafeDefault,
            },
        }];
        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: false,
            mock_graph_only: true,
            limit_to_stream_ids: None,
            backend: &backend,
        };
        apply_device_rules(&mut graph, &device_rules, &ctx).expect("apply device rules");

        let source = graph.devices.iter().find(|device| device.id == "chat-sink").unwrap();
        // find_safe_default_device sorts physical outputs by label; "HDMI" < "Speakers".
        assert_eq!(source.current_target.as_deref(), Some("hdmi"));
    }

    #[test]
    fn device_rule_stays_unrouted_when_target_missing_and_keep_current() {
        let mut graph = graph_with_virtual_sink();
        let device_rules = vec![DeviceRouteRule {
            source_system_name: "pipe-deck-chat".into(),
            target_system_name: Some("missing-target".into()),
            target_system_names: Vec::new(),
            safeguards: RuleSafeguards::default(),
        }];
        let backend = crate::backend::mock::MockAudioBackend::new();
        let ctx = ApplyRulesContext {
            manual_overrides: &HashSet::new(),
            device_manual_overrides: &HashSet::new(),
            dry_run: false,
            mock_graph_only: true,
            limit_to_stream_ids: None,
            backend: &backend,
        };
        apply_device_rules(&mut graph, &device_rules, &ctx).expect("apply device rules");

        let source = graph.devices.iter().find(|device| device.id == "chat-sink").unwrap();
        assert_eq!(source.current_target, None);
    }
}
