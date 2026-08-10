pub mod linux;
pub mod mock;
pub mod scenario;
pub mod stub;

use crate::core::models::{
    Device, DeviceDirection, EffectChainConfig, LatencyPathNode, LatencyPingResult, MixSourceSpec,
    PortDirection, ProcessingNode, RuntimeGraph, VirtualDeviceInfo, VirtualDeviceResult,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("{0}")]
    Message(String),
}

pub type GraphListener = Box<dyn Fn(RuntimeGraph) + Send + Sync>;

/// Shared by every backend's virtual-device system_name derivation — moved
/// here (from `backend::linux::virtual_devices`, still re-exported there)
/// so `MockAudioBackend` doesn't need to depend on `backend::linux`.
pub fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "device".into()
    } else {
        slug
    }
}

pub trait AudioBackend: Send + Sync {
    // Graph fetch/subscribe.
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError>;
    fn subscribe(&self, listener: GraphListener) -> Result<(), BackendError>;

    // Volume / mute. `graph` is passed alongside the domain id because
    // resolving an id to whatever the backend addresses volume/mute by
    // (a pactl sink-input index, a Core Audio device UID, ...) needs the
    // already-fetched graph, not a second live lookup.
    fn set_device_volume(
        &self,
        graph: &RuntimeGraph,
        device_id: &str,
        percent: u8,
    ) -> Result<(), BackendError>;
    fn set_device_mute(
        &self,
        graph: &RuntimeGraph,
        device_id: &str,
        muted: bool,
    ) -> Result<(), BackendError>;
    fn set_stream_volume(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        percent: u8,
    ) -> Result<(), BackendError>;
    fn set_stream_mute(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        muted: bool,
    ) -> Result<(), BackendError>;

    /// The `system_name` of the current default output device (sink), if
    /// one can be determined (#11's tray quick controls). Read-only
    /// complement to `set_default_output_device`. Defaults to `None` so a
    /// backend with no concept of "default output" (e.g. `StubBackend`)
    /// doesn't need an explicit override.
    fn default_output_device_name(&self) -> Option<String> {
        None
    }

    /// Makes the sink named `system_name` the PipeWire/PulseAudio default
    /// output (#11's tray "switch default output" action), mirroring
    /// `pactl set-default-sink`. Errs by default — only a backend that can
    /// actually change the system default (today, only
    /// `LinuxPipeWireBackend`, via `pactl`) needs to override this.
    fn set_default_output_device(&self, _system_name: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "setting the default output device is not supported by this backend".into(),
        ))
    }

    // Routing: set or clear a single stream/device route.
    fn clear_stream_target(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError>;
    fn route_stream(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        target_device_id: &str,
    ) -> Result<(), BackendError>;

    // Graph/routing reconciliation. These stay call-granularity-agnostic on
    // purpose (see PD-019 and issue #68): the Linux impl internally discovers
    // and reconciles live pw-link/pactl state in one batched pass rather than
    // one link at a time, and a future backend is free to do the same in
    // whatever shape its platform's routing APIs need — the trait boundary is
    // "engine code doesn't name `backend::linux` directly", not "every route
    // change is one trait call."
    fn sync_live_routing_graph(&self, graph: &mut RuntimeGraph);
    fn apply_user_cleared_routes(
        &self,
        graph: &mut RuntimeGraph,
        cleared_streams: &HashSet<StreamIdentityKey>,
        cleared_devices: &HashSet<String>,
    );
    fn apply_graph_routing(&self, graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>);

    // Virtual device mix sources / aliases / levels.
    fn apply_virtual_mic_mix(
        &self,
        virtual_input: &Device,
        mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError>;
    fn set_mix_source_volume(
        &self,
        virtual_input_system_name: &str,
        source_system_name: &str,
        percent: u8,
    ) -> Result<(), BackendError>;
    fn set_mix_source_mute(
        &self,
        virtual_input_system_name: &str,
        source_system_name: &str,
        muted: bool,
    ) -> Result<(), BackendError>;
    /// Tears down every mix-source feed into `virtual_input_system_name` —
    /// used ahead of deleting the virtual input device outright (see
    /// `virtual_ops::remove_virtual_device`), where there's nothing left to
    /// preserve a mix relationship with.
    fn disconnect_all_virtual_mic_mixes(
        &self,
        virtual_input_system_name: &str,
    ) -> Result<(), BackendError>;
    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]);

    // Virtual device lifecycle. `create_virtual_output`/`create_virtual_input`
    // are for user-initiated new devices, where system_name is derived from
    // the label. `restore_virtual_device` is for config-driven recreation
    // (core/restore.rs) where system_name is already fixed (the persisted
    // slug) and must NOT be re-derived from a possibly-since-renamed label.
    fn create_virtual_output(
        &self,
        label: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, BackendError>;
    fn create_virtual_input(&self, label: &str) -> Result<VirtualDeviceResult, BackendError>;
    fn restore_virtual_device(
        &self,
        system_name: &str,
        label: &str,
        direction: DeviceDirection,
        multi: bool,
        mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError>;
    fn remove_virtual_device(&self, system_name: &str) -> Result<(), BackendError>;
    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo>;
    fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError>;

    // --- Soundboard (#127): one-shot clip playback ---

    /// Plays `path`'s audio into `target_system_name` (a virtual input, a
    /// hardware input's underlying device, or — for a Soundboard clip's
    /// monitor leg, #398 — a plain output sink) at `volume_percent` (0-100).
    /// Fire-and-forget: this returns once playback has *started*, not once
    /// the clip finishes. The implementation is responsible for tracking
    /// whatever process/handle it spawned so a later `stop_sound` call can
    /// interrupt it (issue #399) — callers get no handle back here.
    fn play_sound(
        &self,
        path: &std::path::Path,
        target_system_name: &str,
        volume_percent: u8,
    ) -> Result<(), BackendError>;

    /// Interrupts whatever Soundboard clip is currently playing (#399) —
    /// both the target and monitor legs (#398), if both were started. A
    /// no-op default (`Ok(())`) is provided since a backend with no
    /// tracked playback simply has nothing to stop; only `play_sound`
    /// implementations that actually track a handle need to override this.
    fn stop_sound(&self) -> Result<(), BackendError> {
        Ok(())
    }

    /// Resolves a live PipeWire node's id from its `node.name` (#411) — used
    /// by the effects "Live Params" fast path
    /// (`core/engine/effects_ops.rs::set_effect_chain_live_params`) to find
    /// an already-running filter-chain node to push a `pw-cli set-param`
    /// update to. The default implementation falls back to the original
    /// `pw-dump`-shellout workaround (`pipewire::pw_cli::find_node_id_by_name`)
    /// — correct everywhere, just not instant. `LinuxPipeWireBackend`
    /// overrides this to consult its own live registry index first (#410's
    /// `pw_registry::NativeGraphWatcher`) when available, falling back to
    /// the same shellout on a miss (e.g. a node created within the last
    /// debounce window that hasn't reached the index yet).
    fn find_live_node_id(&self, node_name: &str) -> Result<Option<u32>, BackendError> {
        crate::pipewire::pw_cli::find_node_id_by_name(node_name)
    }

    // Live routing-state queries used only as rule-matching fallbacks when
    // `RuntimeGraph`'s own `current_targets`/`current_target` are stale or
    // missing (see core/rules/matching.rs, core/rules/evaluation.rs). A
    // graph-derived answer is always tried first by the caller; these exist
    // for the rare case a live re-check is genuinely needed.
    fn monitor_routes_for_source(&self, _source_system_name: &str) -> Vec<String> {
        Vec::new()
    }

    fn is_routed_to(
        &self,
        _source_system_name: &str,
        _target_system_name: &str,
        _target_is_input: bool,
    ) -> bool {
        false
    }

    /// Whether a pipe-deck-owned sink/source currently exists under
    /// `system_name` — used by `core/restore.rs` to tell an already-live
    /// device apart from one that needs recreating. Same fallback
    /// conventions as the two queries above.
    fn device_is_live(&self, _system_name: &str, _direction: DeviceDirection) -> bool {
        false
    }

    // Backing audio-stack version, for display only (Settings/about footer).
    // `None` means "unknown/unavailable" rather than an error — every backend
    // gets this for free unless it overrides it.
    fn platform_audio_version(&self) -> Option<String> {
        None
    }

    /// Theoretical/buffering latency along `path`, from `pw-top -b`'s
    /// per-node QUANT/RATE (issue #223) — not a real measured round-trip,
    /// see `docs/architecture/Decisions.md` context in the issue for why
    /// that's out of scope for now. Errs by default; only the Linux backend
    /// can shell out to `pw-top`.
    fn measure_latency_ping(
        &self,
        _path: &[LatencyPathNode],
    ) -> Result<LatencyPingResult, BackendError> {
        Err(BackendError::Message(
            "latency measurement is not supported by this backend".to_string(),
        ))
    }

    // --- Live effects (issue #148/#149: native, restart-free transport) ---

    /// What the installed system can actually back for live effects — used
    /// to grey out UI controls nothing can realize. Default body delegates
    /// to `pipewire::fx_capability::probe_capabilities()`'s static
    /// filesystem probe (the only implementation that exists today),
    /// mirroring `find_live_node_id`'s own "default body reaches into
    /// `pipewire::` directly, only a backend that needs different behavior
    /// overrides it" convention — see issue #74: this exists so
    /// `core::engine::effects_ops` calls `self.adapter.effect_capabilities()`
    /// instead of importing `pipewire::fx_capability` itself.
    fn effect_capabilities(&self) -> crate::pipewire::fx_capability::FxCapabilities {
        crate::pipewire::fx_capability::probe_capabilities()
    }

    /// Validates `config` against this backend's own `effect_capabilities()`
    /// without touching anything live — safe to call on every UI slider
    /// change. Default body delegates to the pure
    /// `pipewire::fx_validate::preflight` function, so every backend gets
    /// identical validation logic unless a platform genuinely needs
    /// different rules.
    fn preflight_effect_chain(
        &self,
        config: &EffectChainConfig,
    ) -> crate::pipewire::fx_validate::PreflightResult {
        crate::pipewire::fx_validate::preflight(config, &self.effect_capabilities())
    }

    /// Pushes `config`'s live params (EQ gain, output trim, ...) to an
    /// already-resolved live node id — the second half of the Live Params
    /// fast path, split from `find_live_node_id` so callers can distinguish
    /// "not loaded yet" (their own message, based on `find_live_node_id`
    /// returning `None`) from a real push failure here. `device_system_name`
    /// is carried alongside `node_id` (not resolvable from it) so an
    /// overriding backend can route a portable-DSP-hosted device
    /// (`pipewire::native_dsp_host`) differently from a builtin-module-hosted
    /// one — see `LinuxPipeWireBackend`'s override. Default body delegates
    /// to `pipewire::pw_cli::set_params`, same convention as
    /// `find_live_node_id`/`effect_capabilities` above.
    fn push_effect_chain_live_params(
        &self,
        _device_system_name: &str,
        node_id: u32,
        config: &EffectChainConfig,
    ) -> Result<(), BackendError> {
        crate::pipewire::pw_cli::set_params(
            node_id,
            &crate::pipewire::fx_validate::live_params(config),
        )
    }

    /// Reverts a device from an effects-hosted node back to its plain
    /// pactl null-sink/virtual-source. `wait_for_node` controls whether to
    /// wait for the recreated node to register before returning — apply's
    /// rollback path historically doesn't wait, remove's primary path does;
    /// preserved as a parameter rather than silently unifying the two.
    fn revert_to_plain_device(
        &self,
        device: &Device,
        wait_for_node: bool,
    ) -> Result<(), BackendError>;

    /// Briefly parks any streams currently playing into `device_system_name`
    /// on a scratch holding sink, for the duration of a module swap. Returns
    /// the held streams' own `system_name`s (#428 — an opaque-to-the-caller
    /// handle either way; a native re-link identifies a stream by its own
    /// node name rather than a `pactl` sink-input index) for a later
    /// `release_held_sink_inputs` call. A no-op (empty result) if nothing
    /// is currently playing into the device.
    fn hold_sink_inputs_for_swap(
        &self,
        device_system_name: &str,
    ) -> Result<Vec<String>, BackendError>;

    /// Moves previously held streams back onto `target_system_name` and
    /// tears down the scratch holding sink if nothing else is using it.
    fn release_held_sink_inputs(
        &self,
        held_streams: &[String],
        target_system_name: &str,
    ) -> Result<(), BackendError>;

    /// Whatever's currently monitor-linked into `target_system_name`'s
    /// input ports — must be captured before a module swap severs them.
    /// `target_is_virtual_source` selects the port-prefix convention: `true`
    /// for a plain virtual input's own `input_*` ports, `false` for a
    /// filter-chain capture inlet's `playback_*` ports.
    fn list_mic_feeds(
        &self,
        target_system_name: &str,
        target_is_virtual_source: bool,
    ) -> Vec<String>;

    /// Re-points a previously captured feeder list so each one now feeds
    /// `to_system_name` instead of `from_system_name`. `to_is_virtual_source`
    /// follows the same port-prefix convention as `list_mic_feeds`.
    fn relink_mic_feeds(
        &self,
        feeders: &[String],
        from_system_name: &str,
        to_system_name: &str,
        to_is_virtual_source: bool,
    ) -> Result<(), BackendError>;

    /// Load a filter-chain-equivalent effect chain onto `device`, replacing
    /// whatever chain (if any) is already live for it, and re-link onward:
    /// `downstream_targets` for an output-direction device, `mic_feeders`
    /// for an input-direction one. Returns the node name actually
    /// relinked — `effect_output.*` for outputs, `effect_input.*` for
    /// inputs — mostly useful to callers for logging; the relinking itself
    /// is already done by the time this returns. Default body returns "not
    /// implemented" so `MockAudioBackend`/`StubBackend` don't need a real
    /// implementation (they no-op or error respectively, see their own
    /// overrides).
    fn load_effect_chain(
        &self,
        _device: &Device,
        _config: &EffectChainConfig,
        _downstream_targets: &[Device],
        _mic_feeders: &[String],
    ) -> Result<String, BackendError> {
        Err(BackendError::Message(
            "load_effect_chain: not implemented".into(),
        ))
    }

    /// Unloads a previously loaded chain's native module. Does *not* recreate
    /// the device's plain pactl sink/source — that's `revert_to_plain_device`'s
    /// job, called separately by the caller.
    fn unload_effect_chain(&self, _device_system_name: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "unload_effect_chain: not implemented".into(),
        ))
    }

    /// Whether a chain is currently loaded for `device_system_name` —
    /// backed by a real, out-of-process query (`daemon::ipc` for
    /// `LinuxPipeWireBackend`) rather than any in-memory bookkeeping this
    /// process itself might hold, since the process asking (the GUI) is
    /// never the process actually hosting the chain (the daemon). `false`
    /// by default, correct for any backend that can't host live effects at
    /// all.
    fn is_effect_chain_loaded(&self, _device_system_name: &str) -> bool {
        false
    }

    /// Push updated stage parameters (EQ gain, bypass, ...) to an
    /// already-loaded chain without reloading it — the in-process
    /// equivalent of today's `pw_cli::set_params` live-slider path.
    fn set_effect_chain_live_params(
        &self,
        _device_system_name: &str,
        _config: &EffectChainConfig,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_effect_chain_live_params: not implemented".into(),
        ))
    }

    // --- Processing nodes (PD-032: Mixer/Fan-out/EQ/stub graph nodes) ---
    //
    // Deliberately shaped like the effects methods just above: `load`/
    // `unload`/`is_loaded`/relink, all defaulting to "not implemented"/`false`
    // so `MockAudioBackend`/`StubBackend` don't need real bodies unless they
    // opt in. `Stub`-kind nodes never call any of these — they're a pure
    // pass-through with nothing to load (see `ProcessingNodeKind::Stub`).

    /// Loads (or replaces, under the same `system_name`) the PipeWire object
    /// backing `node`. Whatever transport backs `load_effect_chain` today
    /// backs this too — no new transport is introduced for processing nodes
    /// (PD-032).
    fn load_processing_node(&self, _node: &ProcessingNode) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "load_processing_node: not implemented".into(),
        ))
    }

    /// Unloads a previously loaded processing node's native module. Does not
    /// relink anything on its own — callers capture and restore links
    /// separately (mirrors `unload_effect_chain`/`revert_to_plain_device`'s
    /// split responsibility).
    fn unload_processing_node(&self, _system_name: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "unload_processing_node: not implemented".into(),
        ))
    }

    /// Whether a processing node is currently loaded — same out-of-process,
    /// correct-by-construction reasoning as `is_effect_chain_loaded`.
    fn is_processing_node_loaded(&self, _system_name: &str) -> bool {
        false
    }

    /// Re-points a processing node's `port_index` (on the `direction` side)
    /// to `peer_id` (a device or stream id, resolved against `graph` the
    /// same way `route_stream`/`route_device` resolve their targets — a
    /// stream needs `pactl` sink-input move, a device needs a `pw-link`
    /// monitor link, and only the backend can tell those apart), or
    /// disconnects the port if `None`.
    fn relink_processing_node_port(
        &self,
        _graph: &RuntimeGraph,
        _system_name: &str,
        _port_index: u32,
        _direction: PortDirection,
        _peer_id: Option<&str>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "relink_processing_node_port: not implemented".into(),
        ))
    }

    /// Live-updates one already-connected Mixer input's gain/mute without
    /// touching linking — the fast path for a slider drag, same PD-017
    /// two-speed contract as `set_effect_chain_live_params`. Only meaningful
    /// for `ProcessingNodeKind::Mixer` inputs; other kinds have no per-port
    /// gain concept and never call this.
    fn set_processing_node_input_gain(
        &self,
        _system_name: &str,
        _peer_system_name: &str,
        _gain_percent: u8,
        _muted: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_input_gain: not implemented".into(),
        ))
    }

    /// Live-updates a Fan-Out/Group node's own output volume/mute — a plain
    /// device-style volume on the node's backing sink, not a shaping gain
    /// (neither kind has DSP). Only meaningful for `ProcessingNodeKind::FanOut`/`Group`.
    fn set_processing_node_volume(
        &self,
        _system_name: &str,
        _volume_percent: u8,
        _muted: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_volume: not implemented".into(),
        ))
    }

    /// Live-updates an EQ5Band node's band gains without reloading the
    /// chain — the PD-017 two-speed fast path, same mechanism
    /// `CoreEngine::set_effect_chain_live_params` already uses for a
    /// device's attached EQ, just addressed by `system_name` directly
    /// rather than through a `Device`. `bypassed` reuses that same
    /// mechanism's neutral-live-params-regardless-of-configured-values
    /// behavior (`fx_validate::live_params`) — connections stay exactly as
    /// wired, only the signal itself passes through unprocessed.
    #[allow(clippy::too_many_arguments)]
    fn set_processing_node_eq_params(
        &self,
        _system_name: &str,
        _eq_sub: i32,
        _eq_bass: i32,
        _eq_mid: i32,
        _eq_treble: i32,
        _eq_air: i32,
        _output_gain: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_eq_params: not implemented".into(),
        ))
    }

    /// Live-updates a Delay node's Delay/Feedback/Feedforward controls
    /// without reloading the chain — same PD-017 fast path and bypass
    /// mechanism as `set_processing_node_eq_params` (issue #313).
    fn set_processing_node_delay_params(
        &self,
        _system_name: &str,
        _delay_ms: i32,
        _feedback_percent: i32,
        _feedforward_percent: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_delay_params: not implemented".into(),
        ))
    }

    /// Live-updates a Limiter node's ceiling without reloading the chain —
    /// same PD-017 fast path and bypass mechanism as
    /// `set_processing_node_delay_params` (issue #311).
    fn set_processing_node_limiter_params(
        &self,
        _system_name: &str,
        _ceiling_db: i32,
        _floor_db: i32,
        _symmetric: bool,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_limiter_params: not implemented".into(),
        ))
    }

    /// Live-updates an HPF node's Freq/Resonance without reloading the
    /// chain — same PD-017 fast path and bypass mechanism as
    /// `set_processing_node_limiter_params` (issue #312).
    fn set_processing_node_hpf_params(
        &self,
        _system_name: &str,
        _freq_hz: i32,
        _resonance_x10: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_hpf_params: not implemented".into(),
        ))
    }

    /// Live-updates a Reverb node's Mix without reloading the chain — same
    /// PD-017 fast path and bypass mechanism as
    /// `set_processing_node_limiter_params` (issue #327).
    fn set_processing_node_reverb_params(
        &self,
        _system_name: &str,
        _mix_percent: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_reverb_params: not implemented".into(),
        ))
    }

    /// Live-updates a Widener node's Width without reloading the chain —
    /// same PD-017 fast path and bypass mechanism as
    /// `set_processing_node_limiter_params` (issue #314).
    fn set_processing_node_widener_params(
        &self,
        _system_name: &str,
        _width_percent: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_widener_params: not implemented".into(),
        ))
    }

    /// Live-updates a Pan node's Balance without reloading the chain — same
    /// PD-017 fast path and bypass mechanism as
    /// `set_processing_node_limiter_params` (issue #16).
    fn set_processing_node_pan_params(
        &self,
        _system_name: &str,
        _balance_percent: i32,
        _bypassed: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(
            "set_processing_node_pan_params: not implemented".into(),
        ))
    }
}

/// Backend selection is compile-time/explicit-factory only (PD-019) — never
/// a runtime plugin.
pub fn create_backend() -> Box<dyn AudioBackend> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Box::new(mock::MockAudioBackend::from_env());
    }

    #[cfg(target_os = "linux")]
    {
        match linux::LinuxPipeWireBackend::new() {
            Ok(backend) => Box::new(backend),
            Err(error) => {
                eprintln!("PipeWire enumeration unavailable: {error}");
                Box::new(EmptyAudioBackend {
                    notice: format!("PipeWire unavailable: {error}"),
                })
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Real macOS/Windows backends land as #69/#70; `StubBackend` only
        // proves the trait boundary holds on a second platform target.
        Box::new(stub::StubBackend::new())
    }
}

struct EmptyAudioBackend {
    notice: String,
}

impl AudioBackend for EmptyAudioBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        Ok(RuntimeGraph {
            notice: Some(self.notice.clone()),
            ..RuntimeGraph::default()
        })
    }

    fn subscribe(&self, _listener: GraphListener) -> Result<(), BackendError> {
        Ok(())
    }

    fn set_device_volume(
        &self,
        _graph: &RuntimeGraph,
        _device_id: &str,
        _percent: u8,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_device_mute(
        &self,
        _graph: &RuntimeGraph,
        _device_id: &str,
        _muted: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_stream_volume(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _percent: u8,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_stream_mute(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _muted: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn clear_stream_target(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn route_stream(
        &self,
        _graph: &RuntimeGraph,
        _stream_id: &str,
        _target_device_id: &str,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn sync_live_routing_graph(&self, _graph: &mut RuntimeGraph) {}

    fn apply_user_cleared_routes(
        &self,
        _graph: &mut RuntimeGraph,
        _cleared_streams: &HashSet<StreamIdentityKey>,
        _cleared_devices: &HashSet<String>,
    ) {
    }

    fn apply_graph_routing(&self, _graph: &mut RuntimeGraph, _ctx: &ApplyRulesContext<'_>) {}

    fn apply_virtual_mic_mix(
        &self,
        _virtual_input: &Device,
        _mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_mix_source_volume(
        &self,
        _virtual_input_system_name: &str,
        _source_system_name: &str,
        _percent: u8,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn set_mix_source_mute(
        &self,
        _virtual_input_system_name: &str,
        _source_system_name: &str,
        _muted: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn disconnect_all_virtual_mic_mixes(
        &self,
        _virtual_input_system_name: &str,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn apply_device_aliases_and_levels(&self, _devices: &mut [Device]) {}

    fn monitor_routes_for_source(&self, _source_system_name: &str) -> Vec<String> {
        Vec::new()
    }

    fn is_routed_to(
        &self,
        _source_system_name: &str,
        _target_system_name: &str,
        _target_is_input: bool,
    ) -> bool {
        false
    }

    fn create_virtual_output(
        &self,
        _label: &str,
        _multi: bool,
    ) -> Result<VirtualDeviceResult, BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn create_virtual_input(&self, _label: &str) -> Result<VirtualDeviceResult, BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn restore_virtual_device(
        &self,
        _system_name: &str,
        _label: &str,
        _direction: DeviceDirection,
        _multi: bool,
        _mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn remove_virtual_device(&self, _system_name: &str) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn list_virtual_devices(&self) -> Vec<VirtualDeviceInfo> {
        Vec::new()
    }

    fn set_virtual_device_alias(
        &self,
        _system_name: &str,
        _alias: &str,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn play_sound(
        &self,
        _path: &std::path::Path,
        _target_system_name: &str,
        _volume_percent: u8,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn revert_to_plain_device(
        &self,
        _device: &Device,
        _wait_for_node: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn hold_sink_inputs_for_swap(
        &self,
        _device_system_name: &str,
    ) -> Result<Vec<String>, BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn release_held_sink_inputs(
        &self,
        _held_streams: &[String],
        _target_system_name: &str,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }

    fn list_mic_feeds(
        &self,
        _target_system_name: &str,
        _target_is_virtual_source: bool,
    ) -> Vec<String> {
        Vec::new()
    }

    fn relink_mic_feeds(
        &self,
        _feeders: &[String],
        _from_system_name: &str,
        _to_system_name: &str,
        _to_is_virtual_source: bool,
    ) -> Result<(), BackendError> {
        Err(BackendError::Message(self.notice.clone()))
    }
}

#[cfg(test)]
mod slugify_tests {
    use super::slugify;

    #[test]
    fn slugifies_names_with_punctuation_and_case() {
        assert_eq!(slugify("Game Mix"), "game-mix");
        assert_eq!(slugify("My Mic!!!"), "my-mic");
    }

    #[test]
    fn empty_or_all_punctuation_falls_back_to_device() {
        assert_eq!(slugify(""), "device");
        assert_eq!(slugify("!!!"), "device");
    }
}
