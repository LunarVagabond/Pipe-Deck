use crate::core::models::{
    DeviceDirection, DeviceKind, DeviceRouteIntent, Profile, RoutingIntent, RuntimeGraph, Stream,
};
use crate::backend::{AudioBackend, BackendError};
use crate::backend::linux::split_sink;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("adapter error: {0}")]
    Adapter(#[from] BackendError),
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
                    target_device_id: Some(target.clone()),
                    target_device_ids: Vec::new(),
                })
            })
            .collect(),
        device_intents: graph
            .devices
            .iter()
            .filter_map(|device| {
                let targets = device.resolved_targets();
                if targets.is_empty() {
                    return None;
                }
                Some(DeviceRouteIntent {
                    source_device_id: device.id.clone(),
                    target_device_id: targets.first().cloned(),
                    target_device_ids: targets,
                })
            })
            .collect(),
    }
}

pub fn apply_routing_intent(
    graph: &RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), RoutingError> {
    let target = intent
        .target_device_id
        .as_ref()
        .or_else(|| intent.target_device_ids.first())
        .ok_or_else(|| RoutingError::Message("routing intent has no target".into()))?;
    split_sink::apply_stream_to_sink(graph, &intent.stream_id, target)?;
    Ok(())
}

pub fn apply_device_route_intent(
    graph: &RuntimeGraph,
    intent: &DeviceRouteIntent,
) -> Result<(), RoutingError> {
    let targets = intent.target_ids();
    split_sink::apply_sink_targets(graph, &intent.source_device_id, &targets)?;
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
    backend: &dyn AudioBackend,
    graph: &RuntimeGraph,
    profile: &Profile,
) -> Result<(), RoutingError> {
    for (device_id, state) in &profile.volume_state {
        backend.set_device_volume(graph, device_id, state.volume_percent)?;
        backend.set_device_mute(graph, device_id, state.muted)?;
    }
    Ok(())
}

pub fn apply_stream_to_sink(
    graph: &RuntimeGraph,
    stream: &Stream,
    target_device_id: &str,
) -> Result<(), RoutingError> {
    split_sink::apply_stream_to_sink(graph, &stream.id, target_device_id)?;
    Ok(())
}

pub fn apply_sink_targets(
    graph: &RuntimeGraph,
    sink_device_id: &str,
    target_device_ids: &[String],
) -> Result<(), RoutingError> {
    split_sink::apply_sink_targets(graph, sink_device_id, target_device_ids)?;
    Ok(())
}

/// Confirms a route command actually took effect, instead of trusting
/// whatever the next graph refresh happens to report. Reconciliation today
/// (`apply_device_rules_pass`) issues `apply_sink_targets` and, on `Ok`,
/// immediately writes the desired state into the graph model — a route
/// that silently didn't take (the shell-out equivalent of a fire-and-forget
/// write, exactly the failure mode behind issue #210) would otherwise go
/// unnoticed until something else happened to catch the mismatch. Polls
/// `backend.is_routed_to` (the same primitive `AudioBackend::is_routed_to`
/// already exposes for the "already correctly routed" check) rather than
/// re-deriving link state itself. Short timeout by design: this exists to
/// catch a route that silently didn't take, not to paper over a genuinely
/// slow/broken PipeWire session.
pub fn verify_route_applied(
    backend: &dyn AudioBackend,
    source_system_name: &str,
    target_system_name: &str,
    target_is_input: bool,
    timeout: std::time::Duration,
) -> Result<(), RoutingError> {
    let start = std::time::Instant::now();
    loop {
        if backend.is_routed_to(source_system_name, target_system_name, target_is_input) {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(RoutingError::Message(format!(
                "{source_system_name} does not appear routed to {target_system_name} after {timeout:?} — the route command may have silently failed"
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mock::MockAudioBackend;
    use std::time::Duration;

    #[test]
    fn verify_route_applied_succeeds_once_the_backend_reports_the_route_live() {
        let backend = MockAudioBackend::new();
        // The mock's sample graph seeds "sink-chat" already routed to
        // "sink-headphones" — no need to issue a route first.
        let result = verify_route_applied(&backend, "sink-chat", "sink-headphones", false, Duration::from_millis(500));
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn verify_route_applied_times_out_when_the_route_never_takes() {
        let backend = MockAudioBackend::new();
        let result = verify_route_applied(&backend, "sink-chat", "sink-speakers", false, Duration::from_millis(200));
        assert!(result.is_err());
    }
}
