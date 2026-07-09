use crate::core::models::{DeviceKind, Profile, RoutingIntent, RuntimeGraph, VolumeStateEntry};
use chrono::Utc;
use std::collections::HashMap;

pub fn capture_profile_from_graph(graph: &RuntimeGraph, id: &str, name: &str) -> Profile {
    let now = Utc::now().to_rfc3339();
    let routing_intents = graph
        .streams
        .iter()
        .filter_map(|stream| {
            stream.current_target.as_ref().map(|target| RoutingIntent {
                stream_id: stream.id.clone(),
                target_device_id: Some(target.clone()),
                target_device_ids: Vec::new(),
            })
        })
        .collect();

    let mut volume_state = HashMap::new();
    for device in &graph.devices {
        if let (Some(volume_percent), Some(muted)) = (device.volume_percent, device.muted) {
            volume_state.insert(
                device.id.clone(),
                VolumeStateEntry {
                    volume_percent,
                    muted,
                },
            );
        }
    }

    let mut device_assumptions = HashMap::new();
    for device in &graph.devices {
        if device.kind == DeviceKind::Virtual {
            device_assumptions.insert(device.id.clone(), device.system_name.clone());
        }
    }

    Profile {
        version: 1,
        id: id.to_string(),
        name: name.to_string(),
        created: now.clone(),
        updated: now,
        routing_intents,
        volume_state,
        device_assumptions,
    }
}

pub fn update_profile_timestamp(profile: &mut Profile) {
    profile.updated = Utc::now().to_rfc3339();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Device, DeviceDirection, DeviceKind, RuntimeGraph};

    #[test]
    fn capture_profile_records_virtual_device_assumptions() {
        let graph = RuntimeGraph {
            devices: vec![Device {
                id: "virtual-game".into(),
                system_name: "pipe-deck-game".into(),
                label: "Game".into(),
                kind: DeviceKind::Virtual,
                direction: DeviceDirection::Output,
                sink_mode: Some(crate::core::models::SinkMode::Single),
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
            }],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
            ..Default::default()
        };
        let profile = capture_profile_from_graph(&graph, "gaming", "Gaming");
        assert_eq!(
            profile.device_assumptions.get("virtual-game"),
            Some(&"pipe-deck-game".to_string())
        );
    }
}
