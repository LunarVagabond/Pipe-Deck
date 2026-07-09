use crate::core::models::{Profile, RoutingIntent, RuntimeGraph, VolumeStateEntry};
use chrono::Utc;
use std::collections::HashMap;

pub fn capture_profile_from_graph(graph: &RuntimeGraph, id: &str, name: &str) -> Profile {
    let now = Utc::now().to_rfc3339();
    let routing_intents = graph
        .streams
        .iter()
        .filter_map(|stream| {
            stream
                .current_target
                .as_ref()
                .map(|target| RoutingIntent {
                    stream_id: stream.id.clone(),
                    target_device_id: target.clone(),
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

    Profile {
        version: 1,
        id: id.to_string(),
        name: name.to_string(),
        created: now.clone(),
        updated: now,
        routing_intents,
        volume_state,
        device_assumptions: HashMap::new(),
    }
}

pub fn update_profile_timestamp(profile: &mut Profile) {
    profile.updated = Utc::now().to_rfc3339();
}
