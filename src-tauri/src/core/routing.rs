use crate::core::models::{DeviceDirection, DeviceKind, DeviceRouteIntent, Profile, RoutingIntent, RuntimeGraph};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::{pactl, pw_link};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("adapter error: {0}")]
    Adapter(#[from] AdapterError),
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone)]
pub struct RoutingSnapshot {
    pub stream_intents: Vec<RoutingIntent>,
    pub device_intents: Vec<DeviceRouteIntent>,
}

pub fn capture_routing_snapshot(graph: &RuntimeGraph) -> RoutingSnapshot {
    RoutingSnapshot {
        stream_intents: graph
            .streams
            .iter()
            .filter_map(|stream| {
                stream.current_target.as_ref().map(|target| RoutingIntent {
                    stream_id: stream.id.clone(),
                    target_device_id: target.clone(),
                })
            })
            .collect(),
        device_intents: graph
            .devices
            .iter()
            .filter_map(|device| {
                device.current_target.as_ref().map(|target| DeviceRouteIntent {
                    source_device_id: device.id.clone(),
                    target_device_id: target.clone(),
                })
            })
            .collect(),
    }
}

pub fn apply_routing_intent(
    graph: &RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), RoutingError> {
    pactl::move_stream_to_target(graph, &intent.stream_id, &intent.target_device_id)?;
    Ok(())
}

pub fn apply_device_route_intent(
    graph: &RuntimeGraph,
    intent: &DeviceRouteIntent,
) -> Result<(), RoutingError> {
    let source = graph
        .devices
        .iter()
        .find(|device| device.id == intent.source_device_id)
        .ok_or_else(|| RoutingError::Message(format!("source device not found: {}", intent.source_device_id)))?;
    let target = graph
        .devices
        .iter()
        .find(|device| device.id == intent.target_device_id)
        .ok_or_else(|| RoutingError::Message(format!("target device not found: {}", intent.target_device_id)))?;

    validate_device_route(source, target)?;

    pw_link::link_sink_monitor_to_target(
        &source.system_name,
        &target.system_name,
        target.direction == DeviceDirection::Input,
    )?;
    Ok(())
}

pub fn apply_profile_routing(
    graph: &RuntimeGraph,
    profile: &Profile,
) -> Result<(), RoutingError> {
    for intent in &profile.routing_intents {
        apply_routing_intent(graph, intent)?;
    }
    Ok(())
}

pub fn restore_routing_snapshot(
    graph: &RuntimeGraph,
    snapshot: &RoutingSnapshot,
) -> Result<(), RoutingError> {
    for intent in &snapshot.stream_intents {
        apply_routing_intent(graph, intent)?;
    }
    for intent in &snapshot.device_intents {
        apply_device_route_intent(graph, intent)?;
    }
    Ok(())
}

pub fn apply_profile_volumes(
    graph: &RuntimeGraph,
    profile: &Profile,
) -> Result<(), RoutingError> {
    for (device_id, state) in &profile.volume_state {
        pactl::set_device_volume(device_id, graph, state.volume_percent)?;
        pactl::set_device_mute(device_id, graph, state.muted)?;
    }
    Ok(())
}

fn validate_device_route(
    source: &crate::core::models::Device,
    target: &crate::core::models::Device,
) -> Result<(), RoutingError> {
    if source.kind != DeviceKind::Virtual || source.direction != DeviceDirection::Output {
        return Err(RoutingError::Message(
            "only virtual sinks can be routed to another device".into(),
        ));
    }

    let valid_target = matches!(target.direction, DeviceDirection::Output | DeviceDirection::Input)
        && (target.kind == DeviceKind::Physical
            || (target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input));

    if !valid_target {
        return Err(RoutingError::Message(
            "virtual sinks can route to hardware outputs or virtual inputs".into(),
        ));
    }

    Ok(())
}

pub fn validate_device_route_ids(
    graph: &RuntimeGraph,
    source_device_id: &str,
    target_device_id: &str,
) -> Result<(), RoutingError> {
    let source = graph
        .devices
        .iter()
        .find(|device| device.id == source_device_id)
        .ok_or_else(|| RoutingError::Message(format!("source device not found: {source_device_id}")))?;
    let target = graph
        .devices
        .iter()
        .find(|device| device.id == target_device_id)
        .ok_or_else(|| RoutingError::Message(format!("target device not found: {target_device_id}")))?;
    validate_device_route(source, target)
}
