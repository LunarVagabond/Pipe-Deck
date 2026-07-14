use crate::core::models::{DeviceDirection, DeviceKind, DeviceRouteRule, Rule, RuntimeGraph, Stream, StreamRouteRule};
use crate::core::routing_rules::find_device_by_system_name;
use crate::core::rules::evaluation::evaluate_stream_route;
use crate::core::rules::matching::device_matches_rule;
use crate::core::stream_identity::{identity_matches, stream_identity_key};
use crate::backend::AudioBackend;
use std::collections::HashSet;

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

pub fn detect_external_device_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<String>,
    device_rules: &[DeviceRouteRule],
    backend: &dyn AudioBackend,
) {
    for rule in device_rules {
        let Some(source) = find_device_by_system_name(graph, &rule.source_system_name) else {
            continue;
        };
        if source.kind != DeviceKind::Virtual || source.direction != DeviceDirection::Output {
            continue;
        }
        let actual = crate::core::rules::matching::actual_device_target_system_names(graph, source, backend);
        if actual.is_empty() {
            continue;
        }
        if !device_matches_rule(graph, source, rule, backend) {
            overrides.insert(source.id.clone());
        }
    }
}

pub fn reconcile_device_manual_overrides(
    graph: &RuntimeGraph,
    overrides: &mut HashSet<String>,
    device_rules: &[DeviceRouteRule],
    backend: &dyn AudioBackend,
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
            device_matches_rule(graph, source, rule, backend)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        Device, DeviceDirection, DeviceKind, RuntimeGraph, Stream, StreamDirection,
    };
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
    fn matching_rule_target_is_not_manual_override() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let rules = vec![Rule {
            id: "firefox".into(),
            name: "Firefox".into(),
            enabled: true,
            priority: 10,
            conditions: vec![crate::core::models::RuleCondition::AppName {
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
    fn detect_external_manual_override_when_system_differs_from_rule() {
        let stream = Stream {
            id: "slack-playback".into(),
            app_name: "Slack".into(),
            executable: Some("slack".into()),
            window_class: None,
            system_name: Some("Slack".into()),
            direction: StreamDirection::Playback,
            current_target: Some("headphones".into()),
            current_targets: Vec::new(),
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
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
                    mix_sources: Vec::new(),
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
                    mix_sources: Vec::new(),
                },
            ],
            streams: vec![stream],
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
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
    fn device_rule_mismatch_tracks_manual_override() {
        let graph = RuntimeGraph {
            devices: vec![
                Device {
                    id: "virtual-chat".into(),
                    system_name: "pipe-deck-chat".into(),
                    label: "Chat".into(),
                    kind: DeviceKind::Virtual,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: Some("headphones".into()),
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                },
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
                    mix_sources: Vec::new(),
                },
            ],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "test".into(),
            notice: None,
            ..Default::default()
        };
        let device_rules = vec![DeviceRouteRule {
            source_system_name: "pipe-deck-chat".into(),
            target_system_name: Some("alsa-speakers".into()),
            target_system_names: Vec::new(),
        }];
        let mut overrides = HashSet::new();
        let backend = crate::backend::mock::MockAudioBackend::new();
        detect_external_device_manual_overrides(&graph, &mut overrides, &device_rules, &backend);
        assert!(overrides.contains("virtual-chat"));
    }
}
