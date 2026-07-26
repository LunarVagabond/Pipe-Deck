use crate::core::models::{Rule, RuntimeGraph, Stream, StreamRouteRule};
use crate::core::rules::evaluation::evaluate_stream_route;
use crate::core::stream_identity::{identity_matches, stream_identity_key};
use std::collections::HashSet;

/// Resolves a `current_target` id to the system_name of whatever it points
/// at — a plain `Device`, or (PD-032) a processing node such as a Mixer's
/// own feed sink. Both manual-override detection functions below need this,
/// not just a device lookup — without the processing-node fallback, a
/// stream currently routed into a Mixer's feed sink never registers as
/// overridden (its `current_target` resolves to the Mixer's id, which isn't
/// in `graph.devices`), so the routing-rules reconciler treats it as
/// unrouted and keeps reasserting whatever rule used to apply before it was
/// manually wired into the Mixer.
fn resolve_target_system_name<'a>(graph: &'a RuntimeGraph, target_id: &str) -> Option<&'a str> {
    if let Some(device) = graph.devices.iter().find(|device| device.id == target_id) {
        return Some(&device.system_name);
    }
    graph
        .processing_nodes
        .iter()
        .find(|node| node.id == target_id)
        .map(|node| node.system_name.as_str())
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
        let Some(target_system_name) = resolve_target_system_name(graph, current_target_id) else {
            continue;
        };

        if should_track_manual_override(
            stream,
            target_system_name,
            authored_rules,
            persisted_rules,
        ) {
            overrides.insert(stream_identity_key(stream));
        }
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
            let Some(target_system_name) = resolve_target_system_name(graph, current_target_id) else {
                return false;
            };
            !should_track_manual_override(
                stream,
                target_system_name,
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
    use crate::core::models::{Device, DeviceDirection, DeviceKind, RuntimeGraph, Stream, StreamDirection};
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
}
