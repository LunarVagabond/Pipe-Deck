use crate::backend::linux::split_sink;
use crate::backend::{AudioBackend, BackendError};
use crate::core::models::{Profile, RoutingIntent, RuntimeGraph, Stream};
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
                    target_system_name: graph
                        .devices
                        .iter()
                        .find(|device| &device.id == target)
                        .map(|device| device.system_name.clone()),
                })
            })
            .collect(),
    }
}

/// Resolves a routing intent's target to a device id that's actually live in
/// `graph` right now. The raw id wins when it still matches a live device;
/// otherwise falls back to `target_system_name` (stable across a BT/USB
/// reconnect that assigned the device a new PipeWire node id — #13, #14).
fn resolve_intent_target<'a>(
    graph: &'a RuntimeGraph,
    intent: &'a RoutingIntent,
) -> Option<&'a str> {
    let target = intent
        .target_device_id
        .as_deref()
        .or_else(|| intent.target_device_ids.first().map(String::as_str))?;
    if graph.devices.iter().any(|device| device.id == target) {
        return Some(target);
    }
    let system_name = intent.target_system_name.as_deref()?;
    graph
        .devices
        .iter()
        .find(|device| device.system_name == system_name)
        .map(|device| device.id.as_str())
}

pub fn apply_routing_intent(
    graph: &RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), RoutingError> {
    let target = resolve_intent_target(graph, intent)
        .ok_or_else(|| RoutingError::Message("routing intent has no target".into()))?;
    split_sink::apply_stream_to_sink(graph, &intent.stream_id, target)?;
    Ok(())
}

pub fn apply_profile_routing(graph: &RuntimeGraph, profile: &Profile) -> Result<(), RoutingError> {
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
    Ok(())
}

pub fn apply_profile_volumes(
    backend: &dyn AudioBackend,
    graph: &RuntimeGraph,
    profile: &Profile,
) -> Result<(), RoutingError> {
    for (device_id, state) in &profile.volume_state {
        // A hardware device's id isn't stable across a BT/USB reconnect (#13, #14) — if the
        // captured id no longer matches a live device, fall back to `system_name`, and skip
        // the entry entirely if even that doesn't resolve (device genuinely gone) rather than
        // failing the whole profile swap over one no-longer-present device.
        let resolved_id = if graph.devices.iter().any(|device| &device.id == device_id) {
            Some(device_id.as_str())
        } else {
            state.system_name.as_deref().and_then(|system_name| {
                graph
                    .devices
                    .iter()
                    .find(|device| device.system_name == system_name)
                    .map(|device| device.id.as_str())
            })
        };
        let Some(resolved_id) = resolved_id else {
            continue;
        };
        backend.set_device_volume(graph, resolved_id, state.volume_percent)?;
        backend.set_device_mute(graph, resolved_id, state.muted)?;
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

/// Confirms a route command actually took effect, instead of trusting
/// whatever the next graph refresh happens to report. A route that silently
/// didn't take (the shell-out equivalent of a fire-and-forget write, exactly
/// the failure mode behind issue #210) would otherwise go unnoticed until
/// something else happened to catch the mismatch. Polls
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
        let result = verify_route_applied(
            &backend,
            "sink-chat",
            "sink-headphones",
            false,
            Duration::from_millis(500),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn verify_route_applied_times_out_when_the_route_never_takes() {
        let backend = MockAudioBackend::new();
        let result = verify_route_applied(
            &backend,
            "sink-chat",
            "sink-speakers",
            false,
            Duration::from_millis(200),
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolve_intent_target_prefers_the_raw_id_when_it_is_still_live() {
        let backend = MockAudioBackend::new();
        let graph = backend.fetch_graph().unwrap();
        let intent = RoutingIntent {
            stream_id: "stream-chat".into(),
            target_device_id: Some("sink-headphones".into()),
            target_device_ids: Vec::new(),
            target_system_name: Some("sink-speakers".into()),
        };
        // The raw id is still live, so it should win even though a (deliberately
        // mismatched) system_name is also present.
        assert_eq!(
            resolve_intent_target(&graph, &intent),
            Some("sink-headphones")
        );
    }

    #[test]
    fn resolve_intent_target_falls_back_to_system_name_when_the_raw_id_is_stale() {
        let backend = MockAudioBackend::new();
        let graph = backend.fetch_graph().unwrap();
        // Simulates a BT/USB reconnect (#13, #14): the profile's saved id
        // ("node-75-pre-reconnect") no longer matches any live device, but the
        // device's system_name is stable across the reconnect.
        let intent = RoutingIntent {
            stream_id: "stream-chat".into(),
            target_device_id: Some("node-75-pre-reconnect".into()),
            target_device_ids: Vec::new(),
            target_system_name: Some("sink-headphones".into()),
        };
        assert_eq!(
            resolve_intent_target(&graph, &intent),
            Some("sink-headphones")
        );
    }

    #[test]
    fn resolve_intent_target_gives_up_when_neither_id_nor_system_name_match() {
        let backend = MockAudioBackend::new();
        let graph = backend.fetch_graph().unwrap();
        let intent = RoutingIntent {
            stream_id: "stream-chat".into(),
            target_device_id: Some("node-75-pre-reconnect".into()),
            target_device_ids: Vec::new(),
            target_system_name: Some("bluez_output.gone-for-good".into()),
        };
        assert_eq!(resolve_intent_target(&graph, &intent), None);
    }

    #[test]
    fn apply_profile_volumes_resolves_a_stale_device_id_via_system_name() {
        let backend = MockAudioBackend::new();
        let graph = backend.fetch_graph().unwrap();
        let profile = Profile {
            version: 1,
            id: "bt-test".into(),
            name: "bt-test".into(),
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
            routing_intents: Vec::new(),
            volume_state: [(
                "node-75-pre-reconnect".to_string(),
                crate::core::models::VolumeStateEntry {
                    volume_percent: 42,
                    muted: true,
                    system_name: Some("sink-headphones".into()),
                },
            )]
            .into_iter()
            .collect(),
            device_assumptions: Default::default(),
            effect_state: Default::default(),
        };

        apply_profile_volumes(&backend, &graph, &profile).expect("should resolve via system_name");

        let updated = backend.fetch_graph().unwrap();
        let device = updated
            .devices
            .iter()
            .find(|device| device.system_name == "sink-headphones")
            .unwrap();
        assert_eq!(device.volume_percent, Some(42));
        assert_eq!(device.muted, Some(true));
    }

    #[test]
    fn apply_profile_volumes_skips_an_entry_that_cannot_be_resolved_instead_of_failing_the_whole_profile(
    ) {
        let backend = MockAudioBackend::new();
        let graph = backend.fetch_graph().unwrap();
        let profile = Profile {
            version: 1,
            id: "bt-test".into(),
            name: "bt-test".into(),
            created: "2026-01-01T00:00:00Z".into(),
            updated: "2026-01-01T00:00:00Z".into(),
            routing_intents: Vec::new(),
            volume_state: [
                (
                    "ghost-device".to_string(),
                    crate::core::models::VolumeStateEntry {
                        volume_percent: 42,
                        muted: true,
                        system_name: Some("bluez_output.gone-for-good".into()),
                    },
                ),
                (
                    "sink-headphones".to_string(),
                    crate::core::models::VolumeStateEntry {
                        volume_percent: 33,
                        muted: false,
                        system_name: Some("sink-headphones".into()),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            device_assumptions: Default::default(),
            effect_state: Default::default(),
        };

        // A device that's genuinely gone shouldn't abort restoring everything else.
        apply_profile_volumes(&backend, &graph, &profile)
            .expect("unresolvable entries should be skipped, not error");

        let updated = backend.fetch_graph().unwrap();
        let device = updated
            .devices
            .iter()
            .find(|device| device.system_name == "sink-headphones")
            .unwrap();
        assert_eq!(device.volume_percent, Some(33));
    }
}
