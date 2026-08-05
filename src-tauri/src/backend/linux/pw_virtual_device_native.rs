//! Native virtual sink/source creation via the `adapter`/
//! `support.null-audio-sink` factory (#422, the second slice of #413/#8's
//! native `pipewire-rs` migration, after volume/mute — PD-043). Replaces
//! `pactl load-module module-null-sink`/`unload-module` for the
//! `create_virtual_output`/`create_virtual_input`/`remove_virtual_device`
//! path with `core.create_object::<pw::node::Node>("adapter", ...)` /
//! `registry.destroy_global(node_id)`.
//!
//! ## "module-null-sink" is not a real PipeWire module
//!
//! Unlike `libpipewire-module-filter-chain` (a genuine native module
//! `pipewire/native_host.rs` loads via `pw_context_load_module`),
//! `module-null-sink` is Pulse-compat terminology `pipewire-pulse` itself
//! implements — there is no PipeWire module by that name to load natively.
//! Confirmed live (`pw-dump`) that what `pactl load-module module-null-sink
//! media.class=Audio/Source/Virtual ...` actually creates is a single
//! `adapter`-factory node with `factory.name = support.null-audio-sink` —
//! exactly the construct `/usr/share/pipewire/pipewire.conf`'s own
//! commented-out example documents for a virtual mic. The `capture_*`/
//! `input_*` vs. `playback_*`/`monitor_*` port-name split `pw_link.rs`
//! depends on isn't something this code sets explicitly — it falls out of
//! `support.null-audio-sink` itself, purely from whatever `media.class` is
//! passed (`Audio/Source/Virtual` vs. `Audio/Sink`), confirmed by comparing
//! a `pactl`-created virtual mic's `pw-dump` output against this module's
//! own `Audio/Sink` test node side by side.
//!
//! ## Object lifetime — `object.linger`, not a retained handle
//!
//! `native_host.rs`'s filter-chain modules are unloaded via a *retained*
//! `*mut pw_impl_module` handle (`NativeHost::loaded`), because a
//! `pw_context_load_module` handle has no other identity to unload by. A
//! created `Node` is different: like `pw_link_native.rs`'s links, it's
//! created with `object.linger => "1"` so it survives this connection's own
//! proxy going out of scope, and — critically — it also survives a full
//! process restart, since nothing about it depends on this process still
//! being alive. Removal therefore never needs a retained handle at all: it
//! re-resolves the node's *current* global id by name (via this module's own
//! registry index) and calls `registry.destroy_global(node_id)`, the same
//! mechanism `pw_link_native.rs` uses to tear down a link it didn't
//! necessarily create in this process lifetime either.
//!
//! ## What this slice does *not* cover
//!
//! Existence checks (`sink_exists`/`source_exists`/`pipe_deck_device_is_live`),
//! module discovery after a restart (`list_pipe_deck_modules`, which still
//! works for natively-created nodes since `pactl list sinks/sources short`
//! surfaces them regardless of how they were created — but `pactl list
//! modules short` does *not*, confirmed live, since there's no Pulse
//! "module" backing a plain `adapter` node), description-sync-via-recreate,
//! the holding sink, and feed-sink lifecycle are all deliberately left on
//! `pactl` — this slice only covers the create/remove path
//! `create_virtual_output`/`create_virtual_input`/`remove_virtual_device` go
//! through (`pactl::create_null_sink`/`create_virtual_source`/
//! `unload_module`). See PD-044.

use crate::backend::BackendError;
use pipewire as pw;
use pw::context::ContextRc;
use pw::core::CoreRc;
use pw::properties::properties;
use pw::registry::RegistryRc;
use pw::thread_loop::ThreadLoopRc;
use pw::types::ObjectType;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const INITIAL_INDEX_TIMEOUT: Duration = Duration::from_millis(500);

struct Connection {
    _thread_loop: ManuallyDrop<ThreadLoopRc>,
    _context: ManuallyDrop<ContextRc>,
    core: ManuallyDrop<CoreRc>,
    registry: ManuallyDrop<RegistryRc>,
    _listener: ManuallyDrop<pw::registry::Listener>,
    node_ids: Arc<Mutex<HashMap<String, u32>>>,
}

// SAFETY: identical contract to `pw_link_native.rs::Connection`/
// `pw_mixer_native.rs::Connection` — every touch of the `pw`-owned fields
// happens either during setup (this thread, before `start()` returns) or
// from callbacks pw's own thread loop invokes on its internal thread, never
// concurrently. `core`/`registry` are read afterward (under
// `thread_loop.lock()`) to `create_object`/`destroy_global`. `node_ids` is a
// plain `Arc<Mutex<..>>`.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    fn start() -> Option<Self> {
        static PW_INIT: std::sync::Once = std::sync::Once::new();
        PW_INIT.call_once(pw::init);

        // SAFETY: `pw::init()` has just been called above (exactly once,
        // process-wide, via `PW_INIT`).
        let thread_loop = unsafe { ThreadLoopRc::new(Some("pipe-deck-virtual-device"), None) }.ok()?;
        thread_loop.start();

        let (tx, rx) = mpsc::channel::<(u32, String)>();

        let (context, core, registry, listener) = {
            let _lock = thread_loop.lock();
            let context = ContextRc::new(&thread_loop, None).ok()?;
            let core = context.connect_rc(None).ok()?;
            let registry = core.get_registry_rc().ok()?;

            let listener = registry
                .add_listener_local()
                .global(move |global| {
                    if global.type_ != ObjectType::Node {
                        return;
                    }
                    let Some(name) = global.props.and_then(|props| props.get("node.name")) else {
                        return;
                    };
                    let _ = tx.send((global.id, name.to_string()));
                })
                .register();

            (context, core, registry, listener)
        };

        let node_ids: Arc<Mutex<HashMap<String, u32>>> = Arc::new(Mutex::new(HashMap::new()));
        let assembler_node_ids = node_ids.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        thread::spawn(move || run_assembler(rx, assembler_node_ids, ready_tx));
        let _ = ready_rx.recv_timeout(INITIAL_INDEX_TIMEOUT);

        Some(Self {
            _thread_loop: ManuallyDrop::new(thread_loop),
            _context: ManuallyDrop::new(context),
            core: ManuallyDrop::new(core),
            registry: ManuallyDrop::new(registry),
            _listener: ManuallyDrop::new(listener),
            node_ids,
        })
    }

    fn node_id(&self, name: &str) -> Option<u32> {
        self.node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(name).copied()
    }

    fn create_null_audio_sink(&self, system_name: &str, description: &str, media_class: &str) -> Result<(), BackendError> {
        let _lock = self._thread_loop.lock();
        self.core
            .create_object::<pw::node::Node>(
                "adapter",
                &properties! {
                    "factory.name" => "support.null-audio-sink",
                    "node.name" => system_name,
                    "node.description" => description,
                    "node.nick" => description,
                    "media.class" => media_class,
                    "audio.position" => "FL,FR",
                    "object.linger" => "1",
                },
            )
            .map(|_node: pw::node::Node| ())
            .map_err(|error| BackendError::Message(format!("failed to create native virtual device {system_name}: {error}")))
    }

    fn destroy_node(&self, node_id: u32) -> Result<(), BackendError> {
        let _lock = self._thread_loop.lock();
        self.registry
            .destroy_global(node_id)
            .into_result()
            .map(|_| ())
            .map_err(|error| BackendError::Message(format!("failed to destroy native node {node_id}: {error}")))
    }
}

static CONNECTION: OnceLock<Option<Connection>> = OnceLock::new();

fn connection() -> Option<&'static Connection> {
    CONNECTION.get_or_init(Connection::start).as_ref()
}

fn run_assembler(rx: mpsc::Receiver<(u32, String)>, node_ids: Arc<Mutex<HashMap<String, u32>>>, ready_tx: mpsc::Sender<()>) {
    let mut ready_tx = Some(ready_tx);
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok((id, name)) => {
                node_ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(name, id);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if let Some(tx) = ready_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Creates a virtual output (sink) node named `system_name`. `None` if the
/// native connection never started — the caller falls back to `pactl
/// load-module module-null-sink` entirely.
pub fn create_output(system_name: &str, description: &str) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    Some(conn.create_null_audio_sink(system_name, description, "Audio/Sink"))
}

/// Creates a virtual input (mic/capture) node named `system_name` — same
/// underlying `support.null-audio-sink` factory as [`create_output`], just
/// `media.class = Audio/Source/Virtual`, which is what actually produces the
/// `capture_*`/`input_*` port-name split instead of `playback_*`/`monitor_*`.
pub fn create_input(system_name: &str, description: &str) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    Some(conn.create_null_audio_sink(system_name, description, "Audio/Source/Virtual"))
}

/// Removes a natively-created virtual device by re-resolving its current
/// global id by name and destroying it — see this module's own doc comment
/// for why no retained creation-time handle is needed. `None` if the native
/// connection never started, or the node isn't (or is no longer) indexed —
/// the caller falls back to `pactl`'s module-id-based unload, which for a
/// device that really was created natively will itself just harmlessly fail
/// (no such module in `pactl`'s own table) rather than silently do nothing.
pub fn remove(system_name: &str) -> Option<Result<(), BackendError>> {
    let conn = connection()?;
    let node_id = conn.node_id(system_name)?;
    Some(conn.destroy_node(node_id))
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Drives creation/removal
    //! through the real `AudioBackend` trait methods (not this module's
    //! functions directly) so the test exercises the exact same call path a
    //! real create/remove-device action in the app takes, and verifies both
    //! the resulting port topology (the `capture_*`/`input_*` vs.
    //! `playback_*`/`monitor_*` split this module's own doc comment is about)
    //! and Pulse-compat visibility independently via `pactl`, not this
    //! module's own index.
    use crate::backend::AudioBackend;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use crate::backend::linux::pactl::sink_exists;
    use std::thread;
    use std::time::Duration;

    fn output_ports(system_name: &str) -> Vec<String> {
        let output = crate::sysproc::command("pw-link").arg("-o").output().expect("failed to run pw-link -o");
        let prefix = format!("{system_name}:");
        String::from_utf8_lossy(&output.stdout).lines().filter(|line| line.starts_with(&prefix)).map(str::to_string).collect()
    }

    fn input_ports(system_name: &str) -> Vec<String> {
        let output = crate::sysproc::command("pw-link").arg("-i").output().expect("failed to run pw-link -i");
        let prefix = format!("{system_name}:");
        String::from_utf8_lossy(&output.stdout).lines().filter(|line| line.starts_with(&prefix)).map(str::to_string).collect()
    }

    #[test]
    #[ignore]
    fn creates_a_virtual_output_natively_with_playback_and_monitor_ports() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend.create_virtual_output("Pipe Deck Native Output Test", false).expect("create should succeed");

        assert!(
            (0..20).any(|_| {
                if sink_exists(&device.system_name).unwrap_or(false) {
                    return true;
                }
                thread::sleep(Duration::from_millis(100));
                false
            }),
            "expected pactl's own sink listing to see the natively-created device"
        );

        let outputs = output_ports(&device.system_name);
        assert!(outputs.iter().any(|p| p.contains(":monitor_")), "expected monitor_* output ports, got {outputs:?}");
        let inputs = input_ports(&device.system_name);
        assert!(inputs.iter().any(|p| p.contains(":playback_")), "expected playback_* input ports, got {inputs:?}");

        backend.remove_virtual_device(&device.system_name).expect("remove should succeed");

        let removed = (0..20).any(|_| {
            if !sink_exists(&device.system_name).unwrap_or(true) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(removed, "expected pactl's own sink listing to no longer see the device after removal");
    }

    #[test]
    #[ignore]
    fn creates_a_virtual_input_natively_with_capture_and_input_ports() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend.create_virtual_input("Pipe Deck Native Input Test").expect("create should succeed");

        let outputs = output_ports(&device.system_name);
        assert!(outputs.iter().any(|p| p.contains(":capture_")), "expected capture_* output ports, got {outputs:?}");
        let inputs = input_ports(&device.system_name);
        assert!(inputs.iter().any(|p| p.contains(":input_")), "expected input_* input ports, got {inputs:?}");

        backend.remove_virtual_device(&device.system_name).expect("remove should succeed");
    }
}
