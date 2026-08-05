//! Native `pw::registry`/`pw::link::Link` port-linking backend (#412, step 3
//! of #8's native `pipewire-rs` migration). Replaces `pw_link.rs`'s
//! `pw-link -o`/`-i`/`-l` port discovery and `pw-link src dst`/`-d` link
//! creation/teardown with native registry enumeration and `Link` proxy
//! creation, using the same continuously-running `pw::thread_loop::ThreadLoopRc`
//! lifecycle `pw_registry.rs` (#410) and `pipewire/native_host.rs` (#303's fix)
//! already established.
//!
//! Unlike #410/#411's `NativeGraphWatcher` (owned by the single
//! `LinuxPipeWireBackend` instance), this connection is reached from many
//! call sites scattered across `backend::linux::*` and even `core::engine::*`
//! (`virtual_mic_mix`, `pactl::routing`, `pactl::virtual`, `graph_routing`,
//! `split_sink`, `virtual_devices`, `effects_ops`, `processing_node_ops`) with
//! no single natural owner to thread a connection reference through —
//! exactly the "lazy-init-from-anywhere" case PD-040 flagged as the reason
//! *not* to use a process-wide `OnceLock` for the graph watcher. This module
//! is the case where that tradeoff flips: see PD-042.
//!
//! Every public function here mirrors one in `pw_link.rs` but returns
//! `Option<...>` — `None` means "couldn't be attempted natively" (the
//! connection failed to start, or a node/port this call needs hasn't been
//! indexed yet) and the caller falls back to the `pw-link` CLI implementation
//! unchanged, same "fall back to the shellout for what's missing" contract
//! #411 established for node-id lookup. `Some(_)` means the native path ran
//! to completion (successfully or not) and its result should be used as-is.
//!
//! `route_playback_stream`/`route_capture_stream` (#428, the last slice of
//! #413) extend this same port-linking machinery to stream routing
//! (`pactl move-sink-input`/`move-source-output`'s replacement): a stream is
//! just another `Stream/Output`/`Stream/Input`-class node — confirmed live
//! that `core::models::Stream.system_name` is already populated from the
//! live node's `node.name`, same as any device — so "moving" one is the same
//! disconnect-old/link-new operation as device-to-device routing, just
//! *exclusive* (disconnect every existing link first, not only the ones to
//! one particular target) since a stream draws from or sends to exactly one
//! place, unlike a sink's monitor fan-out. See PD-047.

use crate::backend::BackendError;
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::properties::properties;
use pw::registry::RegistryRc;
use pw::spa::utils::dict::DictRef;
use pw::thread_loop::ThreadLoopRc;
use pw::types::ObjectType;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

/// How long a fresh connection blocks for its first batch of registry events
/// before `connection()` returns it — matches `pw_registry.rs`'s
/// `INITIAL_GRAPH_TIMEOUT` reasoning: not required for correctness (the
/// index keeps filling in after this), just keeps the very first call after
/// process start from racing an empty index unnecessarily.
const INITIAL_INDEX_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PortDirection {
    Input,
    Output,
}

#[derive(Clone)]
struct PortRecord {
    node_id: u32,
    name: String,
    direction: PortDirection,
}

enum RegistryEvent {
    Node { id: u32, name: String },
    Port { id: u32, node_id: u32, name: String, direction: PortDirection },
    Link { id: u32, output_port: u32, input_port: u32 },
    Removed { id: u32 },
}

struct Index {
    node_ids: HashMap<String, u32>,
    node_names: HashMap<u32, String>,
    ports: HashMap<u32, PortRecord>,
    // link global id -> (output port id, input port id)
    links: HashMap<u32, (u32, u32)>,
}

impl Index {
    fn new() -> Self {
        Self { node_ids: HashMap::new(), node_names: HashMap::new(), ports: HashMap::new(), links: HashMap::new() }
    }

    fn apply(&mut self, event: RegistryEvent) {
        match event {
            RegistryEvent::Node { id, name } => {
                self.node_ids.insert(name.clone(), id);
                self.node_names.insert(id, name);
            }
            RegistryEvent::Port { id, node_id, name, direction } => {
                self.ports.insert(id, PortRecord { node_id, name, direction });
            }
            RegistryEvent::Link { id, output_port, input_port } => {
                self.links.insert(id, (output_port, input_port));
            }
            RegistryEvent::Removed { id } => {
                if let Some(name) = self.node_names.remove(&id) {
                    // Only remove if this id is still on record for that
                    // name — same remove-then-recreate race guard
                    // `pw_registry.rs::apply_event` uses for its own index.
                    if self.node_ids.get(&name) == Some(&id) {
                        self.node_ids.remove(&name);
                    }
                }
                self.ports.remove(&id);
                self.links.remove(&id);
            }
        }
    }
}

/// Owns the live PipeWire connection for the process's life — same
/// deliberate never-torn-down `ManuallyDrop` pattern `pw_registry.rs`'s
/// `NativeGraphWatcher` uses, and for the identical reason: a real session
/// confirmed dropping a thread loop / context / core / registry / listener
/// while the thread loop's background thread might still be running trips
/// `impl_ext_end_proxy called from wrong context`. Kept as a process-wide
/// `OnceLock` (unlike `NativeGraphWatcher`) per this module's top-level doc
/// comment.
struct Connection {
    _thread_loop: ManuallyDrop<ThreadLoopRc>,
    _context: ManuallyDrop<ContextRc>,
    core: ManuallyDrop<CoreRc>,
    registry: ManuallyDrop<RegistryRc>,
    _listener: ManuallyDrop<pw::registry::Listener>,
    index: Arc<Mutex<Index>>,
}

// SAFETY: identical contract to `pw_registry.rs::NativeGraphWatcher`'s own
// `unsafe impl Send/Sync` — every touch of the `pw`-owned fields happens
// either during setup (this thread, before `start()` returns) or from
// callbacks pw's own thread loop invokes on its internal thread, never
// concurrently. `core`/`registry` are read (not mutated) after `start()` to
// issue `create_object`/`destroy_global` calls, which is exactly what those
// types are designed to allow from an outside thread while the loop is
// locked (see `native_host.rs`'s identical justification for its own
// `NativeHost`). `index` is a plain `Arc<Mutex<..>>`.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    fn start() -> Option<Self> {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);

        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { ThreadLoopRc::new(Some("pipe-deck-port-linker"), None) }.ok()?;
        thread_loop.start();

        let (tx, rx) = mpsc::channel::<RegistryEvent>();

        let (context, core, registry, listener) = {
            let _lock = thread_loop.lock();
            let context = ContextRc::new(&thread_loop, None).ok()?;
            let core = context.connect_rc(None).ok()?;
            let registry = core.get_registry_rc().ok()?;

            let tx_global = tx.clone();
            let tx_remove = tx;
            let listener = registry
                .add_listener_local()
                .global(move |global| {
                    if let Some(event) = translate_global(global) {
                        let _ = tx_global.send(event);
                    }
                })
                .global_remove(move |id| {
                    let _ = tx_remove.send(RegistryEvent::Removed { id });
                })
                .register();

            (context, core, registry, listener)
        };

        let index = Arc::new(Mutex::new(Index::new()));
        let assembler_index = index.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        thread::spawn(move || run_assembler(rx, assembler_index, ready_tx));
        let _ = ready_rx.recv_timeout(INITIAL_INDEX_TIMEOUT);

        Some(Self {
            _thread_loop: ManuallyDrop::new(thread_loop),
            _context: ManuallyDrop::new(context),
            core: ManuallyDrop::new(core),
            registry: ManuallyDrop::new(registry),
            _listener: ManuallyDrop::new(listener),
            index,
        })
    }

    fn node_id(&self, name: &str) -> Option<u32> {
        self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).node_ids.get(name).copied()
    }

    fn node_id_for_port(&self, port_id: u32) -> Option<u32> {
        self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).ports.get(&port_id).map(|port| port.node_id)
    }

    fn node_name_for_port(&self, port_id: u32) -> Option<String> {
        let index = self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let node_id = index.ports.get(&port_id)?.node_id;
        index.node_names.get(&node_id).cloned()
    }

    /// Every port belonging to `node_id` in the given `direction`, as
    /// `(port global id, port.name)` — the native equivalent of
    /// `pw_link.rs::output_ports_for`/`target_ports_with_prefix`, minus the
    /// name-prefix filtering those apply, since callers already know
    /// direction here and filter by prefix themselves.
    fn ports_for_node(&self, node_id: u32, direction: PortDirection) -> Vec<(u32, String)> {
        self.index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .ports
            .iter()
            .filter(|(_, port)| port.node_id == node_id && port.direction == direction)
            .map(|(id, port)| (*id, port.name.clone()))
            .collect()
    }

    /// Every currently-linked `(output_port_id, input_port_id)` pair whose
    /// output port belongs to one of `output_port_ids` — the native
    /// equivalent of `pw_link.rs::links_from_source`.
    fn links_from_output_ports(&self, output_port_ids: &[u32]) -> Vec<(u32, u32)> {
        let index = self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        index.links.values().filter(|(output, _)| output_port_ids.contains(output)).copied().collect()
    }

    /// The input-side counterpart to [`Self::links_from_output_ports`] — every
    /// currently-linked pair whose *input* port belongs to `input_port_ids`,
    /// regardless of which node the output side belongs to. Needed for stream
    /// routing (#428): a capture-direction stream's own input ports can be
    /// fed from exactly one source at a time, so "what currently feeds this
    /// stream" has to be found from the input side, unlike every earlier
    /// query in this module (all of which start from a known *output* side —
    /// a device's monitor/capture ports fanning out to one or more targets).
    fn links_into_input_ports(&self, input_port_ids: &[u32]) -> Vec<(u32, u32)> {
        let index = self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        index.links.values().filter(|(_, input)| input_port_ids.contains(input)).copied().collect()
    }

    /// Finds the link global id (if any) currently connecting `output_port_id`
    /// to `input_port_id`, needed since destruction goes through
    /// `registry.destroy_global(link_id)`, not the port pair itself.
    fn find_link_id(&self, output_port_id: u32, input_port_id: u32) -> Option<u32> {
        let index = self.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        index
            .links
            .iter()
            .find(|(_, (output, input))| *output == output_port_id && *input == input_port_id)
            .map(|(id, _)| *id)
    }

    /// Creates a link between two ports via the well-known `link-factory`
    /// (the same factory name the `pw-link` CLI itself uses internally).
    /// `object.linger => "1"` tells the server not to destroy the link when
    /// this process drops its local proxy handle immediately after — without
    /// it, the `Link` returned by `create_object` would tear the link right
    /// back down the moment it goes out of scope (`Proxy`'s `Drop` calls
    /// `pw_proxy_destroy`), the same footgun `pipewire-rs`'s own
    /// `create-delete-remote-objects` example flags. This is what lets a link
    /// this connection creates persist exactly like one `pw-link src dst`
    /// creates and then exits without tearing down.
    fn create_link(
        &self,
        output_node_id: u32,
        output_port_id: u32,
        input_node_id: u32,
        input_port_id: u32,
    ) -> Result<(), BackendError> {
        let _lock = self._thread_loop.lock();
        self.core
            .create_object::<pw::link::Link>(
                "link-factory",
                &properties! {
                    "link.output.node" => output_node_id.to_string().as_str(),
                    "link.output.port" => output_port_id.to_string().as_str(),
                    "link.input.node" => input_node_id.to_string().as_str(),
                    "link.input.port" => input_port_id.to_string().as_str(),
                    "object.linger" => "1",
                },
            )
            .map(|_link: pw::link::Link| ())
            .map_err(|error| BackendError::Message(format!("failed to create native link: {error}")))
    }

    fn destroy_link(&self, link_id: u32) -> Result<(), BackendError> {
        let _lock = self._thread_loop.lock();
        // `destroy_global` reports success as either sync or async depending
        // on server timing — the request has already been sent to the server
        // either way, and this connection's own registry listener will pick
        // up the resulting `global_remove` event and update `index` itself,
        // so there's nothing further to wait on here.
        self.registry
            .destroy_global(link_id)
            .into_result()
            .map(|_| ())
            .map_err(|error| BackendError::Message(format!("failed to destroy native link {link_id}: {error}")))
    }
}

static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();

fn connection() -> Option<&'static Connection> {
    CONNECTION.get_or_init(Connection::start).as_ref()
}

fn translate_global(global: &pw::registry::GlobalObject<&DictRef>) -> Option<RegistryEvent> {
    let props = global.props?;
    match global.type_ {
        ObjectType::Node => {
            let name = props.get("node.name")?.to_string();
            Some(RegistryEvent::Node { id: global.id, name })
        }
        ObjectType::Port => {
            let name = props.get("port.name")?.to_string();
            let node_id: u32 = props.get("node.id")?.parse().ok()?;
            let direction = match props.get("port.direction")? {
                "in" => PortDirection::Input,
                "out" => PortDirection::Output,
                _ => return None,
            };
            Some(RegistryEvent::Port { id: global.id, node_id, name, direction })
        }
        ObjectType::Link => {
            let output_port: u32 = props.get("link.output.port")?.parse().ok()?;
            let input_port: u32 = props.get("link.input.port")?.parse().ok()?;
            Some(RegistryEvent::Link { id: global.id, output_port, input_port })
        }
        _ => None,
    }
}

/// Plain `std::thread`, no PipeWire types involved (same shape as
/// `pw_registry.rs::run_assembler`) — applies every incoming event to `index`
/// immediately, with no debounce: unlike the graph watcher's rebuild (an
/// O(objects) `normalize`+`enrich` pass worth batching), updating a hash map
/// entry is cheap enough that batching would only add latency for no benefit,
/// and callers want the smallest possible miss window (same reasoning
/// #411's `name_index` update already used).
fn run_assembler(rx: mpsc::Receiver<RegistryEvent>, index: Arc<Mutex<Index>>, ready_tx: mpsc::Sender<()>) {
    let mut ready_tx = Some(ready_tx);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => {
                index.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).apply(event);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if let Some(tx) = ready_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Pairs source ports with target ports, cycling the (sorted) source list
/// across every (sorted) target port — identical semantics to
/// `pw_link.rs::pair_capture_ports`, operating on `(id, name)` pairs sorted
/// by `name` instead of `"system_name:port_name"` strings (equivalent, since
/// the system-name prefix is constant within a single node's port list).
fn pair_ports(source_ports: &[(u32, String)], target_ports: &[(u32, String)]) -> Vec<(u32, u32)> {
    let mut sorted_sources = source_ports.to_vec();
    sorted_sources.sort_by(|a, b| a.1.cmp(&b.1));
    let mut sorted_targets = target_ports.to_vec();
    sorted_targets.sort_by(|a, b| a.1.cmp(&b.1));

    sorted_targets
        .into_iter()
        .enumerate()
        .map(|(index, (target_id, _))| (sorted_sources[index % sorted_sources.len()].0, target_id))
        .collect()
}

fn route_matches(conn: &Connection, source_port_ids: &[u32], desired: &[(u32, u32)]) -> bool {
    let existing = conn.links_from_output_ports(source_port_ids);
    desired.iter().all(|pair| existing.contains(pair))
}

/// Reconciles `existing` against `desired` exactly like
/// `pw_link.rs::apply_link_diff` — only disconnects pairs that shouldn't be
/// there anymore and links only the pairs that are missing.
fn apply_link_diff(
    conn: &Connection,
    output_node_id: u32,
    input_node_id: u32,
    existing: &[(u32, u32)],
    desired: &[(u32, u32)],
) -> Result<(), BackendError> {
    let to_remove: Vec<(u32, u32)> = existing.iter().filter(|pair| !desired.contains(pair)).copied().collect();
    destroy_pairs(conn, &to_remove)?;

    for &(output_port, input_port) in desired {
        if !existing.contains(&(output_port, input_port)) {
            conn.create_link(output_node_id, output_port, input_node_id, input_port)?;
        }
    }

    Ok(())
}

/// Destroys every link currently connecting each `(output_port_id,
/// input_port_id)` pair, continuing past individual failures (mirrors
/// `pw_link.rs::disconnect_links`'s tolerance of a link already gone by the
/// time it gets processed) but collecting and returning real failures rather
/// than discarding them.
fn destroy_pairs(conn: &Connection, pairs: &[(u32, u32)]) -> Result<(), BackendError> {
    let mut failures = Vec::new();
    for &(output_port, input_port) in pairs {
        let Some(link_id) = conn.find_link_id(output_port, input_port) else {
            continue;
        };
        if let Err(error) = conn.destroy_link(link_id) {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(BackendError::Message(format!(
            "failed to disconnect {} native link(s): {}",
            failures.len(),
            failures.join("; ")
        )))
    }
}

/// Native equivalent of `pw_link.rs::link_capture_source_to_target_ports`.
/// `None` if the connection isn't up yet, or either endpoint's node/ports
/// aren't indexed yet — the caller should fall back to the `pw-link` CLI
/// path in that case.
fn link_target_ports(
    source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let output_node_id = conn.node_id(source_system_name)?;
    let input_node_id = conn.node_id(target_system_name)?;

    let source_ports = conn.ports_for_node(output_node_id, PortDirection::Output);
    if source_ports.is_empty() {
        return None;
    }
    let target_ports: Vec<(u32, String)> = conn
        .ports_for_node(input_node_id, PortDirection::Input)
        .into_iter()
        .filter(|(_, name)| name.starts_with(target_port_prefix))
        .collect();
    if target_ports.is_empty() {
        return None;
    }

    let desired = pair_ports(&source_ports, &target_ports);
    let source_port_ids: Vec<u32> = source_ports.iter().map(|(id, _)| *id).collect();
    if route_matches(conn, &source_port_ids, &desired) {
        return Some(Ok(()));
    }

    let target_port_ids: std::collections::HashSet<u32> = target_ports.iter().map(|(id, _)| *id).collect();
    let existing: Vec<(u32, u32)> = conn
        .links_from_output_ports(&source_port_ids)
        .into_iter()
        .filter(|(_, input)| target_port_ids.contains(input))
        .collect();

    Some(apply_link_diff(conn, output_node_id, input_node_id, &existing, &desired))
}

/// Native equivalent of
/// `pw_link.rs::disconnect_capture_source_from_target_ports` — an empty
/// `target_port_prefix` matches every input port of `target_system_name`,
/// which is what `disconnect_sink_monitor_route` (no prefix restriction in
/// the CLI version) needs.
fn disconnect_target_ports(
    source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let output_node_id = conn.node_id(source_system_name)?;
    let input_node_id = conn.node_id(target_system_name)?;

    let source_port_ids: Vec<u32> =
        conn.ports_for_node(output_node_id, PortDirection::Output).into_iter().map(|(id, _)| id).collect();
    let target_port_ids: std::collections::HashSet<u32> = conn
        .ports_for_node(input_node_id, PortDirection::Input)
        .into_iter()
        .filter(|(_, name)| name.starts_with(target_port_prefix))
        .map(|(id, _)| id)
        .collect();

    let to_remove: Vec<(u32, u32)> = conn
        .links_from_output_ports(&source_port_ids)
        .into_iter()
        .filter(|(_, input)| target_port_ids.contains(input))
        .collect();

    Some(destroy_pairs(conn, &to_remove))
}

pub fn link_capture_source_to_virtual_input(
    capture_source_system_name: &str,
    virtual_input_system_name: &str,
) -> Option<Result<(), BackendError>> {
    link_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

pub fn disconnect_capture_source_from_virtual_input(
    capture_source_system_name: &str,
    virtual_input_system_name: &str,
) -> Option<Result<(), BackendError>> {
    disconnect_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

pub fn link_capture_source_to_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Option<Result<(), BackendError>> {
    link_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

pub fn disconnect_capture_source_from_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Option<Result<(), BackendError>> {
    disconnect_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

pub fn link_sink_monitor_to_target(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> Option<Result<(), BackendError>> {
    let prefix = if target_is_virtual_source { "input_" } else { "playback_" };
    link_target_ports(source_system_name, target_system_name, prefix)
}

pub fn is_sink_monitor_routed_to(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> Option<bool> {
    let prefix = if target_is_virtual_source { "input_" } else { "playback_" };
    let conn = connection()?;
    let output_node_id = conn.node_id(source_system_name)?;
    let input_node_id = conn.node_id(target_system_name)?;

    let source_ports = conn.ports_for_node(output_node_id, PortDirection::Output);
    let target_ports: Vec<(u32, String)> =
        conn.ports_for_node(input_node_id, PortDirection::Input).into_iter().filter(|(_, name)| name.starts_with(prefix)).collect();
    if source_ports.is_empty() || target_ports.is_empty() {
        return None;
    }

    let desired = pair_ports(&source_ports, &target_ports);
    let source_port_ids: Vec<u32> = source_ports.iter().map(|(id, _)| *id).collect();
    Some(route_matches(conn, &source_port_ids, &desired))
}

pub fn list_all_monitor_routes_for_source(source_system_name: &str) -> Option<Vec<String>> {
    let conn = connection()?;
    let source_node_id = conn.node_id(source_system_name)?;
    let source_port_ids: Vec<u32> =
        conn.ports_for_node(source_node_id, PortDirection::Output).into_iter().map(|(id, _)| id).collect();

    let mut targets = Vec::new();
    for (_, input_port_id) in conn.links_from_output_ports(&source_port_ids) {
        if let Some(name) = conn.node_name_for_port(input_port_id) {
            if !targets.contains(&name) {
                targets.push(name);
            }
        }
    }
    Some(targets)
}

pub fn disconnect_sink_monitor_route(
    source_system_name: &str,
    target_system_name: &str,
) -> Option<Result<(), BackendError>> {
    disconnect_target_ports(source_system_name, target_system_name, "")
}

pub fn disconnect_sink_monitor(source_system_name: &str) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let source_node_id = conn.node_id(source_system_name)?;
    let source_port_ids: Vec<u32> =
        conn.ports_for_node(source_node_id, PortDirection::Output).into_iter().map(|(id, _)| id).collect();
    let existing = conn.links_from_output_ports(&source_port_ids);
    Some(destroy_pairs(conn, &existing))
}

/// Input-side counterpart to [`disconnect_sink_monitor`] (#428) — removes
/// every link currently feeding `target_system_name`'s input ports,
/// regardless of source. Used to clear a capture-direction stream's current
/// routing before re-routing it elsewhere, since (unlike a sink's monitor,
/// which can legitimately fan out to several targets) a stream only ever
/// draws from one source at a time.
pub fn disconnect_all_inputs(target_system_name: &str) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let target_node_id = conn.node_id(target_system_name)?;
    let target_port_ids: Vec<u32> =
        conn.ports_for_node(target_node_id, PortDirection::Input).into_iter().map(|(id, _)| id).collect();
    let existing = conn.links_into_input_ports(&target_port_ids);
    Some(destroy_pairs(conn, &existing))
}

/// Exclusively routes a playback-direction stream's output ports into
/// `target_system_name`'s `playback_*` ports (#428) — first disconnecting
/// *every* existing output link the stream currently has (a stream routes to
/// exactly one target, unlike a sink's monitor fan-out), then linking fresh.
/// `target_system_name` is whatever `pactl/routing.rs::resolve_playback_sink_name`
/// already resolved it to (the target device directly, a per-mix-source feed
/// sink, or the synthetic "Unrouted" sink) — always a genuine sink-shaped
/// node with `playback_*` ports, so no direction/prefix branching is needed
/// here the way device-to-device routing needs `target_is_virtual_source`.
/// The stream's own ports are never prefix-filtered (same as every other
/// "source" side in this module) since an arbitrary app's port names follow
/// no Pipe Deck naming convention.
pub fn route_playback_stream(stream_system_name: &str, target_system_name: &str) -> Option<Result<(), BackendError>> {
    if let Some(Err(error)) = disconnect_sink_monitor(stream_system_name) {
        return Some(Err(error));
    }
    link_target_ports(stream_system_name, target_system_name, "playback_")
}

/// Exclusively routes `source_system_name` (the real capture-providing
/// device — hardware mic, virtual mic, ...) into a capture-direction
/// stream's own input ports (#428) — first disconnecting every existing
/// link currently feeding the stream, then linking fresh. The stream's own
/// ports are matched with an empty prefix (any input port), for the same
/// arbitrary-naming reason `route_playback_stream` never prefix-filters the
/// stream side either.
pub fn route_capture_stream(source_system_name: &str, stream_system_name: &str) -> Option<Result<(), BackendError>> {
    if let Some(Err(error)) = disconnect_all_inputs(stream_system_name) {
        return Some(Err(error));
    }
    link_target_ports(source_system_name, stream_system_name, "")
}

pub fn has_output_ports(system_name: &str) -> Option<bool> {
    let conn = connection()?;
    let node_id = conn.node_id(system_name)?;
    Some(!conn.ports_for_node(node_id, PortDirection::Output).is_empty())
}

pub fn has_input_ports(system_name: &str) -> Option<bool> {
    let conn = connection()?;
    let node_id = conn.node_id(system_name)?;
    Some(!conn.ports_for_node(node_id, PortDirection::Input).is_empty())
}

pub fn list_capture_sources_for_virtual_input(virtual_input_system_name: &str) -> Option<Vec<String>> {
    list_capture_sources_for_target_ports(virtual_input_system_name, "input_")
}

pub fn list_capture_sources_for_sink(sink_system_name: &str) -> Option<Vec<String>> {
    list_capture_sources_for_target_ports(sink_system_name, "playback_")
}

/// Native equivalent of `pw_link.rs::list_capture_sources_for_stream` — an
/// empty port-name prefix, since a stream's own input ports follow no Pipe
/// Deck naming convention (same reasoning `route_capture_stream` already
/// applies here).
pub fn list_capture_sources_for_stream(stream_system_name: &str) -> Option<Vec<String>> {
    list_capture_sources_for_target_ports(stream_system_name, "")
}

fn list_capture_sources_for_target_ports(target_system_name: &str, target_port_prefix: &str) -> Option<Vec<String>> {
    let conn = connection()?;
    let target_node_id = conn.node_id(target_system_name)?;
    let target_port_ids: std::collections::HashSet<u32> = conn
        .ports_for_node(target_node_id, PortDirection::Input)
        .into_iter()
        .filter(|(_, name)| name.starts_with(target_port_prefix))
        .map(|(id, _)| id)
        .collect();
    if target_port_ids.is_empty() {
        return None;
    }

    let index = conn.index.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut sources = Vec::new();
    for (output_port_id, input_port_id) in index.links.values() {
        if !target_port_ids.contains(input_port_id) {
            continue;
        }
        let Some(node_id) = index.ports.get(output_port_id).map(|port| port.node_id) else {
            continue;
        };
        if let Some(name) = index.node_names.get(&node_id) {
            if !sources.contains(name) {
                sources.push(name.clone());
            }
        }
    }
    Some(sources)
}

/// Native equivalent of `pw_link.rs::disconnect_stale_output_links` — removes
/// every output link from `source_system_name` whose target port does *not*
/// belong to `keep_target_system_name`.
pub fn disconnect_stale_output_links(
    source_system_name: &str,
    keep_target_system_name: &str,
) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let source_node_id = conn.node_id(source_system_name)?;
    let keep_node_id = conn.node_id(keep_target_system_name)?;
    let source_port_ids: Vec<u32> =
        conn.ports_for_node(source_node_id, PortDirection::Output).into_iter().map(|(id, _)| id).collect();

    let stale: Vec<(u32, u32)> = conn
        .links_from_output_ports(&source_port_ids)
        .into_iter()
        .filter(|(_, input_port_id)| conn.node_id_for_port(*input_port_id) != Some(keep_node_id))
        .collect();

    Some(destroy_pairs(conn, &stale))
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase (e.g.
    //! `pw_registry.rs::live_tests`). Verifies the native path itself —
    //! `Some(_)` from the functions under test, not a silent fallback to the
    //! CLI implementation `pw_link.rs` would use if the native connection
    //! never got the pair of nodes indexed — and cross-checks the resulting
    //! link topology against `pw-link -l` directly, independent of this
    //! module's own `Index`.
    use super::*;
    use crate::backend::AudioBackend;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use std::thread;
    use std::time::Duration;

    /// Polls `connection()`'s index for both node names to appear, up to a
    /// few seconds — a freshly created virtual device's registry event needs
    /// to reach this module's own (separate from `LinuxPipeWireBackend`'s
    /// `NativeGraphWatcher`) assembler thread before any of the functions
    /// under test can do anything but fall back to the CLI.
    fn wait_until_indexed(names: &[&str]) {
        let conn = connection().expect("native port-linking connection should start against a real session");
        for _ in 0..50 {
            if names.iter().all(|name| conn.node_id(name).is_some()) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("timed out waiting for {names:?} to appear in the native port-linking index");
    }

    #[test]
    #[ignore]
    fn links_two_virtual_devices_natively_and_disconnects_again() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let source =
            backend.create_virtual_output("Pipe Deck Native Link Source Test", false).expect("create disposable source");
        let target = backend.create_virtual_input("Pipe Deck Native Link Target Test").expect("create disposable target");

        wait_until_indexed(&[&source.system_name, &target.system_name]);

        let link_result = link_sink_monitor_to_target(&source.system_name, &target.system_name, true);
        assert!(link_result.is_some(), "expected the native path to run, not fall back to the CLI");
        link_result.unwrap().expect("native link creation should succeed");

        assert!(pw_link_l_shows_a_link_between(&source.system_name, &target.system_name), "expected `pw-link -l` to show the new link");

        let routed = is_sink_monitor_routed_to(&source.system_name, &target.system_name, true);
        assert_eq!(routed, Some(true));

        let disconnect_result = disconnect_sink_monitor(&source.system_name);
        assert!(disconnect_result.is_some());
        disconnect_result.unwrap().expect("native disconnect should succeed");

        let disconnected = (0..20).any(|_| {
            if !pw_link_l_shows_a_link_between(&source.system_name, &target.system_name) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(disconnected, "expected `pw-link -l` to no longer show the link after disconnect");

        let _ = backend.remove_virtual_device(&source.system_name);
        let _ = backend.remove_virtual_device(&target.system_name);
    }

    fn pw_link_l_shows_a_link_between(source_system_name: &str, target_system_name: &str) -> bool {
        let output = crate::sysproc::command("pw-link").arg("-l").output().expect("failed to run pw-link -l");
        let text = String::from_utf8_lossy(&output.stdout);
        let source_prefix = format!("{source_system_name}:");
        let target_prefix = format!("{target_system_name}:");
        let mut current_target: Option<String> = None;
        for line in text.lines() {
            if let Some(port) = line.strip_prefix("  |<- ") {
                if let Some(target) = &current_target {
                    if target.starts_with(&target_prefix) && port.trim().starts_with(&source_prefix) {
                        return true;
                    }
                }
                continue;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() && trimmed.contains(':') && !line.starts_with("  |") {
                current_target = Some(trimmed.to_string());
            }
        }
        false
    }

    /// A long-lived `pw-cat --playback` stream, looped in a `sh` wrapper so
    /// it outlives the short test clip's own runtime for the duration of
    /// the test — killing the shell (`Drop`) reaps the loop and whatever
    /// `pw-cat` child is currently running via its own process group.
    struct LoopedPlaybackStream {
        child: std::process::Child,
    }

    impl LoopedPlaybackStream {
        fn spawn(target_system_name: &str) -> Self {
            let clip = "/usr/share/sounds/speech-dispatcher/test.wav";
            assert!(std::path::Path::new(clip).is_file(), "expected a system test wav to exist at {clip}");
            let child = crate::sysproc::command("sh")
                .args(["-c", &format!("while true; do pw-cat --playback --target '{target_system_name}' '{clip}'; done")])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("failed to spawn looped pw-cat");
            Self { child }
        }
    }

    impl Drop for LoopedPlaybackStream {
        fn drop(&mut self) {
            // `pkill`, not `self.child.kill()` — the `sh -c` wrapper's own
            // pid is what we hold, but the actual `pw-cat` doing the playing
            // is a grandchild `sh` spawns fresh on every loop iteration;
            // killing just the wrapper leaves a `pw-cat` orphaned mid-clip.
            let _ = crate::sysproc::command("pkill").args(["-P", &self.child.id().to_string()]).status();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    #[ignore]
    fn routes_a_playback_stream_between_targets_exclusively() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let target_a = backend.create_virtual_output("Pipe Deck Stream Route Target A", false).expect("create target A");
        let target_b = backend.create_virtual_output("Pipe Deck Stream Route Target B", false).expect("create target B");

        wait_until_indexed(&[&target_a.system_name, &target_b.system_name]);
        let _stream = LoopedPlaybackStream::spawn(&target_a.system_name);

        // Wait for the looped pw-cat to actually register as a live node
        // with real ports before routing it anywhere.
        let indexed = (0..50).any(|_| {
            if connection().is_some_and(|conn| conn.node_id("pw-cat").is_some()) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(indexed, "expected the looped pw-cat stream to register as a live node");

        let route_result = route_playback_stream("pw-cat", &target_b.system_name);
        assert!(route_result.is_some(), "expected the native path to run, not fall back to the CLI");
        route_result.unwrap().expect("native stream route should succeed");

        let routed_to_b = (0..20).any(|_| {
            if pw_link_l_shows_a_link_between("pw-cat", &target_b.system_name) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(routed_to_b, "expected pw-cat to be linked into target B after routing");
        assert!(
            !pw_link_l_shows_a_link_between("pw-cat", &target_a.system_name),
            "expected the exclusive route to have disconnected pw-cat from target A"
        );

        let _ = backend.remove_virtual_device(&target_a.system_name);
        let _ = backend.remove_virtual_device(&target_b.system_name);
    }

    /// A long-lived `pw-cat --record` stream — the capture-direction
    /// counterpart to `LoopedPlaybackStream`, discarding whatever it
    /// records rather than needing an input clip.
    struct LoopedRecordStream {
        child: std::process::Child,
    }

    impl LoopedRecordStream {
        fn spawn() -> Self {
            let child = crate::sysproc::command("sh")
                .args(["-c", "while true; do pw-cat --record --format=s16 --rate=48000 --channels=2 /dev/null; done"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("failed to spawn looped pw-cat --record");
            Self { child }
        }
    }

    impl Drop for LoopedRecordStream {
        fn drop(&mut self) {
            let _ = crate::sysproc::command("pkill").args(["-P", &self.child.id().to_string()]).status();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    #[ignore]
    fn routes_a_capture_stream_between_sources_exclusively() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let source_a = backend.create_virtual_output("Pipe Deck Capture Route Source A", false).expect("create source A");
        let source_b = backend.create_virtual_output("Pipe Deck Capture Route Source B", false).expect("create source B");

        wait_until_indexed(&[&source_a.system_name, &source_b.system_name]);
        let _stream = LoopedRecordStream::spawn();

        let indexed = (0..50).any(|_| {
            if connection().is_some_and(|conn| conn.node_id("pw-cat").is_some()) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(indexed, "expected the looped pw-cat --record stream to register as a live node");

        let route_a = route_capture_stream(&source_a.system_name, "pw-cat");
        assert!(route_a.is_some(), "expected the native path to run, not fall back to the CLI");
        route_a.unwrap().expect("native capture route to source A should succeed");

        let routed_to_a = (0..20).any(|_| {
            if pw_link_l_shows_a_link_between(&source_a.system_name, "pw-cat") {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(routed_to_a, "expected pw-cat to be fed by source A after routing");

        let route_b = route_capture_stream(&source_b.system_name, "pw-cat");
        assert!(route_b.is_some());
        route_b.unwrap().expect("native capture route to source B should succeed");

        let routed_to_b = (0..20).any(|_| {
            if pw_link_l_shows_a_link_between(&source_b.system_name, "pw-cat") {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(routed_to_b, "expected pw-cat to be fed by source B after re-routing");
        assert!(
            !pw_link_l_shows_a_link_between(&source_a.system_name, "pw-cat"),
            "expected the exclusive route to have disconnected pw-cat from source A"
        );

        let _ = backend.remove_virtual_device(&source_a.system_name);
        let _ = backend.remove_virtual_device(&source_b.system_name);
    }
}
