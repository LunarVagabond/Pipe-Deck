use crate::core::models::RuntimeGraph;
use crate::backend::{BackendError, GraphListener, AudioBackend};
use crate::backend::linux::graph_enrich;
use crate::backend::linux::graph_routing;
use crate::backend::linux::pw_dump::{self, PwDumpObject};
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

        Ok(Self {
            cached_graph,
            listener,
        })
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

pub use graph_enrich::{apply_device_aliases, apply_device_levels, enrich_graph_from_pactl};
pub use graph_routing::{
    apply_graph_routing, apply_user_cleared_routes, normalize_stream_routing_links,
    sync_live_routing_graph,
};
