use crate::core::models::{DeviceRouteIntent, Profile, RoutingIntent, RuntimeGraph};
use crate::core::routing::RoutingSnapshot;

use super::EngineError;

pub(super) fn apply_mock_routing(
    graph: &mut RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), EngineError> {
    let target_id = intent
        .target_device_id
        .as_ref()
        .or_else(|| intent.target_device_ids.first())
        .ok_or_else(|| EngineError::Routing("routing intent has no target".into()))?;
    let stream = graph
        .streams
        .iter_mut()
        .find(|stream| stream.id == intent.stream_id)
        .ok_or_else(|| EngineError::Routing(format!("stream not found: {}", intent.stream_id)))?;
    if !graph.devices.iter().any(|device| device.id == *target_id) {
        return Err(EngineError::Routing(format!(
            "target device not found: {target_id}"
        )));
    }
    stream.current_target = Some(target_id.clone());
    Ok(())
}

pub(super) fn apply_mock_snapshot(
    graph: &mut RuntimeGraph,
    snapshot: &RoutingSnapshot,
) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
    }
    for device in &mut graph.devices {
        device.current_target = None;
        device.current_targets.clear();
    }
    for intent in &snapshot.stream_intents {
        apply_mock_routing(graph, intent)?;
    }
    for intent in &snapshot.device_intents {
        apply_mock_device_route(graph, intent)?;
    }
    Ok(())
}

pub(super) fn apply_mock_device_route(
    graph: &mut RuntimeGraph,
    intent: &DeviceRouteIntent,
) -> Result<(), EngineError> {
    let targets = intent.target_ids();
    if !graph
        .devices
        .iter()
        .any(|device| device.id == intent.source_device_id)
    {
        return Err(EngineError::Routing(format!(
            "source device not found: {}",
            intent.source_device_id
        )));
    }
    for target_id in &targets {
        if !graph.devices.iter().any(|device| device.id == *target_id) {
            return Err(EngineError::Routing(format!(
                "target device not found: {target_id}"
            )));
        }
    }

    let device = graph
        .devices
        .iter_mut()
        .find(|device| device.id == intent.source_device_id)
        .expect("source device exists");
    device.current_targets = targets.clone();
    device.current_target = targets.first().cloned();
    Ok(())
}

pub(super) fn apply_mock_profile(graph: &mut RuntimeGraph, profile: &Profile) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
    }
    for intent in &profile.routing_intents {
        apply_mock_routing(graph, intent)?;
    }
    Ok(())
}

pub(super) fn apply_mock_profile_volumes(graph: &mut RuntimeGraph, profile: &Profile) {
    for (device_id, state) in &profile.volume_state {
        if let Some(device) = graph.devices.iter_mut().find(|device| device.id == *device_id) {
            device.volume_percent = Some(state.volume_percent);
            device.muted = Some(state.muted);
        }
    }
}
