use crate::backend::linux::graph_enrich;
use crate::backend::linux::graph_routing;
use crate::backend::linux::pactl;
use crate::backend::linux::pw_dump::{self, PwDumpObject};
use crate::backend::linux::pw_link;
use crate::backend::linux::pw_registry;
use crate::backend::linux::split_sink;
use crate::backend::linux::virtual_devices::{VirtualDeviceEntry, VirtualDeviceRegistry};
use crate::backend::linux::virtual_mic_mix;
use crate::backend::slugify;
use crate::backend::{AudioBackend, BackendError, GraphListener};
use crate::core::models::{
    Device, DeviceDirection, DeviceKind, LatencyHop, LatencyPathNode, LatencyPingResult,
    MixSourceSpec, PortDirection, ProcessingNode, ProcessingNodeKind, RuntimeGraph,
    VirtualDeviceInfo, VirtualDeviceResult,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::StreamIdentityKey;
use crate::pipewire::filter_chain;
use crate::sysproc;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MONITOR_DEBOUNCE: Duration = Duration::from_millis(200);
// Under sustained high-churn (many streams appearing/disappearing rapidly),
// events never go quiet long enough for the debounce window alone to fire —
// this caps how long a burst can coalesce before we force a refresh anyway,
// so routing changes still surface promptly (see docs/architecture/PipeWire_Design.md).
const MAX_COALESCE_WINDOW: Duration = Duration::from_millis(400);

pub struct LinuxPipeWireBackend {
    cached_graph: Arc<Mutex<RuntimeGraph>>,
    listener: Arc<Mutex<Option<GraphListener>>>,
    registry: Arc<VirtualDeviceRegistry>,
    // Soundboard (#399): the `pw-cat` child process(es) for whatever clip is
    // currently playing — up to two (a target leg and a monitor leg, #398),
    // never more since the UI only ever lets one clip play at a time. Kept
    // here rather than on `CoreEngine` because the backend is what actually
    // spawned them (see PD-036's rationale for why the trait boundary owns
    // this kind of one-off process, not the engine).
    soundboard_playback: Arc<Mutex<Vec<std::process::Child>>>,
    // Native `pw::registry`-based graph transport (#410, PD-040) — `None`
    // when it failed to start (e.g. PipeWire unreachable at construction
    // time), in which case `fetch_graph`/`subscribe` fall back to the
    // original `pw-dump`-shellout transport unchanged. Holding this alive
    // for as long as the backend itself lives is what keeps the connection
    // (and its background thread-loop/assembler threads) running.
    native_graph: Option<pw_registry::NativeGraphWatcher>,
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

        let native_graph =
            match pw_registry::NativeGraphWatcher::start(cached_graph.clone(), listener.clone()) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    eprintln!(
                    "native PipeWire graph watcher unavailable, falling back to pw-dump: {error}"
                );
                    None
                }
            };

        Ok(Self {
            cached_graph,
            listener,
            registry,
            soundboard_playback: Arc::new(Mutex::new(Vec::new())),
            native_graph,
        })
    }

    fn create_output_internal(
        &self,
        system_name: &str,
        label: &str,
        multi: bool,
    ) -> Result<VirtualDeviceEntry, BackendError> {
        self.registry.create_output_for(system_name, label, multi)
    }

    fn create_input_internal(
        &self,
        system_name: &str,
        label: &str,
    ) -> Result<VirtualDeviceEntry, BackendError> {
        self.registry.create_input_for(system_name, label)
    }
}

impl AudioBackend for LinuxPipeWireBackend {
    fn fetch_graph(&self) -> Result<RuntimeGraph, BackendError> {
        // The native watcher's assembler thread keeps `cached_graph` fresh
        // continuously (instantly on topology changes, at least once a
        // second otherwise — see `pw_registry::POLL_INTERVAL`'s doc
        // comment), so there's nothing to re-shell for; a plain read is
        // both correct and far cheaper than a fresh `pw-dump` round trip.
        if self.native_graph.is_some() {
            let cached = self
                .cached_graph
                .lock()
                .map_err(|_| BackendError::Message("graph lock poisoned".into()))?;
            return Ok(cached.clone());
        }

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
            .map_err(|_| BackendError::Message("listener lock poisoned".into()))? = Some(listener);

        // The native watcher's assembler thread already reads from this
        // same `listener` slot on every rebuild (started back in `new()`,
        // before any listener existed to call) — nothing left to spawn.
        // Only fall back to the pw-dump-shellout transport if the native
        // connection never came up.
        if self.native_graph.is_none() {
            let cached_graph = self.cached_graph.clone();
            let listener_slot = self.listener.clone();
            thread::spawn(move || {
                if !run_pw_dump_monitor(&cached_graph, &listener_slot) {
                    run_poll_loop(&cached_graph, &listener_slot);
                }
            });
        }

        Ok(())
    }

    fn set_device_volume(
        &self,
        graph: &RuntimeGraph,
        device_id: &str,
        percent: u8,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_device_volume(device_id, graph, percent)
    }

    fn set_device_mute(
        &self,
        graph: &RuntimeGraph,
        device_id: &str,
        muted: bool,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_device_mute(device_id, graph, muted)
    }

    fn set_stream_volume(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        percent: u8,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_stream_volume(graph, stream_id, percent)
    }

    fn set_stream_mute(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        muted: bool,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::set_stream_mute(graph, stream_id, muted)
    }

    fn default_output_device_name(&self) -> Option<String> {
        pactl::default_output_system_name()
    }

    fn set_default_output_device(&self, system_name: &str) -> Result<(), BackendError> {
        pactl::set_default_output_system_name(system_name)
    }

    fn clear_stream_target(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<(), BackendError> {
        crate::backend::linux::pactl::clear_stream_target(
            graph,
            stream_id,
            previous_target_device_id,
        )
    }

    fn route_stream(
        &self,
        graph: &RuntimeGraph,
        stream_id: &str,
        target_device_id: &str,
    ) -> Result<(), BackendError> {
        let intent = crate::core::models::RoutingIntent {
            stream_id: stream_id.to_string(),
            target_device_id: Some(target_device_id.to_string()),
            target_device_ids: Vec::new(),
            target_system_name: None,
        };
        crate::core::routing::apply_routing_intent(graph, &intent)
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

    fn apply_virtual_mic_mix(
        &self,
        virtual_input: &Device,
        mix_sources: &[MixSourceSpec],
    ) -> Result<(), BackendError> {
        virtual_mic_mix::apply_virtual_mic_mix(virtual_input, mix_sources)
    }

    fn set_mix_source_volume(
        &self,
        virtual_input_system_name: &str,
        source_system_name: &str,
        percent: u8,
    ) -> Result<(), BackendError> {
        virtual_mic_mix::set_mix_source_volume(
            virtual_input_system_name,
            source_system_name,
            percent,
        )
    }

    fn set_mix_source_mute(
        &self,
        virtual_input_system_name: &str,
        source_system_name: &str,
        muted: bool,
    ) -> Result<(), BackendError> {
        virtual_mic_mix::set_mix_source_mute(virtual_input_system_name, source_system_name, muted)
    }

    fn disconnect_all_virtual_mic_mixes(
        &self,
        virtual_input_system_name: &str,
    ) -> Result<(), BackendError> {
        virtual_mic_mix::disconnect_all_virtual_mic_mixes(virtual_input_system_name)
    }

    fn apply_device_aliases_and_levels(&self, devices: &mut [Device]) {
        graph_enrich::apply_device_aliases(devices);
        graph_enrich::apply_device_levels(devices);
    }

    fn monitor_routes_for_source(&self, source_system_name: &str) -> Vec<String> {
        crate::backend::linux::pw_link::list_all_monitor_routes_for_source(source_system_name)
    }

    fn is_routed_to(
        &self,
        source_system_name: &str,
        target_system_name: &str,
        target_is_input: bool,
    ) -> bool {
        crate::backend::linux::pw_link::is_sink_monitor_routed_to(
            source_system_name,
            target_system_name,
            target_is_input,
        )
    }

    fn device_is_live(&self, system_name: &str, direction: DeviceDirection) -> bool {
        pactl::pipe_deck_device_is_live(system_name, direction)
    }

    fn create_virtual_output(
        &self,
        label: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, BackendError> {
        let system_name = format!("pipe-deck-{}", slugify(label));
        Ok(self
            .create_output_internal(&system_name, label, multi)?
            .into_result())
    }

    fn create_virtual_input(&self, label: &str) -> Result<VirtualDeviceResult, BackendError> {
        let system_name = format!("pipe-deck-{}", slugify(label));
        Ok(self
            .create_input_internal(&system_name, label)?
            .into_result())
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
        self.registry
            .list_devices()
            .iter()
            .map(|entry| entry.to_info())
            .collect()
    }

    fn play_sound(
        &self,
        path: &std::path::Path,
        target_system_name: &str,
        volume_percent: u8,
    ) -> Result<(), BackendError> {
        let child = crate::backend::linux::play_sound::play_sound(
            path,
            target_system_name,
            volume_percent,
        )?;
        let mut playback = self
            .soundboard_playback
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Drop anything that's already finished on its own before adding the
        // new leg, so this doesn't grow unbounded across many clip triggers.
        playback.retain_mut(|existing| !matches!(existing.try_wait(), Ok(Some(_))));
        playback.push(child);
        Ok(())
    }

    fn stop_sound(&self) -> Result<(), BackendError> {
        let mut playback = self
            .soundboard_playback
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for mut child in playback.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    fn find_live_node_id(&self, node_name: &str) -> Result<Option<u32>, BackendError> {
        if let Some(watcher) = &self.native_graph {
            if let Some(id) = watcher.find_node_id(node_name) {
                return Ok(Some(id));
            }
        }
        crate::pipewire::pw_cli::find_node_id_by_name(node_name)
    }

    fn set_virtual_device_alias(&self, system_name: &str, alias: &str) -> Result<(), BackendError> {
        let _ = crate::backend::linux::pactl::sync_feed_sink_for_virtual_input(system_name, alias);
        let _ = self.registry.set_label(system_name, alias);
        if let Some(entry) = self.registry.get(system_name) {
            if let Ok(Some(new_module_id)) =
                crate::backend::linux::pactl::sync_virtual_device_description(
                    system_name,
                    entry.direction,
                    &entry.module_id,
                    alias,
                )
            {
                let _ = self.registry.set_module_id(system_name, &new_module_id);
            }
        }
        Ok(())
    }

    fn platform_audio_version(&self) -> Option<String> {
        query_pipewire_version()
    }

    fn measure_latency_ping(
        &self,
        path: &[LatencyPathNode],
    ) -> Result<LatencyPingResult, BackendError> {
        measure_latency_ping_live(path, self.native_graph.as_ref())
    }

    fn revert_to_plain_device(
        &self,
        device: &Device,
        wait_for_node: bool,
    ) -> Result<(), BackendError> {
        if device.direction == DeviceDirection::Input {
            pactl::create_virtual_source(&device.system_name, &device.label)?;
            if wait_for_node {
                filter_chain::wait_for_source(&device.system_name, Duration::from_secs(5))?;
            }
        } else {
            pactl::create_null_sink(&device.system_name, &device.label)?;
            if wait_for_node {
                filter_chain::wait_for_sink(&device.system_name, Duration::from_secs(5))?;
            }
        }
        Ok(())
    }

    fn hold_sink_inputs_for_swap(
        &self,
        device_system_name: &str,
    ) -> Result<Vec<String>, BackendError> {
        // Same query either native or CLI-backed path uses to answer "what's
        // currently feeding this sink's playback ports" — #412 already built
        // it for capture-source-into-sink discovery, and a playback stream
        // feeding an ordinary sink is the exact same shape of query. See
        // #428's own module doc comment for why a stream is just another
        // node like any other here.
        let held = pw_link::list_capture_sources_for_sink(device_system_name);
        if !held.is_empty() {
            pactl::ensure_holding_sink()?;
            for stream_system_name in &held {
                move_stream_with_retry(
                    stream_system_name,
                    pactl::HOLDING_SINK_NAME,
                    Duration::from_secs(5),
                );
            }
        }
        Ok(held)
    }

    /// Moves held streams back onto `target_system_name`, retrying each move
    /// for a few seconds rather than a single fire-and-forget attempt — a plain
    /// sink recreated moments ago by `revert_to_plain_device` (or an
    /// effects-hosted node reloaded by `swap_to_effect_chain`) can still be a
    /// beat away from actually being live even after that caller's own shorter
    /// wait already gave up, and a move attempted at exactly that instant would
    /// otherwise silently fail with nothing ever retrying it — permanently
    /// stranding audio on the "Pipe Deck (temporary hold)" sink.
    fn release_held_sink_inputs(
        &self,
        held_streams: &[String],
        target_system_name: &str,
    ) -> Result<(), BackendError> {
        for stream_system_name in held_streams {
            move_stream_with_retry(
                stream_system_name,
                target_system_name,
                Duration::from_secs(5),
            );
        }
        let _ = pactl::remove_holding_sink();
        Ok(())
    }

    fn list_mic_feeds(
        &self,
        target_system_name: &str,
        target_is_virtual_source: bool,
    ) -> Vec<String> {
        virtual_mic_mix::list_feeds(target_system_name, target_is_virtual_source)
    }

    fn relink_mic_feeds(
        &self,
        feeders: &[String],
        from_system_name: &str,
        to_system_name: &str,
        to_is_virtual_source: bool,
    ) -> Result<(), BackendError> {
        virtual_mic_mix::relink_feeds_to(
            feeders,
            from_system_name,
            to_system_name,
            to_is_virtual_source,
        )
    }

    // `native_host` is not called directly from this file (issue #148,
    // "daemon-owned" requirement) — only the daemon binary's
    // `daemon::ipc::server::dispatch` actually invokes it. This file talks
    // to that daemon process over `daemon::ipc::client::NativeHostClient`
    // instead.
    fn load_effect_chain(
        &self,
        device: &Device,
        config: &crate::core::models::EffectChainConfig,
        downstream_targets: &[Device],
        mic_feeders: &[String],
    ) -> Result<String, BackendError> {
        use crate::daemon::ipc::client::NativeHostClient;
        use crate::pipewire::native_dsp_host;

        let is_input = device.direction == DeviceDirection::Input;

        if let Some(module_id) = pactl::find_module_id_by_sink_name(&device.system_name)? {
            pactl::unload_module(&module_id)?;
        }

        // Portable-DSP host (issue #74) takes over whatever it can fully
        // express (today: an EQ5Band-only capture-direction chain); anything
        // else keeps going through the builtin-module path. No feature flag
        // — this is dispatch on what the config actually needs, not a
        // rollout gate.
        let playback_name = if is_input && native_dsp_host::supports(config) {
            NativeHostClient::load_dsp_chain(&device.system_name, is_input, config)
        } else {
            NativeHostClient::load_chain(&device.system_name, is_input, config)
        }
        .map_err(|error| BackendError::Message(error.to_string()))?;

        if is_input {
            virtual_mic_mix::relink_feeds_to(mic_feeders, &device.system_name, &playback_name, false).map_err(|error| {
                BackendError::Message(format!(
                    "native effects chain loaded but its mic-mix feeds could not be re-linked: {error}"
                ))
            })?;
            return Ok(playback_name);
        }

        let mut allowed_targets = HashSet::new();
        for target in downstream_targets {
            let is_virtual_input =
                target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
            let result = if is_virtual_input {
                pw_link::link_capture_source_to_virtual_input(&playback_name, &target.system_name)
            } else {
                pw_link::link_capture_source_to_sink(&playback_name, &target.system_name)
            };
            result.map_err(|error| {
                BackendError::Message(format!(
                    "native effects chain loaded but could not be re-linked to {}: {error}",
                    target.label
                ))
            })?;
            allowed_targets.insert(target.system_name.clone());
        }
        // A prior load's downstream targets may no longer match this one,
        // and node identity persisting across a Structural Apply (PD-020)
        // means nothing else ever tears a stale link down on its own.
        let _ = split_sink::prune_stale_fan_out_links(&playback_name, &allowed_targets);

        Ok(playback_name)
    }

    fn unload_effect_chain(&self, device_system_name: &str) -> Result<(), BackendError> {
        use crate::daemon::ipc::client::NativeHostClient;
        // Unconditionally unloads both transports — whichever one wasn't
        // actually hosting `device_system_name` is already a documented
        // no-op on both `native_host::unload_chain` and
        // `native_dsp_host::unload_chain`, so this doesn't need to know in
        // advance which one loaded it.
        NativeHostClient::unload_dsp_chain(device_system_name)
            .map_err(|error| BackendError::Message(error.to_string()))?;
        NativeHostClient::unload_chain(device_system_name)
            .map_err(|error| BackendError::Message(error.to_string()))
    }

    fn is_effect_chain_loaded(&self, device_system_name: &str) -> bool {
        use crate::daemon::ipc::client::NativeHostClient;
        NativeHostClient::is_dsp_chain_loaded(device_system_name)
            || NativeHostClient::is_loaded(device_system_name)
    }

    fn push_effect_chain_live_params(
        &self,
        device_system_name: &str,
        node_id: u32,
        config: &crate::core::models::EffectChainConfig,
    ) -> Result<(), BackendError> {
        use crate::daemon::ipc::client::NativeHostClient;
        use crate::pipewire::native_dsp_host;

        if native_dsp_host::supports(config)
            && NativeHostClient::is_dsp_chain_loaded(device_system_name)
        {
            return NativeHostClient::set_dsp_chain_live_params(device_system_name, config)
                .map_err(|error| BackendError::Message(error.to_string()));
        }
        crate::pipewire::pw_cli::set_params(
            node_id,
            &crate::pipewire::fx_validate::live_params(config),
        )
    }

    // --- Processing nodes (PD-032). Fan-out, Mixer, and EQ5Band (issue #293
    // phases 2-4). Stub never reaches here at all (CoreEngine never calls
    // this trait for a Stub kind — see `ProcessingNodeKind::Stub`).

    fn load_processing_node(&self, node: &ProcessingNode) -> Result<(), BackendError> {
        match &node.kind {
            ProcessingNodeKind::FanOut {
                volume_percent,
                muted,
            }
            | ProcessingNodeKind::Group {
                volume_percent,
                muted,
            } => {
                if pactl::sink_exists(&node.system_name)? {
                    return Ok(());
                }
                pactl::create_null_sink(&node.system_name, &node.label)?;
                let _ = pactl::set_sink_volume_by_name(&node.system_name, *volume_percent);
                let _ = pactl::set_sink_mute_by_name(&node.system_name, *muted);
                Ok(())
            }
            ProcessingNodeKind::Mixer { .. } => {
                if pactl::sink_exists(&node.system_name)? {
                    return Ok(());
                }
                pactl::create_null_sink(&node.system_name, &node.label)?;
                Ok(())
            }
            ProcessingNodeKind::Eq5Band {
                eq_sub,
                eq_bass,
                eq_mid,
                eq_treble,
                eq_air,
                output_gain,
            } => {
                // Real DSP from creation, unlike a device's swap-by-identity
                // effect chain (PD-020) which pivots an *already-existing*
                // plain sink between plain and filter-chain mode — an EQ
                // node has no "plain" precursor state to preserve, so this
                // loads the chain directly rather than reusing
                // `effects_ops.rs`'s capture/rollback machinery, which is
                // built specifically for that pivot.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Eq5Band {
                        id: "eq".into(),
                        eq_sub: *eq_sub,
                        eq_bass: *eq_bass,
                        eq_mid: *eq_mid,
                        eq_treble: *eq_treble,
                        eq_air: *eq_air,
                        output_gain: *output_gain,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                // Unlike the device-attached EQ path (`effects_ops.rs`'s
                // `apply_effect_chain_structural`), which pivots an
                // *already-existing* plain sink that inherited a sane
                // volume/mute state from its original `pactl` creation, this
                // sink has never existed in PipeWire before — nothing has
                // ever set its volume, so it's at the mercy of whatever
                // default WirePlumber/PipeWire applies to a brand-new
                // filter-chain node (observed: silence). Force a known-good
                // state explicitly rather than relying on that default.
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new EQ5Band sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new EQ5Band sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Delay {
                delay_ms,
                feedback_percent,
                feedforward_percent,
            } => {
                // Same "real DSP from creation" reasoning as the Eq5Band arm
                // above — no plain-sink precursor to pivot from.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Delay {
                        id: "delay".into(),
                        delay_ms: *delay_ms,
                        feedback_percent: *feedback_percent,
                        feedforward_percent: *feedforward_percent,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new Delay sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new Delay sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Limiter {
                ceiling_db,
                floor_db,
                symmetric,
            } => {
                // Same "real DSP from creation" reasoning as Eq5Band/Delay.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Limiter {
                        id: "limiter".into(),
                        ceiling_db: *ceiling_db,
                        floor_db: *floor_db,
                        symmetric: *symmetric,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new Limiter sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new Limiter sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Hpf {
                freq_hz,
                resonance_x10,
            } => {
                // Same "real DSP from creation" reasoning as Eq5Band/Delay/Limiter.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Hpf {
                        id: "hpf".into(),
                        freq_hz: *freq_hz,
                        resonance_x10: *resonance_x10,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new HPF sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new HPF sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Reverb { mix_percent } => {
                // Same "real DSP from creation" reasoning as Eq5Band/Delay/Limiter.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Reverb {
                        id: "reverb".into(),
                        mix_percent: *mix_percent,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new Reverb sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new Reverb sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Widener { width_percent } => {
                // Same "real DSP from creation" reasoning as Eq5Band/Delay/Limiter.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Widener {
                        id: "widener".into(),
                        width_percent: *width_percent,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new Widener sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new Widener sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Pan { balance_percent } => {
                // Same "real DSP from creation" reasoning as Eq5Band/Delay/Limiter.
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Pan {
                        id: "pan".into(),
                        balance_percent: *balance_percent,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                let capabilities = crate::pipewire::fx_capability::probe_capabilities();
                let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
                if !preflight.ok {
                    return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
                }
                crate::daemon::ipc::client::NativeHostClient::load_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                if let Err(error) = pactl::set_sink_volume_by_name(&node.system_name, 100) {
                    eprintln!(
                        "failed to force volume on new Pan sink {}: {error}",
                        node.system_name
                    );
                }
                if let Err(error) = pactl::set_sink_mute_by_name(&node.system_name, false) {
                    eprintln!(
                        "failed to unmute new Pan sink {}: {error}",
                        node.system_name
                    );
                }
                Ok(())
            }
            ProcessingNodeKind::Compressor {
                threshold_db,
                ratio_x10,
                attack_ms,
                release_ms,
                makeup_gain_db,
            } => {
                // Unlike every other kind above, there's no builtin
                // filter-chain dynamics primitive to fall back to at all
                // (issue #86) — this always routes through the portable DSP
                // host (issue #74's `native_dsp_host`, generalized for this
                // standalone-bus identity in PD-052), never
                // `NativeHostClient::load_chain`. No `fx_validate::preflight`
                // call either: that gates builtin-filter *availability*,
                // which is irrelevant to hand-written DSP that's always
                // present. `node.system_name` stays the plain capture
                // identity (a client `pw::stream`, not a `pactl`-visible
                // sink — the volume/mute-forcing steps the other kinds need
                // don't apply here, see `native_dsp_host`'s own "Scope/known
                // limitation" doc section).
                let config = crate::core::models::EffectChainConfig {
                    stages: vec![crate::core::models::EffectStage::Compressor {
                        id: "compressor".into(),
                        threshold_db: *threshold_db,
                        ratio_x10: *ratio_x10,
                        attack_ms: *attack_ms,
                        release_ms: *release_ms,
                        makeup_gain_db: *makeup_gain_db,
                    }],
                    bypassed: node.bypassed,
                    ..Default::default()
                };
                crate::daemon::ipc::client::NativeHostClient::load_dsp_chain(
                    &node.system_name,
                    false,
                    &config,
                )
                .map_err(|error| BackendError::Message(error.to_string()))?;
                Ok(())
            }
            ProcessingNodeKind::Stub { .. } => Ok(()),
        }
    }

    fn unload_processing_node(&self, system_name: &str) -> Result<(), BackendError> {
        if system_name.starts_with("pipe-deck-proc-compressor-") {
            // Routed exclusively through the portable DSP host (issue #86)
            // — never `native_host`'s filter-chain module path, so this has
            // no `is_loaded` guard to check first the way the builtin-module
            // kinds below do; `native_dsp_host::unload_chain` is already a
            // documented no-op if nothing is loaded.
            crate::daemon::ipc::client::NativeHostClient::unload_dsp_chain(system_name)
                .map_err(|error| BackendError::Message(error.to_string()))?;
            return Ok(());
        }
        if system_name.starts_with("pipe-deck-proc-eq5band-")
            || system_name.starts_with("pipe-deck-proc-delay-")
            || system_name.starts_with("pipe-deck-proc-limiter-")
            || system_name.starts_with("pipe-deck-proc-hpf-")
            || system_name.starts_with("pipe-deck-proc-reverb-")
            || system_name.starts_with("pipe-deck-proc-widener-")
            || system_name.starts_with("pipe-deck-proc-pan-")
        {
            if crate::daemon::ipc::client::NativeHostClient::is_loaded(system_name) {
                crate::daemon::ipc::client::NativeHostClient::unload_chain(system_name)
                    .map_err(|error| BackendError::Message(error.to_string()))?;
            }
            return Ok(());
        }

        // A Mixer's per-input feed sinks (see `relink_processing_node_port`)
        // are owned by this node and have no independent lifetime — GC them
        // all before unloading the node's own sink, the same
        // capture-nothing/tear-down-everything reasoning
        // `disconnect_all_virtual_mic_mixes` already applies to a plain
        // virtual mic. Harmless no-op for a Fan-out node, which never has any.
        let _ = pactl::gc_feed_sinks_for_mix_pairs(system_name, &std::collections::HashSet::new());
        if let Some(module_id) = pactl::find_module_id_by_sink_name(system_name)? {
            pactl::unload_module(&module_id)?;
        }
        Ok(())
    }

    fn is_processing_node_loaded(&self, system_name: &str) -> bool {
        if system_name.starts_with("pipe-deck-proc-compressor-") {
            return crate::daemon::ipc::client::NativeHostClient::is_dsp_chain_loaded(system_name);
        }
        if system_name.starts_with("pipe-deck-proc-eq5band-") {
            return crate::daemon::ipc::client::NativeHostClient::is_loaded(system_name);
        }
        pactl::sink_exists(system_name).unwrap_or(false)
    }

    fn relink_processing_node_port(
        &self,
        graph: &RuntimeGraph,
        system_name: &str,
        port_index: u32,
        direction: PortDirection,
        peer_id: Option<&str>,
    ) -> Result<(), BackendError> {
        let node = graph
            .processing_nodes
            .iter()
            .find(|node| node.system_name == system_name);
        let is_mixer = matches!(
            node.map(|node| &node.kind),
            Some(ProcessingNodeKind::Mixer { .. })
        );

        // A Stub node (issue #293's 11 non-DSP kinds) has no backing
        // PipeWire object at all — `load_processing_node` never creates one
        // (see `ProcessingNodeKind::Stub`'s doc comment) — so there is
        // nothing real to link or unlink here. The connection still exists
        // as graph data (`CoreEngine::connect_processing_node_port` persists
        // it regardless of kind); this is purely the "don't attempt a
        // pw-link against a sink that was never created" guard.
        if matches!(
            node.map(|node| &node.kind),
            Some(ProcessingNodeKind::Stub { .. })
        ) {
            return Ok(());
        }

        // A "sink-like" peer is anything addressable by system_name the same
        // way a plain device is: a Device, or another processing node
        // (PD-032 phase 5's chaining follow-up — Mixer -> Fan-out,
        // Fan-out -> Mixer, etc. all resolve through here identically,
        // since a processing node's own identity is just as system-name-
        // addressable as a device's). Streams are handled separately below
        // — they're pactl sink-inputs moved onto a target, not devices with
        // ports to `pw-link`.
        let resolve_sink_like = |id: &str| -> Option<(String, bool)> {
            if let Some(device) = graph.devices.iter().find(|device| device.id == id) {
                let target_is_virtual_source = device.kind == DeviceKind::Virtual
                    && device.direction == DeviceDirection::Input;
                return Some((device.system_name.clone(), target_is_virtual_source));
            }
            graph
                .processing_nodes
                .iter()
                .find(|peer_node| peer_node.id == id)
                .map(|peer_node| (peer_node.system_name.clone(), false))
        };

        match direction {
            PortDirection::Input => match peer_id {
                Some(id) => {
                    if let Some((peer_system_name, _)) = resolve_sink_like(id) {
                        // Route peer-source resolution through the same
                        // post-DSP `effect_output.*` preference the Output
                        // arm below already uses (`split_sink::
                        // effective_fan_out_source`) — a plain device peer
                        // falls straight through to its own system_name
                        // unchanged, but a processing-node peer whose real
                        // audio lives on an effect output (currently only
                        // EQ5Band) would otherwise get linked from its raw,
                        // pre-DSP sink monitor instead.
                        let peer_source = split_sink::effective_fan_out_source(&peer_system_name);
                        if is_mixer {
                            // Each input gets its own independent-gain feed
                            // sink (PD-032: the Mixer Node generalizes the
                            // existing mic-mix mechanism — see
                            // `virtual_mic_mix::apply_virtual_mic_mix` for
                            // the device-scoped original this mirrors)
                            // rather than a direct unity-gain link.
                            let node_label =
                                node.map(|node| node.label.as_str()).unwrap_or(system_name);
                            let feed_name = pactl::ensure_feed_sink_for_mix_pair(
                                system_name,
                                &peer_system_name,
                                node_label,
                            )?;
                            pw_link::link_capture_source_to_sink(&peer_source, &feed_name)?;
                            pactl::set_sink_volume_by_name(&feed_name, 100)?;
                            pw_link::link_sink_monitor_to_target(&feed_name, system_name, false)
                        } else {
                            pw_link::link_sink_monitor_to_target(&peer_source, system_name, false)
                        }
                    } else if graph.streams.iter().any(|stream| stream.id == id) {
                        if is_mixer {
                            // A stream is a sink-input, not a device with
                            // ports to `pw-link` — move it onto its own
                            // per-pair feed sink (same independent-gain
                            // mechanism as a device source above) instead of
                            // linking a monitor. Named off the raw peer id
                            // (not the stream's own system_name, which may
                            // still be unresolved) so connect and disconnect
                            // always compute the identical feed sink name.
                            let node_label =
                                node.map(|node| node.label.as_str()).unwrap_or(system_name);
                            let feed_name =
                                pactl::ensure_feed_sink_for_mix_pair(system_name, id, node_label)?;
                            pactl::move_stream_to_sink_name(graph, id, &feed_name)?;
                            pactl::set_sink_volume_by_name(&feed_name, 100)?;
                            pw_link::link_sink_monitor_to_target(&feed_name, system_name, false)
                        } else {
                            pactl::move_stream_to_sink_name(graph, id, system_name)?;
                            // `pactl move-sink-input` only updates the
                            // Pulse-compat "current sink" for this stream —
                            // a native (non-Pulse) client's actual output
                            // ports can stay linked to wherever they were
                            // before, independent of that move (issue #303
                            // follow-up: confirmed live via `pw-link -l`
                            // showing a stream linked to both its old and
                            // new destination at once right after this exact
                            // move). Clean up anything left over so audio
                            // doesn't keep flowing to both places.
                            if let Some(stream_system_name) = graph
                                .streams
                                .iter()
                                .find(|stream| stream.id == id)
                                .and_then(|stream| stream.system_name.as_deref())
                            {
                                let _ = pw_link::disconnect_stale_output_links(
                                    stream_system_name,
                                    system_name,
                                );
                            }
                            Ok(())
                        }
                    } else {
                        Err(BackendError::Message(format!(
                            "relink peer not found: {id}"
                        )))
                    }
                }
                // Disconnecting a stream's route is a graph-model concept
                // (forget the desired route), not a forced move — same
                // semantics as `clear_stream_target` elsewhere, *unless*
                // it's a Mixer input, whose stream feed sink is this node's
                // own object to tear down (mirrors the device-source case).
                None => {
                    let Some(node) = node else {
                        return Ok(());
                    };
                    let Some(previous_id) = node
                        .inputs
                        .iter()
                        .find(|port| port.index == port_index)
                        .and_then(|port| port.connected_id.as_deref())
                    else {
                        return Ok(());
                    };
                    if let Some((peer_system_name, _)) = resolve_sink_like(previous_id) {
                        if is_mixer {
                            pactl::remove_feed_sink_for_mix_pair(system_name, &peer_system_name)
                        } else {
                            // Must resolve the same effective source the
                            // connect side used above, or a torn-down link
                            // that was actually made from a peer's
                            // `effect_output.*` port never gets disconnected
                            // — leaving live, unaccounted-for audio behind
                            // exactly like the retarget-leak bug bb25d6d
                            // fixed for the device-side case.
                            let peer_source =
                                split_sink::effective_fan_out_source(&peer_system_name);
                            pw_link::disconnect_sink_monitor_route(&peer_source, system_name)
                        }
                    } else if is_mixer {
                        pactl::remove_feed_sink_for_mix_pair(system_name, previous_id)
                    } else {
                        Ok(())
                    }
                }
            },
            PortDirection::Output => {
                // A node's *processed* output leaves via `effect_output.*`
                // once live DSP is loaded (Eq5Band); falls back to the
                // node's own sink monitor otherwise — same source-resolution
                // rule `split_sink::effective_fan_out_source` already
                // applies to a Device, generalized here since it only ever
                // needed a system_name in the first place.
                let link_source = split_sink::effective_fan_out_source(system_name);
                match peer_id {
                    Some(id) => {
                        let (target_system_name, target_is_virtual_source) = resolve_sink_like(id)
                            .ok_or_else(|| {
                                BackendError::Message(format!("relink target not found: {id}"))
                            })?;
                        pw_link::link_sink_monitor_to_target(
                            &link_source,
                            &target_system_name,
                            target_is_virtual_source,
                        )
                    }
                    None => {
                        let Some(node) = graph
                            .processing_nodes
                            .iter()
                            .find(|node| node.system_name == system_name)
                        else {
                            return Ok(());
                        };
                        let Some(previous_id) = node
                            .outputs
                            .iter()
                            .find(|port| port.index == port_index)
                            .and_then(|port| port.connected_id.as_deref())
                        else {
                            return Ok(());
                        };
                        match resolve_sink_like(previous_id) {
                            Some((target_system_name, _)) => {
                                pw_link::disconnect_sink_monitor_route(
                                    &link_source,
                                    &target_system_name,
                                )
                            }
                            None => Ok(()),
                        }
                    }
                }
            }
        }
    }

    fn set_processing_node_input_gain(
        &self,
        system_name: &str,
        peer_system_name: &str,
        gain_percent: u8,
        muted: bool,
    ) -> Result<(), BackendError> {
        let feed_name = pactl::feed_sink_name_for_mix_pair(system_name, peer_system_name);
        pactl::set_sink_volume_by_name(&feed_name, gain_percent)?;
        pactl::set_sink_mute_by_name(&feed_name, muted)
    }

    fn set_processing_node_volume(
        &self,
        system_name: &str,
        volume_percent: u8,
        muted: bool,
    ) -> Result<(), BackendError> {
        pactl::set_sink_volume_by_name(system_name, volume_percent)?;
        pactl::set_sink_mute_by_name(system_name, muted)
    }

    fn set_processing_node_eq_params(
        &self,
        system_name: &str,
        eq_sub: i32,
        eq_bass: i32,
        eq_mid: i32,
        eq_treble: i32,
        eq_air: i32,
        output_gain: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".into(),
                eq_sub,
                eq_bass,
                eq_mid,
                eq_treble,
                eq_air,
                output_gain,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        // Pushed over the daemon's persistent native PipeWire connection
        // (`native_host::set_param`) instead of shelling out to
        // `pw-dump`+`pw-cli set-param` per slider tick — removes both the
        // node-id lookup shell-out and the brittle stderr-scraping success
        // check from this hot path. `pw_cli::set_params` stays in place for
        // the device-attached EQ path (PD-020, `effects_ops.rs`), which
        // isn't part of this migration.
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_delay_params(
        &self,
        system_name: &str,
        delay_ms: i32,
        feedback_percent: i32,
        feedforward_percent: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Delay {
                id: "delay".into(),
                delay_ms,
                feedback_percent,
                feedforward_percent,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_limiter_params(
        &self,
        system_name: &str,
        ceiling_db: i32,
        floor_db: i32,
        symmetric: bool,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Limiter {
                id: "limiter".into(),
                ceiling_db,
                floor_db,
                symmetric,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_hpf_params(
        &self,
        system_name: &str,
        freq_hz: i32,
        resonance_x10: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Hpf {
                id: "hpf".into(),
                freq_hz,
                resonance_x10,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_reverb_params(
        &self,
        system_name: &str,
        mix_percent: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Reverb {
                id: "reverb".into(),
                mix_percent,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_widener_params(
        &self,
        system_name: &str,
        width_percent: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Widener {
                id: "widener".into(),
                width_percent,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    fn set_processing_node_pan_params(
        &self,
        system_name: &str,
        balance_percent: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Pan {
                id: "pan".into(),
                balance_percent,
            }],
            bypassed,
            ..Default::default()
        };
        let capabilities = crate::pipewire::fx_capability::probe_capabilities();
        let preflight = crate::pipewire::fx_validate::preflight(&config, &capabilities);
        if !preflight.ok {
            return Err(BackendError::Message(preflight.blocking_reasons.join("; ")));
        }
        let params = crate::pipewire::fx_validate::live_params(&config);
        push_eq_params_and_reforce_volume(
            || {
                crate::daemon::ipc::client::NativeHostClient::set_param(system_name, &params)
                    .map_err(|error| BackendError::Message(error.to_string()))
            },
            || {
                let _ = pactl::set_sink_volume_by_name(system_name, 100);
                let _ = pactl::set_sink_mute_by_name(system_name, false);
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn set_processing_node_compressor_params(
        &self,
        system_name: &str,
        threshold_db: i32,
        ratio_x10: i32,
        attack_ms: i32,
        release_ms: i32,
        makeup_gain_db: i32,
        bypassed: bool,
    ) -> Result<(), BackendError> {
        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Compressor {
                id: "compressor".into(),
                threshold_db,
                ratio_x10,
                attack_ms,
                release_ms,
                makeup_gain_db,
            }],
            bypassed,
            ..Default::default()
        };
        // No preflight/pactl-reforce steps — see `load_processing_node`'s
        // Compressor arm's doc comment for why neither applies here.
        crate::daemon::ipc::client::NativeHostClient::set_dsp_chain_live_params(
            system_name,
            &config,
        )
        .map_err(|error| BackendError::Message(error.to_string()))
    }
}

/// Runs `push`, then unconditionally runs `reforce`, then returns `push`'s
/// own result — split out from `set_processing_node_eq_params` so the
/// ordering itself (reforce must run even when push fails) is a plain
/// function `push_eq_params_and_reforce_volume_reforces_even_when_push_fails`
/// below can assert on without touching PipeWire. This is the exact bug
/// class that shipped before: `push_result?` short-circuited past the
/// volume/mute reforce whenever the live param push errored, leaving a
/// freshly-created EQ node's sink muted with no way to un-stick it short of
/// a slider drag that happened to succeed.
fn push_eq_params_and_reforce_volume(
    push: impl FnOnce() -> Result<(), BackendError>,
    reforce: impl FnOnce(),
) -> Result<(), BackendError> {
    let push_result = push();
    reforce();
    push_result
}

fn query_pipewire_version() -> Option<String> {
    let output = sysproc::command("pw-cli").arg("--version").output().ok()?;
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

/// Retries `pw_link::route_playback_stream` until it succeeds or `timeout`
/// elapses (#428's stream-routing replacement for `pactl::move_sink_input_with_retry`)
/// — a target sink recreated moments ago by `revert_to_plain_device` (or an
/// effects-hosted node reloaded by `swap_to_effect_chain`) can still be a
/// beat away from actually being live even after that caller's own shorter
/// wait already gave up.
fn move_stream_with_retry(stream_system_name: &str, target_system_name: &str, timeout: Duration) {
    let start = Instant::now();
    loop {
        if pw_link::route_playback_stream(stream_system_name, target_system_name).is_ok() {
            return;
        }
        if start.elapsed() > timeout {
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// Resolves each path node to a live PipeWire node id, then computes
/// theoretical/buffering latency per hop from a single `pw-top -b -n 2` run
/// (see `pipewire::pw_top` for why two iterations are required). `Device`/
/// `Stream` ids are `"node-{n}"` and resolve for free by stripping the
/// prefix; anything else (processing nodes) resolves via `native_graph`'s
/// live name index (#411) first, falling back to the bulk `pw-dump`
/// shellout (`pw_cli::find_node_ids_by_names`) for whatever's still missing
/// — either because the native watcher isn't running or a node is too
/// recent to have reached the index yet.
fn measure_latency_ping_live(
    path: &[LatencyPathNode],
    native_graph: Option<&pw_registry::NativeGraphWatcher>,
) -> Result<LatencyPingResult, BackendError> {
    let mut resolved_ids: Vec<Option<u32>> = Vec::with_capacity(path.len());
    let mut names_to_resolve: Vec<String> = Vec::new();

    for node in path {
        if let Some(id) = node
            .id
            .strip_prefix("node-")
            .and_then(|suffix| suffix.parse::<u32>().ok())
        {
            resolved_ids.push(Some(id));
        } else if let Some(system_name) = &node.system_name {
            names_to_resolve.push(system_name.clone());
            resolved_ids.push(None);
        } else {
            resolved_ids.push(None);
        }
    }

    let mut name_lookup = if names_to_resolve.is_empty() {
        std::collections::HashMap::new()
    } else {
        native_graph
            .map(|watcher| watcher.find_node_ids(&names_to_resolve))
            .unwrap_or_default()
    };
    let still_missing: Vec<String> = names_to_resolve
        .iter()
        .filter(|name| !name_lookup.contains_key(*name))
        .cloned()
        .collect();
    if !still_missing.is_empty() {
        name_lookup.extend(crate::pipewire::pw_cli::find_node_ids_by_names(
            &still_missing,
        )?);
    }

    for (index, node) in path.iter().enumerate() {
        if resolved_ids[index].is_some() {
            continue;
        }
        if let Some(system_name) = &node.system_name {
            resolved_ids[index] = name_lookup.get(system_name).copied();
        }
    }

    let rows = crate::pipewire::pw_top::run_batch(2)?;

    let mut hops = Vec::with_capacity(path.len());
    let mut total_latency_ms = Some(0.0_f64);

    for (node, node_id) in path.iter().zip(resolved_ids.iter().copied()) {
        let row = node_id.and_then(|id| rows.iter().find(|row| row.node_id == id));
        let quantum = row.and_then(|row| row.quantum);
        let rate = row.and_then(|row| row.rate);
        let latency_ms = match (quantum, rate) {
            (Some(quantum), Some(rate)) => Some(f64::from(quantum) / f64::from(rate) * 1000.0),
            _ => None,
        };

        match (total_latency_ms, latency_ms) {
            (Some(total), Some(hop_ms)) => total_latency_ms = Some(total + hop_ms),
            (_, None) => total_latency_ms = None,
            _ => {}
        }

        hops.push(LatencyHop {
            id: node.id.clone(),
            node_id,
            quantum,
            rate,
            latency_ms,
        });
    }

    Ok(LatencyPingResult {
        hops,
        total_latency_ms,
    })
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
    let mut child = match sysproc::command("pw-dump")
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
    use super::{parse_pipewire_version, push_eq_params_and_reforce_volume};
    use crate::backend::BackendError;
    use std::cell::Cell;

    #[test]
    fn parses_linked_with_line() {
        let output = "pw-cli\nCompiled with libpipewire 1.0.5\nLinked with libpipewire 1.0.5\n";
        assert_eq!(parse_pipewire_version(output), Some("1.0.5".to_string()));
    }

    #[test]
    fn none_for_unexpected_output() {
        assert_eq!(parse_pipewire_version("command not found"), None);
    }

    // These two tests cover *only* the reforce-ordering bug fixed in
    // `push_eq_params_and_reforce_volume` — the volume/mute reforce running
    // unconditionally regardless of push success/failure. They say nothing
    // about whether the EQ node's DSP actually processes audio once live:
    // that turned out to still be broken (issue #303, muted audio through
    // the whole chain) via a different mechanism these injected closures
    // never touch (the real `pw-cli`/native IPC push and the filter-chain's
    // own signal path). Do not read a pass here as "EQ live-push is
    // solid" — it only means this one ordering bug stays fixed.
    #[test]
    fn push_eq_params_and_reforce_volume_reforces_even_when_push_fails() {
        let reforced = Cell::new(false);
        let result = push_eq_params_and_reforce_volume(
            || Err(BackendError::Message("simulated push failure".into())),
            || reforced.set(true),
        );
        assert!(
            reforced.get(),
            "volume/mute reforce must run even when the live param push errors"
        );
        assert!(
            result.is_err(),
            "the original push error must still be surfaced to the caller"
        );
    }

    #[test]
    fn push_eq_params_and_reforce_volume_reforces_and_succeeds_on_a_clean_push() {
        let reforced = Cell::new(false);
        let result = push_eq_params_and_reforce_volume(|| Ok(()), || reforced.set(true));
        assert!(reforced.get());
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Exercises
    //! `hold_sink_inputs_for_swap`/`release_held_sink_inputs` (#428) —
    //! the actual trait methods `effects_ops.rs`'s structural-swap path
    //! calls — directly, rather than driving the full effects-swap flow,
    //! to isolate whether the hold/release dance itself (now stream-name
    //! keyed, not `pactl` sink-input-index keyed) actually parks and
    //! restores a real stream correctly.
    use super::*;
    use crate::backend::AudioBackend;
    use std::process::{Child, Command, Stdio};

    struct LoopedPlaybackStream {
        child: Child,
    }

    impl LoopedPlaybackStream {
        fn spawn(target_system_name: &str) -> Self {
            let clip = "/usr/share/sounds/speech-dispatcher/test.wav";
            assert!(
                std::path::Path::new(clip).is_file(),
                "expected a system test wav to exist at {clip}"
            );
            let child = Command::new("sh")
                .args(["-c", &format!("while true; do pw-cat --playback --target '{target_system_name}' '{clip}'; done")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("failed to spawn looped pw-cat");
            Self { child }
        }
    }

    impl Drop for LoopedPlaybackStream {
        fn drop(&mut self) {
            let _ = Command::new("pkill")
                .args(["-P", &self.child.id().to_string()])
                .status();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    #[ignore]
    fn holds_and_releases_a_real_stream_across_a_simulated_swap() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend
            .create_virtual_output("Pipe Deck Hold Release Test", false)
            .expect("create disposable device");

        let _stream = LoopedPlaybackStream::spawn(&device.system_name);
        let indexed = (0..50).any(|_| {
            if pw_link::has_output_ports("pw-cat") {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            indexed,
            "expected the looped pw-cat stream to register as a live node"
        );

        // Give the stream a moment to actually land on the device (pw-cat's
        // own --target resolution is itself async) before holding it.
        let landed = (0..20).any(|_| {
            if pw_link::list_capture_sources_for_sink(&device.system_name)
                .iter()
                .any(|name| name == "pw-cat")
            {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            landed,
            "expected pw-cat to land on the disposable device before the hold/release test begins"
        );

        let held = backend
            .hold_sink_inputs_for_swap(&device.system_name)
            .expect("hold should succeed");
        assert_eq!(
            held,
            vec!["pw-cat".to_string()],
            "expected the held list to be the stream's own node name"
        );

        let held_on_holding_sink = (0..20).any(|_| {
            if pw_link_native_shows_link("pw-cat", pactl::HOLDING_SINK_NAME) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            held_on_holding_sink,
            "expected pw-cat to be parked on the holding sink"
        );
        assert!(
            !pw_link_native_shows_link("pw-cat", &device.system_name),
            "expected pw-cat to no longer be linked into the original device while held"
        );

        backend
            .release_held_sink_inputs(&held, &device.system_name)
            .expect("release should succeed");

        let released_back = (0..20).any(|_| {
            if pw_link_native_shows_link("pw-cat", &device.system_name) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            released_back,
            "expected pw-cat to be moved back onto the original device after release"
        );

        let _ = backend.remove_virtual_device(&device.system_name);
    }

    fn pw_link_native_shows_link(source_system_name: &str, target_system_name: &str) -> bool {
        let output = crate::sysproc::command("pw-link")
            .arg("-l")
            .output()
            .expect("failed to run pw-link -l");
        let text = String::from_utf8_lossy(&output.stdout);
        let source_prefix = format!("{source_system_name}:");
        let target_prefix = format!("{target_system_name}:");
        let mut current_target: Option<String> = None;
        for line in text.lines() {
            if let Some(port) = line.strip_prefix("  |<- ") {
                if let Some(target) = &current_target {
                    if target.starts_with(&target_prefix) && port.trim().starts_with(&source_prefix)
                    {
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
}
