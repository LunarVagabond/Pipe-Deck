use crate::core::models::{Profile, RoutingDrift, RoutingDriftItem, RuntimeGraph};

pub fn compare_profile_to_graph(profile: &Profile, graph: &RuntimeGraph) -> RoutingDrift {
    let mut items = Vec::new();

    for intent in &profile.routing_intents {
        let desired_target_id = intent
            .target_device_id
            .clone()
            .or_else(|| intent.target_device_ids.first().cloned());

        let stream = graph.streams.iter().find(|stream| stream.id == intent.stream_id);
        let live_target_id = stream.and_then(|stream| stream.current_target.clone());

        if live_target_id == desired_target_id {
            continue;
        }

        let stream_label = stream
            .map(|stream| stream.app_name.clone())
            .unwrap_or_else(|| intent.stream_id.clone());

        items.push(RoutingDriftItem {
            stream_id: intent.stream_id.clone(),
            stream_label,
            live_target_id: live_target_id.clone(),
            live_target_label: live_target_id
                .as_deref()
                .and_then(|id| device_label(graph, id)),
            desired_target_id: desired_target_id.clone(),
            desired_target_label: desired_target_id
                .as_deref()
                .and_then(|id| device_label(graph, id)),
        });
    }

    RoutingDrift {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        has_drift: !items.is_empty(),
        items,
    }
}

fn device_label(graph: &RuntimeGraph, device_id: &str) -> Option<String> {
    graph
        .devices
        .iter()
        .find(|device| device.id == device_id)
        .map(|device| device.label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{
        Device, DeviceDirection, DeviceKind, RoutingIntent, RuntimeGraph, Stream,
        StreamDirection,
    };

    #[test]
    fn detects_profile_drift_against_live_graph() {
        let profile = Profile {
            version: 1,
            id: "gaming".into(),
            name: "Gaming".into(),
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
            routing_intents: vec![RoutingIntent {
                stream_id: "slack".into(),
                target_device_id: Some("speakers".into()),
                target_device_ids: Vec::new(),
            }],
            volume_state: Default::default(),
            device_assumptions: Default::default(),
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
            streams: vec![Stream {
                id: "slack".into(),
                app_name: "Slack".into(),
                executable: Some("slack".into()),
                window_class: None,
                system_name: None,
                direction: StreamDirection::Playback,
                current_target: Some("headphones".into()),
                current_targets: Vec::new(),
                media_name: None,
                is_system: false,
                route_explanation: None,
            }],
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        };

        let drift = compare_profile_to_graph(&profile, &graph);
        assert!(drift.has_drift);
        assert_eq!(drift.items.len(), 1);
        assert_eq!(drift.items[0].live_target_label.as_deref(), Some("Headphones"));
        assert_eq!(drift.items[0].desired_target_label.as_deref(), Some("Speakers"));
    }
}
