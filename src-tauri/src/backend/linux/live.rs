use crate::core::models::{Device, DeviceDirection, MixSourceSpec, RuntimeGraph, VirtualDeviceInfo, VirtualDeviceResult};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use crate::backend::{BackendError, GraphListener, AudioBackend};
use crate::backend::linux::graph_enrich;
use crate::backend::linux::graph_routing;
use crate::backend::linux::pw_dump::{self, PwDumpObject};
use crate::backend::linux::virtual_devices::{VirtualDeviceEntry, VirtualDeviceRegistry};
use crate::backend::linux::virtual_mic_mix;
use crate::backend::slugify;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MONITOR_DEBOUNCE: Duration = Duration::from_millis(200);
// Under sustained high-churn (many streams appearing/disappearing rapidly),
// events never go quiet long enough for the debounce window alone to fire —
// this caps how long a burst can coalesce before we force a refresh anyway,
// so routing changes still surface promptly (see PipeWire_Design.md).
const MAX_COALESCE_WINDOW: Duration = Duration::from_millis(400);

pub struct LinuxPipeWireBackend {
    cached_graph: Arc<Mutex<RuntimeGraph>>,
    listener: Arc<Mutex<Option<GraphListener>>>,
    registry: Arc<VirtualDeviceRegistry>,
}

impl LinuxPipeWireBackend {
    pub fn new() -> Result<Self, BackendError> {
        let graph = enumerate_pipewire().unwrap_or_else(|error| RuntimeGraph {
            notice: Some(format!(
                "PipeWire snapshot unavailable ({error}). Dashboard will retry automatically."
            )),
            ..RuntimeGraph::default()
        });
        let cached_graph = Arc::new(Mutex::new(graph));
        let listener = Arc::new(Mutex::new(None));
        let registry = VirtualDeviceRegistry::new();

        Ok(Self {
            cached_graph,
            listener,
            registry,
        })
    }

    fn create_output_internal(&self, system_name: &str, label: &str, multi: bool) -> Result<VirtualDeviceEntry, BackendError> {
        self.registry.create_output_for(system_name, label, multi)
    }

    fn create_input_internal(&self, system_name: &str, label: &str) -> Result<VirtualDeviceEntry, BackendError> {
        self.registry.create_input_for(system_name, label)
    }
}

impl AudioBackend for LinuxPipeWireBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        match enumerate_pipewire() {
            Ok(graph) => {
                let mut cached = self
                    .cached_graph
                    .lock()
                    .map_err(|_| BackendError::Message("graph lock poisoned".into()))?;
                *cached = graph.clone();
                Ok(graph)
            }
            Err(error) => {
                let cached = self
                    .cached_graph
                    .lock()
                    .map_err(|_| BackendError::Message("graph lock poisoned".into()))?;
                if cached.devices.is_empty() && cached.streams.is_empty() {
                    return Err(error);
                }
                let mut graph = cached.clone();
                graph.notice = Some(format!(
                    "PipeWire snapshot unavailable ({error}). Showing last known graph."
                ));
                Ok(graph)
            }
        }
    }

    fn subscribe(&self, listener: GraphListener) -> Result<(), BackendError> {
        *self
            .listener
            .lock()
            .map_err(|_| BackendError::Message("listener lock poisoned".into()))? =
            Some(listener);

        let cached_graph = self.cached_graph.clone();
        let listener_slot = self.listener.clone();
        thread::spawn(move || {
            if !run_pw_dump_monitor(&cached_graph, &listener_slot) {
                run_poll_loop(&cached_graph, &listener_slot);
            }
        });

        Ok(())
    }

    fn set_device_volume(&self, graph: &RuntimeGraph, device_id: &str, percent: u8) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_device_volume(device_id, graph, percent)
    }

    fn set_device_mute(&self, graph: &RuntimeGraph, device_id: &str, muted: bool) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_device_mute(device_id, graph, muted)
    }

    fn set_stream_volume(&self, graph: &RuntimeGraph, stream_id: &str, percent: u8) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_stream_volume(graph, stream_id, percent)
    }

    fn set_stream_mute(&self, graph: &RuntimeGraph, stream_id: &str, muted: bool) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_stream_mute(graph, stream_id, muted)
    }

    fn clear_stream_target(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::clear_stream_target(graph, stream_id, previous_target_device_id)
    }

    fn route_stream(&self, graph: &RuntimeGraph, stream_id: &str, target_device_id: &str) -> Result<(), BackendError> {
        let intent = crate::core::models::RoutingIntent {
            stream_id: stream_id.to_string(),
            target_device_id: Some(target_device_id.to_string()),
            target_device_ids: Vec::new(),
        };
        crate::core::routing::apply_routing_intent(graph, &intent)
            .map_err(|error| BackendError::Message(error.to_string()))
    }

    fn route_device(&self, graph: &RuntimeGraph, source_device_id: &str, target_device_ids: &[String]) -> Result<(), BackendError> {
        let intent = crate::core::models::DeviceRouteIntent {
            source_device_id: source_device_id.to_string(),
            target_device_id: target_device_ids.first().cloned(),
            target_device_ids: target_device_ids.to_vec(),
        };
        crate::core::routing::apply_device_route_intent(graph, &intent)
            .map_err(|error| BackendError::Message(error.to_string()))
    }

    fn sync_live_routing_graph(&self, graph: &mut RuntimeGraph) {
        graph_routing::sync_live_routing_graph(graph);
    }

    fn apply_user_cleared_routes(
        &self,
        graph: &mut RuntimeGraph,
        cleared_streams: &HashSet<StreamIdentityKey>,
        cleared_devices: &HashSet<String>,
    ) {
        graph_routing::apply_user_cleared_routes(graph, cleared_streams, cleared_devices);
    }

    fn apply_graph_routing(&self, graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>) {
        graph_routing::apply_graph_routing(graph, ctx);
    }

    fn apply_virtual_mic_mix(&self, virtual_input: &Device, mix_sources: &[MixSourceSpec]) -> Result<(), BackendError> {
        virtual_mic_mix::apply_virtual_mic_mix(virtual_input, mix_sources)
    }

    fn set_mix_source_volume(&self, virtual_input_system_name: &str, source_system_name: &str, percent: u8) -> Result<(), BackendError> {
        virtual_mic_mix::set_mix_source_volume(virtual_input_system_name, source_system_name, percent)
    }

    fn set_mix_source_mute(&self, virtual_input_system_name: &str, source_system_name: &str, muted: bool) -> Result<(), BackendError> {
        virtual_mic_mix::set_mix_source_mute(virtual_input_system_name, source_system_name, muted)
    }

    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]) {
        graph_enrich::apply_device_aliases(devices);
        graph_enrich::apply_device_levels(devices);
    }

    fn add_connection_effect(&self, graph: &RuntimeGraph, source_id: &str, target_device_id: &str) -> Result<(String, String), BackendError> {
        crate::backend::linux::connection_effects::add_connection_effect(graph, source_id, target_device_id)
    }

    fn remove_connection_effect(&self, source_system_name: &str, target_system_name: &str) -> Result<(), BackendError> {
        crate::backend::linux::connection_effects::remove_connection_effect(source_system_name, target_system_name)
    }

    fn set_connection_volume(&self, source_system_name: &str, target_system_name: &str, percent: u8) -> Result<(), BackendError> {
        crate::backend::linux::connection_effects::set_connection_volume(source_system_name, target_system_name, percent)
    }

    fn set_connection_mute(&self, source_system_name: &str, target_system_name: &str, muted: bool) -> Result<(), BackendError> {
        crate::backend::linux::connection_effects::set_connection_mute(source_system_name, target_system_name, muted)
    }

    fn monitor_routes_for_source(&self, source_system_name: &str) -> Vec<String> {
        crate::backend::linux::pw_link::list_all_monitor_routes_for_source(source_system_name)
    }

    fn is_routed_to(&self, source_system_name: &str, target_system_name: &str, target_is_input: bool) -> bool {
        crate::backend::linux::pw_link::is_sink_monitor_routed_to(source_system_name, target_system_name, target_is_input)
    }

    fn create_virtual_output(&self, label: &str, multi: bool) -> Result<VirtualDeviceResult, BackendError> {
        let system_name = format!("pipe-deck-{}", slugify(label));
        Ok(self.create_output_internal(&system_name, label, multi)?.into_result())
    }

    fn create_virtual_input(&self, label: &str) -> Result<VirtualDeviceResult, BackendError> {
        let system_name = format!("pipe-deck-{}", slugify(label));
        Ok(self.create_input_internal(&system_name, label)?.into_result())
    }

    fn restore_virtual_device(
        &self,
        system_name: &str,
        label: &str,
        direction: DeviceDirection,
        multi: bool,
        mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        let entry = match direction {
            DeviceDirection::Input => self.create_input_internal(system_name, label)?,
            DeviceDirection::Output | DeviceDirection::Duplex => {
                self.create_output_internal(system_name, label, multi)?
            }
        };

        if direction != DeviceDirection::Duplex && !mix_sources.is_empty() {
            virtual_mic_mix::apply_virtual_mic_mix(&entry.to_device(), mix_sources)?;
        }

        Ok(())
    }

    fn remove_virtual_device(&self, system_name: &str) -> Result<(), BackendError> {
        self.registry.remove_device(system_name)
    }

    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo> {
        let _ = self.registry.discover_from_pactl();
        self.registry.list_devices().iter().map(|entry| entry.to_info()).collect()
    }

    fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError> {
        let _ = crate::backend::linux::pactl::sync_feed_sink_for_virtual_input(system_name, alias);
        let _ = self.registry.set_label(system_name, alias);
        if let Some(entry) = self.registry.get(system_name) {
            if let Ok(Some(new_module_id)) = crate::backend::linux::pactl::sync_virtual_device_description(
                system_name,
                entry.direction,
                &entry.module_id,
                alias,
            ) {
                let _ = self.registry.set_module_id(system_name, &new_module_id);
            }
        }
        Ok(())
    }

    fn platform_audio_version(&self) -> Option<String> {
        query_pipewire_version()
    }
}

fn query_pipewire_version() -> Option<String> {
    let output = Command::new("pw-cli").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_pipewire_version(&String::from_utf8_lossy(&output.stdout))
}

fn parse_pipewire_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("Linked with libpipewire "))
        .map(|version| version.trim().to_string())
}

fn notify_graph_listeners(
    cached_graph: &Arc<Mutex<RuntimeGraph>>,
    listener_slot: &Arc<Mutex<Option<GraphListener>>>,
) {
    let Ok(next_graph) = enumerate_pipewire() else {
        return;
    };
    let changed = {
        let mut current = cached_graph.lock().expect("graph lock poisoned");
        if *current != next_graph {
            *current = next_graph.clone();
            true
        } else {
            false
        }
    };
    if changed {
        if let Some(callback) = listener_slot
            .lock()
            .expect("listener lock poisoned")
            .as_ref()
        {
            callback(next_graph);
        }
    }
}

fn run_pw_dump_monitor(
    cached_graph: &Arc<Mutex<RuntimeGraph>>,
    listener_slot: &Arc<Mutex<Option<GraphListener>>>,
) -> bool {
    let mut child = match Command::new("pw-dump")
        .args(["-m"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let Some(stdout) = child.stdout.take() else {
        return false;
    };

    let reader = BufReader::new(stdout);
    // A dedicated reader thread lets the main loop coalesce bursts by
    // *waiting to go quiet* (or hitting MAX_COALESCE_WINDOW) rather than
    // firing one full graph refresh per line — under high churn, a burst of
    // pw-dump events collapses into a single refresh instead of a refresh
    // storm that never lets the graph settle.
    let (tx, rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        for line in reader.lines() {
            if line.is_err() || tx.send(()).is_err() {
                break;
            }
        }
    });

    loop {
        if rx.recv().is_err() {
            break;
        }

        let deadline = Instant::now() + MAX_COALESCE_WINDOW;
        loop {
            match rx.recv_timeout(MONITOR_DEBOUNCE) {
                Ok(()) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    notify_graph_listeners(cached_graph, listener_slot);
                    let _ = child.kill();
                    return false;
                }
            }
        }

        notify_graph_listeners(cached_graph, listener_slot);
    }

    let _ = child.kill();
    false
}

fn run_poll_loop(
    cached_graph: &Arc<Mutex<RuntimeGraph>>,
    listener_slot: &Arc<Mutex<Option<GraphListener>>>,
) {
    loop {
        thread::sleep(POLL_INTERVAL);
        notify_graph_listeners(cached_graph, listener_slot);
    }
}

fn enumerate_pipewire() -> Result<RuntimeGraph, BackendError> {
    let stdout = pw_dump::run_snapshot()?;
    if stdout.is_empty() {
        return Err(BackendError::Message(
            "pw-dump returned no data — is PipeWire running?".into(),
        ));
    }

    let objects: Vec<PwDumpObject> = serde_json::from_slice(&stdout).map_err(|error| {
        BackendError::Message(format!("failed to parse pw-dump output: {error}"))
    })?;

    let mut graph = pw_dump::normalize(&objects);
    graph_enrich::enrich_graph_from_pactl(&mut graph);
    Ok(graph)
}


#[cfg(test)]
mod version_tests {
    use super::parse_pipewire_version;

    #[test]
    fn parses_linked_with_line() {
        let output = "pw-cli\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5\n";
        assert_eq!(parse_pipewire_version(output), Some("1.0.5".to_string()));
    }

    #[test]
    fn none_for_unexpected_output() {
        assert_eq!(parse_pipewire_version("command not found"), None);
    }
}
