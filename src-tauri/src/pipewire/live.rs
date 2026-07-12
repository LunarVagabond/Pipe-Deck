use crate::core::models::RuntimeGraph;
use crate::pipewire::adapter::{AdapterError, GraphListener, PipeWireAdapter};
use crate::pipewire::graph_enrich;
use crate::pipewire::graph_routing;
use crate::pipewire::pw_dump::{self, PwDumpObject};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MONITOR_DEBOUNCE: Duration = Duration::from_millis(200);

pub struct LivePipeWireAdapter {
    cached_graph: Arc<Mutex<RuntimeGraph>>,
    listener: Arc<Mutex<Option<GraphListener>>>,
}

impl LivePipeWireAdapter {
    pub fn new() -> Result<Self, AdapterError> {
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

impl PipeWireAdapter for LivePipeWireAdapter {
    fn fetch_graph(&self) -> Result<RuntimeGraph, AdapterError> {
        match enumerate_pipewire() {
            Ok(graph) => {
                let mut cached = self
                    .cached_graph
                    .lock()
                    .map_err(|_| AdapterError::Message("graph lock poisoned".into()))?;
                *cached = graph.clone();
                Ok(graph)
            }
            Err(error) => {
                let cached = self
                    .cached_graph
                    .lock()
                    .map_err(|_| AdapterError::Message("graph lock poisoned".into()))?;
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

    fn subscribe(&self, listener: GraphListener) -> Result<(), AdapterError> {
        *self
            .listener
            .lock()
            .map_err(|_| AdapterError::Message("listener lock poisoned".into()))? =
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
    let mut last_refresh = Instant::now() - MONITOR_DEBOUNCE;

    for line in reader.lines() {
        if line.is_err() {
            break;
        }
        let elapsed = last_refresh.elapsed();
        if elapsed < MONITOR_DEBOUNCE {
            thread::sleep(MONITOR_DEBOUNCE - elapsed);
        }
        last_refresh = Instant::now();
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

fn enumerate_pipewire() -> Result<RuntimeGraph, AdapterError> {
    let stdout = pw_dump::run_snapshot()?;
    if stdout.is_empty() {
        return Err(AdapterError::Message(
            "pw-dump returned no data — is PipeWire running?".into(),
        ));
    }

    let objects: Vec<PwDumpObject> = serde_json::from_slice(&stdout).map_err(|error| {
        AdapterError::Message(format!("failed to parse pw-dump output: {error}"))
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
