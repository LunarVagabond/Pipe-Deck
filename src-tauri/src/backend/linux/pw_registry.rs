//! Native `pw::registry`-based graph watcher (#410, step 1 of #8's native
//! `pipewire-rs` migration) — replaces the `pw-dump` snapshot-and-diff/
//! poll-fallback transport `LinuxPipeWireBackend` used for graph fetch and
//! subscribe with a persistent, continuously-running
//! `pw::thread_loop::ThreadLoopRc` connection (the same lifecycle pattern
//! already proven in `pipewire/native_host.rs` — the fix for #303's
//! permanently-suspended-node bug: a manually-`iterate()`d
//! `pw::main_loop::MainLoopRc` is never listening the rest of the time,
//! while a `ThreadLoopRc` runs continuously on its own dedicated OS thread
//! for the life of the connection) that receives real add/remove events
//! from PipeWire's registry instead of re-shelling to `pw-dump` and
//! reparsing JSON on every refresh.
//!
//! Deliberately reuses `pw_dump::normalize` and `graph_enrich` unchanged:
//! a registry `global` event's `props` dict carries the exact same
//! key/value space `pw-dump`'s own JSON `info.props` does (`media.class`,
//! `node.name`, `audio.channels`, `node.rate`, `link.output.node`, ...), so
//! translating a live registry global into a synthetic `PwDumpObject` and
//! feeding it through the existing (already well-tested) normalize/label/
//! classification logic is far lower-risk than re-deriving that logic
//! against `DictRef` from scratch — the only real difference this module
//! introduces is *transport* (event push vs. shellout-and-reparse), which
//! is exactly what #410 set out to fix. See `docs/architecture/Decisions.md`
//! PD-040 for the full design writeup, including what this deliberately
//! does *not* yet replace (the `pactl` enrichment pass, and per-node
//! `Format` param round trips for sample rate — both flagged as follow-on
//! work, not silently dropped).
//!
//! Also maintains a plain `node.name -> id` index (`NativeGraphWatcher::
//! find_node_id`/`find_node_ids`, #411) over *every* live Node — including
//! `pipe-deck-*`/`effect_output.*`/`effect_input.*` names `pw_dump::normalize`
//! deliberately filters out of the UI-facing graph — so GUI-process call
//! sites that used to shell out to `pw-dump` just to resolve a node id
//! (`pipewire::pw_cli::find_node_id_by_name`'s original workaround, needed
//! because a registry `global` listener only ever fires once per object and
//! misses anything created before it attached) can query this connection's
//! already-current state instead. See PD-041.

use crate::backend::linux::graph_enrich;
use crate::backend::linux::pw_dump::{self, PwDumpObject};
use crate::backend::{BackendError, GraphListener};
use crate::core::models::RuntimeGraph;
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::registry::RegistryRc;
use pw::spa::utils::dict::DictRef;
use pw::thread_loop::ThreadLoopRc;
use pw::types::ObjectType;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// Same debounce/coalesce shape `run_pw_dump_monitor` (`live.rs`) already
// used for `pw-dump -m` line events — a burst of registry events (startup,
// or many nodes appearing/disappearing at once) collapses into a single
// rebuild instead of a rebuild storm that never lets the graph settle.
const DEBOUNCE: Duration = Duration::from_millis(200);
const MAX_COALESCE_WINDOW: Duration = Duration::from_millis(400);
// Registry `global`/`global_remove` only fires on object announce/removal,
// never on an existing node's param changes — so a volume/mute change made
// outside Pipe Deck (pavucontrol, another app) doesn't generate a registry
// event at all, only the `pactl`-shellout enrichment pass would notice it.
// A periodic re-enrich-and-notify tick (matching `live.rs`'s own
// `POLL_INTERVAL` fallback cadence) keeps that data from silently going
// stale between real topology events, without needing a native param
// listener per node (a bigger follow-on, not this spike's job — see
// PD-040).
const POLL_INTERVAL: Duration = Duration::from_secs(1);
// How long `start()` blocks for the first debounced build before returning
// anyway — matches `LinuxPipeWireBackend::new()`'s previous synchronous-seed
// behavior closely enough for callers not to notice the difference, without
// making this genuinely block forever if PipeWire is slow to answer.
const INITIAL_GRAPH_TIMEOUT: Duration = Duration::from_secs(2);

enum RegistryEvent {
    Global {
        id: u32,
        object_type: String,
        props: serde_json::Map<String, serde_json::Value>,
    },
    Removed {
        id: u32,
    },
}

/// Owns the live PipeWire connection for as long as it's alive. Every field
/// is wrapped in `ManuallyDrop` and **deliberately never torn down** —
/// confirmed against a real session that dropping these normally (thread
/// loop, context, core, registry, listener, in whatever order field-drop
/// glue picks) while the thread loop's background thread may still be
/// running triggers `impl_ext_end_proxy called from wrong context, check
/// thread and locking` from libpipewire. `pipewire/native_host.rs` hit the
/// same class of problem for its own long-lived connection and documents
/// the same resolution: never tear down, let process exit reclaim
/// everything, rather than get cross-thread teardown ordering exactly
/// right. Kept as a field on `LinuxPipeWireBackend` rather than a
/// process-wide `OnceLock` (`native_host.rs`'s pattern) since the backend
/// already has a single, naturally long-lived owner — no
/// lazy-init-from-anywhere requirement here. This matters concretely (not
/// just in theory) for `pipe-deck-cli`, whose one-shot `main()` runs real
/// destructors on clean exit, unlike the GUI/daemon processes which never
/// unwind this far.
pub struct NativeGraphWatcher {
    _thread_loop: std::mem::ManuallyDrop<ThreadLoopRc>,
    _context: std::mem::ManuallyDrop<ContextRc>,
    _core: std::mem::ManuallyDrop<CoreRc>,
    _registry: std::mem::ManuallyDrop<RegistryRc>,
    _global_listener: std::mem::ManuallyDrop<pw::registry::Listener>,
    // `node.name -> id` over every live Node (#411) — a plain, already-Sync
    // `Arc<Mutex<..>>`, unlike the `pw`-owned fields above; doesn't need or
    // affect the `unsafe impl`s below. Written by the assembler thread on
    // every registry event, read by `find_node_id`/`find_node_ids`.
    name_index: Arc<Mutex<HashMap<String, u32>>>,
}

// SAFETY: every touch of the `pw`-owned fields above happens either during
// setup (the calling thread, before `start()` returns) or from callbacks
// pw's own thread loop invokes on its own internal thread — never
// concurrently from two threads at once. This is the same contract
// `pipewire/native_host.rs`'s `NativeHost` relies on for its own
// `unsafe impl Send`. None of them are ever mutated again after `start()`
// returns (only read on `Drop`, which itself is a no-op — see the
// `ManuallyDrop` note above), so sharing `&NativeGraphWatcher` across
// threads has nothing to race on for those fields; `name_index` needs no
// such justification, being an ordinary `Arc<Mutex<..>>`.
unsafe impl Send for NativeGraphWatcher {}
unsafe impl Sync for NativeGraphWatcher {}

impl NativeGraphWatcher {
    /// Starts the connection and spawns a plain (non-PipeWire) assembler
    /// thread that debounces incoming registry events, rebuilds
    /// `cached_graph` via `pw_dump::normalize` + `graph_enrich`, and invokes
    /// `listener_slot`'s callback when the result actually changes — the
    /// same contract `run_pw_dump_monitor` provided, just event-driven
    /// instead of polling `pw-dump -m`'s stdout.
    ///
    /// Blocks up to `INITIAL_GRAPH_TIMEOUT` for that first build so a fresh
    /// `LinuxPipeWireBackend::new()` looks the same to its caller as
    /// today's synchronous `enumerate_pipewire()` seed. A timeout here is
    /// not an error — `cached_graph` (already seeded by the caller) gets
    /// updated the moment the assembler's first debounce window closes,
    /// same as any later PipeWire change. Only returns `Err` if PipeWire
    /// itself can't be reached at all (no thread loop/context/registry).
    pub fn start(
        cached_graph: Arc<Mutex<RuntimeGraph>>,
        listener_slot: Arc<Mutex<Option<GraphListener>>>,
    ) -> Result<Self, BackendError> {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);

        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { ThreadLoopRc::new(Some("pipe-deck-graph-watcher"), None) }
            .map_err(|error| {
                BackendError::Message(format!("failed to create PipeWire thread loop: {error}"))
            })?;
        thread_loop.start();

        let (tx, rx) = mpsc::channel::<RegistryEvent>();

        let (context, core, registry, global_listener) = {
            let _lock = thread_loop.lock();
            let context = ContextRc::new(&thread_loop, None).map_err(|error| {
                BackendError::Message(format!("failed to create PipeWire context: {error}"))
            })?;
            let core = context.connect_rc(None).map_err(|error| {
                BackendError::Message(format!("failed to connect to PipeWire: {error}"))
            })?;
            let registry = core.get_registry_rc().map_err(|error| {
                BackendError::Message(format!("failed to get PipeWire registry: {error}"))
            })?;

            let tx_global = tx.clone();
            let tx_remove = tx;
            let global_listener = registry
                .add_listener_local()
                .global(move |global| {
                    let Some((object_type, props)) = translate_global(global) else {
                        return;
                    };
                    let _ = tx_global.send(RegistryEvent::Global {
                        id: global.id,
                        object_type,
                        props,
                    });
                })
                .global_remove(move |id| {
                    let _ = tx_remove.send(RegistryEvent::Removed { id });
                })
                .register();

            (context, core, registry, global_listener)
        };

        // Plain std::thread, no PipeWire types involved — free to run pactl
        // enrichment (real subprocess calls) without blocking the thread
        // loop's own internal dispatch thread, which must stay responsive
        // for the connection's own protocol traffic.
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let name_index: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
        let assembler_name_index = name_index.clone();
        thread::spawn(move || {
            run_assembler(
                rx,
                cached_graph,
                listener_slot,
                assembler_name_index,
                ready_tx,
            )
        });

        let _ = ready_rx.recv_timeout(INITIAL_GRAPH_TIMEOUT);

        Ok(Self {
            _thread_loop: std::mem::ManuallyDrop::new(thread_loop),
            _context: std::mem::ManuallyDrop::new(context),
            _core: std::mem::ManuallyDrop::new(core),
            _registry: std::mem::ManuallyDrop::new(registry),
            _global_listener: std::mem::ManuallyDrop::new(global_listener),
            name_index,
        })
    }

    /// Resolves a live node's id from its `node.name` (#411) — covers every
    /// Node this connection has ever seen announced and not yet removed,
    /// including names `pw_dump::normalize` filters out of the UI-facing
    /// graph (`pipe-deck-*`, `effect_output.*`, `effect_input.*`). Returns
    /// `None` on a miss — callers should fall back to the original
    /// `pw-dump`-shellout lookup (`pipewire::pw_cli::find_node_id_by_name`)
    /// rather than treat a miss as "node doesn't exist": a node created
    /// within the last `DEBOUNCE`/`MAX_COALESCE_WINDOW` may not have reached
    /// this index yet.
    pub fn find_node_id(&self, node_name: &str) -> Option<u32> {
        self.name_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(node_name)
            .copied()
    }

    /// Bulk form of [`Self::find_node_id`] — one lock acquisition for
    /// several names at once, only returning the ones actually found
    /// (missing names are simply absent from the result, same "fall back to
    /// the shellout for what's missing" contract as the single-name form).
    pub fn find_node_ids(&self, names: &[String]) -> HashMap<String, u32> {
        let index = self
            .name_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        names
            .iter()
            .filter_map(|name| index.get(name).map(|id| (name.clone(), *id)))
            .collect()
    }
}

/// Only `Node`/`Link` globals become part of the graph — matches
/// `pw_dump::normalize`'s own filtering (it only ever looks at objects
/// whose type ends in `Interface:Node`/`Interface:Link`), so `Port`/
/// `Device`/`Client`/... globals are dropped here before they ever reach
/// the channel.
fn translate_global(
    global: &pw::registry::GlobalObject<&DictRef>,
) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    if !matches!(global.type_, ObjectType::Node | ObjectType::Link) {
        return None;
    }

    let mut props = serde_json::Map::new();
    if let Some(dict) = global.props {
        for (key, value) in dict.iter() {
            props.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    Some((global.type_.to_str().to_string(), props))
}

type LiveObjects = HashMap<u32, (String, serde_json::Map<String, serde_json::Value>)>;

fn run_assembler(
    rx: mpsc::Receiver<RegistryEvent>,
    cached_graph: Arc<Mutex<RuntimeGraph>>,
    listener_slot: Arc<Mutex<Option<GraphListener>>>,
    name_index: Arc<Mutex<HashMap<String, u32>>>,
    ready_tx: mpsc::Sender<()>,
) {
    let mut live: LiveObjects = HashMap::new();
    let mut ready_tx = Some(ready_tx);

    loop {
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(first) => {
                apply_event(&mut live, &name_index, first);

                // Coalesce a burst of events (startup, or many nodes
                // appearing/disappearing at once) into a single rebuild.
                // The name index itself (unlike `cached_graph`) is updated
                // per-event below, not just once at the end of this
                // coalescing — #411's callers want the smallest possible
                // miss window, not the same debounce this module's UI-facing
                // graph rebuild is fine waiting on.
                let deadline = Instant::now() + MAX_COALESCE_WINDOW;
                loop {
                    match rx.recv_timeout(DEBOUNCE) {
                        Ok(event) => {
                            apply_event(&mut live, &name_index, event);
                            if Instant::now() >= deadline {
                                break;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            rebuild_and_notify(&live, &cached_graph, &listener_slot, &mut ready_tx);
                            return;
                        }
                    }
                }

                rebuild_and_notify(&live, &cached_graph, &listener_slot, &mut ready_tx);
            }
            // No topology event this tick — still re-enrich from pactl and
            // notify if that alone changed something (see POLL_INTERVAL's
            // doc comment above).
            Err(RecvTimeoutError::Timeout) => {
                rebuild_and_notify(&live, &cached_graph, &listener_slot, &mut ready_tx);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn apply_event(
    live: &mut LiveObjects,
    name_index: &Mutex<HashMap<String, u32>>,
    event: RegistryEvent,
) {
    match event {
        RegistryEvent::Global {
            id,
            object_type,
            props,
        } => {
            if object_type == "PipeWire:Interface:Node" {
                if let Some(name) = props.get("node.name").and_then(|value| value.as_str()) {
                    name_index
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(name.to_string(), id);
                }
            }
            live.insert(id, (object_type, props));
        }
        RegistryEvent::Removed { id } => {
            if let Some((object_type, props)) = live.remove(&id) {
                if object_type == "PipeWire:Interface:Node" {
                    if let Some(name) = props.get("node.name").and_then(|value| value.as_str()) {
                        let mut index = name_index
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        // Only remove if this id is still the one on record
                        // for that name — guards against a rare
                        // remove-then-immediately-recreate-with-a-new-id
                        // race clobbering a just-inserted newer entry.
                        if index.get(name) == Some(&id) {
                            index.remove(name);
                        }
                    }
                }
            }
        }
    }
}

fn rebuild_and_notify(
    live: &LiveObjects,
    cached_graph: &Arc<Mutex<RuntimeGraph>>,
    listener_slot: &Arc<Mutex<Option<GraphListener>>>,
    ready_tx: &mut Option<mpsc::Sender<()>>,
) {
    let objects: Vec<PwDumpObject> = live
        .iter()
        .map(|(id, (object_type, props))| PwDumpObject {
            id: *id,
            object_type: object_type.clone(),
            info: Some(serde_json::json!({ "props": props })),
        })
        .collect();

    // `normalize` already runs `graph_enrich::finalize_graph` internally;
    // this mirrors `live.rs::enumerate_pipewire`'s second, separate
    // `enrich_graph_from_pactl` pass exactly, so pactl (not the native
    // registry) stays the source of truth for volume/mute/routing-target
    // data, unchanged — see this module's top-level doc comment and PD-040
    // for why that pass isn't replaced here.
    let mut next_graph = pw_dump::normalize(&objects);
    graph_enrich::enrich_graph_from_pactl(&mut next_graph);

    let changed = {
        let mut current = cached_graph.lock().expect("graph lock poisoned");
        if *current != next_graph {
            *current = next_graph.clone();
            true
        } else {
            false
        }
    };

    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(());
    }

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

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase (e.g.
    //! `core/engine/soundboard_ops.rs`).
    use super::*;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use crate::backend::AudioBackend;
    use std::path::Path;
    use std::sync::mpsc::RecvTimeoutError as ChanTimeout;

    #[test]
    #[ignore]
    fn fetch_graph_reflects_the_real_session_via_the_native_watcher() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let graph = backend.fetch_graph().expect("fetch_graph should succeed");

        assert_eq!(graph.data_source, "pipewire");
    }

    #[test]
    #[ignore]
    fn subscribe_receives_a_live_push_when_a_stream_appears_and_disappears() {
        // Deliberately drives this via a real `pw-cat --playback` stream
        // rather than a `pipe-deck-*` virtual device: `pw_dump::normalize`
        // filters out every `pipe-deck-*` node unconditionally (see
        // `pipe_deck_devices_are_left_to_virtual_registry` in
        // `pw_dump.rs`'s own tests) — those get merged into the graph by
        // `CoreEngine::merge_virtual_devices` (`core/engine/virtual_ops.rs`),
        // a layer above `LinuxPipeWireBackend`, so exercising this backend
        // directly (as this test does) would never see one appear no matter
        // how the graph is fetched.
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let clip = Path::new("/usr/share/sounds/speech-dispatcher/test.wav");
        assert!(
            clip.is_file(),
            "expected a system test wav to exist at {}",
            clip.display()
        );

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");

        let (tx, rx) = mpsc::channel::<RuntimeGraph>();
        backend
            .subscribe(Box::new(move |graph| {
                let _ = tx.send(graph);
            }))
            .expect("subscribe should succeed");

        // `pw-cat` against a made-up target still plays into the default
        // sink — good enough to generate a real, short-lived stream node.
        let mut child = crate::sysproc::command("pw-cat")
            .args(["--playback"])
            .arg(clip)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn pw-cat");

        let saw_creation = (0..20).any(|_| match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(graph) => graph
                .streams
                .iter()
                .any(|stream| stream.app_name.contains("pw-cat")),
            Err(ChanTimeout::Timeout) => false,
            Err(ChanTimeout::Disconnected) => false,
        });

        let _ = child.kill();
        let _ = child.wait();

        let saw_removal = (0..20).any(|_| match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(graph) => !graph
                .streams
                .iter()
                .any(|stream| stream.app_name.contains("pw-cat")),
            Err(ChanTimeout::Timeout) => false,
            Err(ChanTimeout::Disconnected) => false,
        });

        assert!(
            saw_creation,
            "expected a live graph push reflecting the new pw-cat stream"
        );
        assert!(
            saw_removal,
            "expected a live graph push reflecting the stream's removal"
        );
    }

    #[test]
    #[ignore]
    fn find_live_node_id_resolves_a_pipe_deck_device_hidden_from_the_ui_graph() {
        // The whole point of #411: `pipe-deck-*` node names are exactly what
        // `pw_dump::normalize` filters out of the UI-facing graph (see the
        // previous test's comment) — proving `find_live_node_id` resolves
        // one anyway is what distinguishes this from just re-testing #410's
        // graph watcher.
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");

        let created = backend
            .create_virtual_input("Pipe Deck Node Lookup Test")
            .expect("create disposable test device");

        // The live registry index updates per-event, not on the graph's own
        // debounce window (see `run_assembler`'s doc comment) — but still
        // asynchronous relative to `create_virtual_input` returning, so poll
        // briefly rather than asserting on the very first attempt.
        let found = (0..20).find_map(|_| {
            let id = backend
                .find_live_node_id(&created.system_name)
                .ok()
                .flatten();
            if id.is_some() {
                return id;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });

        let _ = backend.remove_virtual_device(&created.system_name);

        assert!(
            found.is_some(),
            "expected find_live_node_id to resolve the disposable virtual device"
        );
    }
}
