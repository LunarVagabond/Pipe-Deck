//! End-to-end regression coverage for `CoreEngine` against `MockAudioBackend`.
//!
//! Before issue #68's `AudioBackend` refactor, none of this had automated
//! coverage — mixer/routing/virtual-device mutations against the mock data
//! source were only ever checked by hand via `PIPE_DECK_USE_MOCK=1 make dev`.
//! These tests exercise the same call paths `cargo test`-style so a future
//! change to the trait or its Linux/mock implementations gets a real signal
//! before it ships, not just a clean `cargo check`.

use pipe_deck_lib::backend::mock::MockAudioBackend;
use pipe_deck_lib::backend::AudioBackend;
use pipe_deck_lib::config::ConfigStore;
use pipe_deck_lib::core::engine::CoreEngine;
use pipe_deck_lib::core::models::{
    Device, DeviceDirection, DeviceKind, LatencyPathNode, Profile, Rule, RuleAction, RuleCondition, RuntimeGraph,
    Stream, StreamDirection, VirtualDeviceSpec,
};
use pipe_deck_lib::core::restore;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Serializes every test in this file against the others. They all mutate
/// the same process-wide `PIPE_DECK_CONFIG_DIR`/`PIPE_DECK_USE_MOCK` env
/// vars (see `tests/plugin_host_integration.rs`'s identical pattern), and
/// the guard must be held for the whole test — not just this setup call —
/// since anything the test does afterward (`ConfigStore::new()` inside a
/// `CoreEngine` method) re-reads the current environment.
fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `PIPE_DECK_USE_MOCK=1` only fakes the PipeWire graph — `ConfigStore`
/// still resolves to a real directory unless `PIPE_DECK_CONFIG_DIR` is also
/// overridden, so without this every test here would read/write the
/// developer's actual `~/.config/pipe-deck/` instead of an isolated temp
/// dir.
fn mock_engine() -> (CoreEngine, MutexGuard<'static, ()>) {
    let guard = lock_env();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let config_dir = std::env::temp_dir().join(format!(
        "pipe-deck-mock-backend-test-config-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&config_dir);
    std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
    std::env::set_var("PIPE_DECK_USE_MOCK", "1");
    let mut engine = CoreEngine::new();
    engine.refresh_graph().expect("initial refresh should succeed");
    (engine, guard)
}

#[test]
fn mixer_mutations_persist_across_refresh() {
    let (mut engine, _guard) = mock_engine();
    let device_id = engine.runtime_graph().devices[0].id.clone();
    let stream_id = engine.runtime_graph().streams[0].id.clone();

    engine.set_device_volume(&device_id, 55).unwrap();
    engine.set_device_mute(&device_id, true).unwrap();
    engine.set_stream_volume(&stream_id, 20).unwrap();
    engine.set_stream_mute(&stream_id, true).unwrap();
    engine.refresh_graph().unwrap();

    let device = engine.runtime_graph().devices.iter().find(|d| d.id == device_id).unwrap();
    assert_eq!(device.volume_percent, Some(55));
    assert_eq!(device.muted, Some(true));

    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap();
    assert_eq!(stream.volume_percent, Some(20));
    assert_eq!(stream.muted, Some(true));
}

#[test]
fn stream_routing_set_clear_and_undo_round_trip() {
    let (mut engine, _guard) = mock_engine();
    let graph = engine.runtime_graph().clone();
    let stream_id = graph.streams[0].id.clone();
    let target_a = graph.devices[1].id.clone();
    let target_b = graph.devices[2].id.clone();

    let result = engine.set_stream_target(&stream_id, &target_a).unwrap();
    assert!(result.success, "{:?}", result.message);
    assert_eq!(
        engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap().current_target.as_deref(),
        Some(target_a.as_str())
    );

    let result = engine.set_stream_target(&stream_id, &target_b).unwrap();
    assert!(result.success, "{:?}", result.message);

    let undo = engine.undo_last_routing().unwrap();
    assert!(undo.success, "{:?}", undo.message);
    assert_eq!(
        engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap().current_target.as_deref(),
        Some(target_a.as_str()),
        "undo should restore the previously set target"
    );

    let clear = engine.clear_stream_target(&stream_id, Some(&target_a)).unwrap();
    assert!(clear.success, "{:?}", clear.message);
    engine.refresh_graph().unwrap();
    assert_eq!(
        engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap().current_target,
        None,
        "cleared route must stay cleared across a refresh, not just until the next fetch"
    );
}

/// Issue #305: mic passthrough for a stream already routed to a Pipe
/// Deck-owned virtual output taps that device's monitor directly. This one
/// covers the retired-Bus-routing gap the ticket is actually about: Spotify
/// playing straight to a bare hardware sink, no Pipe Deck monitor in front
/// of it to tap. The fix auto-provisions a Fan-Out node instead — this
/// checks it ends up wired as stream -> Fan-Out -> {original hardware sink,
/// mic}, so the mic hears it *and* the original destination keeps playing.
#[test]
fn mic_passthrough_from_a_bare_hardware_target_auto_provisions_a_fan_out() {
    use pipe_deck_lib::core::models::PortDirection;

    let (mut engine, _guard) = mock_engine();

    engine.set_stream_target("stream-spotify", "sink-headphones").expect("route spotify to hardware sink first");

    let result = engine
        .enable_stream_mic_passthrough("stream-spotify", "source-mic-filtered")
        .expect("enable passthrough");
    assert!(result.success, "{:?}", result.message);

    let graph = engine.runtime_graph();
    let fan_out = graph
        .processing_nodes
        .iter()
        .find(|node| node.label == "Spotify Passthrough")
        .expect("auto-provisioned fan-out node");
    assert_eq!(fan_out.inputs.len(), 1);
    assert_eq!(fan_out.inputs[0].connected_id.as_deref(), Some("stream-spotify"));
    assert_eq!(fan_out.outputs.len(), 2);
    let output_targets: Vec<&str> = fan_out.outputs.iter().filter_map(|port| port.connected_id.as_deref()).collect();
    assert!(output_targets.contains(&"sink-headphones"), "{output_targets:?}");
    assert!(output_targets.contains(&"source-mic-filtered"), "{output_targets:?}");

    // Re-invoking passthrough for the same stream/mic pair is a no-op, not a
    // second Fan-Out or a duplicate output leg.
    let second_call = engine.enable_stream_mic_passthrough("stream-spotify", "source-mic-filtered").expect("second call");
    assert!(!second_call.success);
    let graph = engine.runtime_graph();
    let fan_out_count = graph.processing_nodes.iter().filter(|node| node.label == "Spotify Passthrough").count();
    assert_eq!(fan_out_count, 1);
    let fan_out = graph.processing_nodes.iter().find(|node| node.label == "Spotify Passthrough").unwrap();
    assert_eq!(fan_out.outputs.len(), 2, "must not grow a duplicate output leg to the mic");

    // Disconnecting just the mic leg leaves the original playback route
    // (Fan-Out -> hardware sink) untouched.
    let mic_port_index = fan_out
        .outputs
        .iter()
        .find(|port| port.connected_id.as_deref() == Some("source-mic-filtered"))
        .map(|port| port.index)
        .unwrap();
    let fan_out_id = fan_out.id.clone();
    engine.disconnect_processing_node_port(&fan_out_id, PortDirection::Output, mic_port_index).expect("disconnect mic leg");
    let graph = engine.runtime_graph();
    let fan_out = graph.processing_nodes.iter().find(|node| node.label == "Spotify Passthrough").unwrap();
    assert_eq!(fan_out.outputs.len(), 1);
    assert_eq!(fan_out.outputs[0].connected_id.as_deref(), Some("sink-headphones"));
}

/// Issue #208: deleting a virtual output with a stream actively routed
/// through it must reroute that stream to a fallback destination first,
/// instead of leaving it pointed at a now-nonexistent device id (which is
/// what caused playback to pause outright on the live backend).
#[test]
fn removing_virtual_device_reroutes_streams_routed_through_it() {
    let (mut engine, _guard) = mock_engine();

    let output = engine.create_virtual_output("Doomed Output").expect("create output");
    let stream_id = engine.runtime_graph().streams[0].id.clone();

    engine
        .set_stream_target(&stream_id, &output.device_id)
        .expect("route stream to the soon-to-be-removed device");
    assert_eq!(
        engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap().current_target,
        Some(output.device_id.clone()),
    );

    engine.remove_virtual_device(&output.system_name).expect("remove virtual device");

    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == stream_id).unwrap();
    assert_ne!(
        stream.current_target.as_deref(),
        Some(output.device_id.as_str()),
        "stream must not be left pointing at the removed device"
    );
    assert!(
        stream.current_target.is_some(),
        "stream must land on a fallback device, not be stranded with no target"
    );
}

#[test]
fn virtual_device_create_remove_cycle_leaves_no_residue() {
    let (mut engine, _guard) = mock_engine();

    let output = engine.create_virtual_output("Integration Output").expect("create output");
    assert!(engine.runtime_graph().devices.iter().any(|d| d.id == output.device_id));

    let multi = engine.create_virtual_multi_output("Integration Multi").expect("create multi output");
    assert!(multi.multi);
    assert!(engine.runtime_graph().devices.iter().any(|d| d.id == multi.device_id));

    let input = engine.create_virtual_input("Integration Input").expect("create input");
    assert!(engine.runtime_graph().devices.iter().any(|d| d.id == input.device_id));

    // Repeated create/remove cycles must not leak state in the backend's
    // held graph (regression guard for the Mutex<RuntimeGraph> design).
    for i in 0..3 {
        let created = engine.create_virtual_output(&format!("Cycle {i}")).expect("create in cycle");
        engine.remove_virtual_device(&created.system_name).expect("remove in cycle");
        assert!(!engine.runtime_graph().devices.iter().any(|d| d.id == created.device_id));
    }

    engine.remove_virtual_device(&output.system_name).unwrap();
    engine.remove_virtual_device(&multi.system_name).unwrap();
    engine.remove_virtual_device(&input.system_name).unwrap();
    for id in [&output.device_id, &multi.device_id, &input.device_id] {
        assert!(!engine.runtime_graph().devices.iter().any(|d| &d.id == id));
    }
}

/// PD-032 Phase 1: bare create/remove round trip for the new `ProcessingNode`
/// graph model — no real DSP/PipeWire wiring yet, just proving the node
/// shows up in `RuntimeGraph.processing_nodes` and cleanly disappears again,
/// the processing-node equivalent of `virtual_device_create_remove_cycle_leaves_no_residue`.
#[test]
fn processing_node_create_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Game Mixer", ProcessingNodeSpecKind::Mixer)
        .expect("create mixer node");
    assert_eq!(node.system_name, "pipe-deck-proc-mixer-game-mixer");
    assert!(matches!(node.kind, ProcessingNodeKind::Mixer { .. }));
    assert!(engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));

    let result = engine.remove_processing_node(&node.id).expect("remove mixer node");
    assert!(result.success);
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Stub effect kinds (issue #293's originally-11 non-DSP kinds, now 10 since
/// `reverb_delay` graduated to a real `Delay` processing node — issue #313)
/// round-trip the same way as real kinds through the graph model even
/// though — per PD-032 — nothing ever calls `AudioBackend::load_processing_node`'s
/// PipeWire path for them in anger; this only proves the create/remove
/// *bookkeeping* is uniform across kinds, not that Phase 5's pass-through
/// wiring exists yet.
#[test]
fn stub_processing_node_round_trips_without_error() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind, StubEffectKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Saturation", ProcessingNodeSpecKind::Stub { stub_kind: StubEffectKind::Saturation })
        .expect("create stub node");
    assert!(matches!(node.kind, ProcessingNodeKind::Stub { stub_kind: StubEffectKind::Saturation }));
    assert!(!node.live);

    engine.remove_processing_node(&node.id).expect("remove stub node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// All 7 remaining non-DSP stub kinds (issue #293, less `eq5band`/`delay`/
/// `limiter`/`hpf`/`widener` which have since graduated to real processing nodes)
/// round-trip identically: create, connect a real input and output,
/// disconnect, remove — pure pass-through graph bookkeeping, never `live`,
/// and (per a real-backend regression this caught during phase 5)
/// connect/disconnect must be a true no-op rather than attempting a
/// `pw-link` against a sink a stub never actually creates.
#[test]
fn every_stub_effect_kind_round_trips_create_connect_disconnect_remove() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind, StubEffectKind};

    let kinds = [
        StubEffectKind::Compressor,
        StubEffectKind::NoiseGate,
        StubEffectKind::Denoise,
        StubEffectKind::DeEsser,
        StubEffectKind::AutoGainLeveler,
        StubEffectKind::PitchShift,
        StubEffectKind::LoudnessNormalizer,
        StubEffectKind::Saturation,
    ];

    let (mut engine, _guard) = mock_engine();
    let upstream = engine.create_virtual_output("Stub Upstream").expect("create upstream");
    let downstream = engine.create_virtual_output("Stub Downstream").expect("create downstream");

    for stub_kind in kinds {
        let node = engine
            .create_processing_node(&format!("{stub_kind:?}"), ProcessingNodeSpecKind::Stub { stub_kind })
            .unwrap_or_else(|error| panic!("create {stub_kind:?} node: {error}"));
        assert!(!node.live, "{stub_kind:?} should never report live");

        engine
            .connect_processing_node_port(&node.id, PortDirection::Input, &upstream.device_id)
            .unwrap_or_else(|error| panic!("connect {stub_kind:?} input: {error}"));
        engine
            .connect_processing_node_port(&node.id, PortDirection::Output, &downstream.device_id)
            .unwrap_or_else(|error| panic!("connect {stub_kind:?} output: {error}"));

        engine
            .disconnect_processing_node_port(&node.id, PortDirection::Input, 0)
            .unwrap_or_else(|error| panic!("disconnect {stub_kind:?} input: {error}"));

        engine.remove_processing_node(&node.id).unwrap_or_else(|error| panic!("remove {stub_kind:?}: {error}"));
    }
}

/// Processing nodes can chain into each other in any kind/direction
/// pairing — a peer id can itself be another processing node, not just a
/// device or stream. Originally three separate tests, one per pairing
/// reported broken live in issue #293's manual testing (Fan-out output ->
/// Mixer input; the reverse, Mixer output -> Fan-out input, specifically
/// called out since "passes in mock tests" had been cited as evidence
/// something else was wrong — a claim that didn't hold up, since no
/// existing test drove this exact direction through `CoreEngine` at all;
/// and Mixer-to-Mixer, with no explicit validation-layer rule found to
/// block it on review). Consolidated into one three-node chain
/// (source -> Fan-out -> Mixer -> Mixer) once it became clear all three
/// exercise the identical `connect_processing_node_port` chaining path with
/// no kind-pair-specific branching to distinguish them — the real bug this
/// session found for these live reports was in `merge_processing_nodes`
/// (graph-merge port resolution), a code path the mock backend can't reach
/// at all, so no amount of mock-side test variety here would have caught
/// it regardless.
#[test]
fn processing_nodes_chain_through_any_kind_and_direction_pairing() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let fan_out = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out");
    let upstream_mixer = engine.create_processing_node("Voice Mix", ProcessingNodeSpecKind::Mixer).expect("create upstream mixer");
    let downstream_mixer = engine.create_processing_node("Master Mix", ProcessingNodeSpecKind::Mixer).expect("create downstream mixer");
    let source = engine.create_virtual_output("Source").expect("create source");

    // source -> Fan-out
    engine
        .connect_processing_node_port(&fan_out.id, PortDirection::Input, &source.device_id)
        .expect("connect source into fan-out");
    // Fan-out output -> Mixer input
    engine
        .connect_processing_node_port(&upstream_mixer.id, PortDirection::Input, &fan_out.id)
        .expect("chain fan-out into upstream mixer");
    // Mixer output -> Mixer input
    engine
        .connect_processing_node_port(&downstream_mixer.id, PortDirection::Input, &upstream_mixer.id)
        .expect("chain upstream mixer output into downstream mixer input");

    let graph = engine.runtime_graph();
    let fan_out_after = graph.processing_nodes.iter().find(|n| n.id == fan_out.id).unwrap();
    assert_eq!(fan_out_after.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
    assert_eq!(fan_out_after.outputs[0].connected_id.as_deref(), Some(upstream_mixer.id.as_str()));

    let upstream_after = graph.processing_nodes.iter().find(|n| n.id == upstream_mixer.id).unwrap();
    assert_eq!(upstream_after.inputs[0].connected_id.as_deref(), Some(fan_out.id.as_str()));
    assert_eq!(upstream_after.outputs[0].connected_id.as_deref(), Some(downstream_mixer.id.as_str()));

    let downstream_after = graph.processing_nodes.iter().find(|n| n.id == downstream_mixer.id).unwrap();
    assert_eq!(downstream_after.inputs[0].connected_id.as_deref(), Some(upstream_mixer.id.as_str()));
}

/// Regression for a real bug found in manual live-PipeWire testing: dragging
/// the same stream/device onto a second processing node's input (e.g. into
/// both a Mixer and a Fan-Out) must move it, not leave it double-booked. A
/// device/stream peer can only ever be wired into one place at a time — the
/// first node's port bookkeeping must be disconnected automatically when the
/// same peer connects to a second node, mirroring bb25d6d's "disconnect the
/// stale side first" fix for edge-retargets, generalized to a brand-new
/// connect landing on a different node entirely.
#[test]
fn connecting_a_peer_to_a_second_processing_node_disconnects_the_first() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let mixer = engine.create_processing_node("Mix", ProcessingNodeSpecKind::Mixer).expect("create mixer");
    let fan_out = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out");
    let source = engine.create_virtual_output("Source").expect("create source");

    engine
        .connect_processing_node_port(&mixer.id, PortDirection::Input, &source.device_id)
        .expect("connect source into mixer");
    let mixer_after_first = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == mixer.id).unwrap().clone();
    assert_eq!(mixer_after_first.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine
        .connect_processing_node_port(&fan_out.id, PortDirection::Input, &source.device_id)
        .expect("connect the same source into fan-out");

    let mixer_after_second = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == mixer.id).unwrap().clone();
    assert!(
        mixer_after_second.inputs.iter().all(|port| port.connected_id.is_none()),
        "mixer's input must be disconnected once the same source moves to fan-out, not left stale: {:?}",
        mixer_after_second.inputs
    );

    let fan_out_after = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == fan_out.id).unwrap().clone();
    assert_eq!(fan_out_after.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

/// Regression for a real bug found in manual live-PipeWire testing: chaining
/// a growable-side node (Fan-out's output) into a peer whose *own*
/// single-capacity port is already occupied by something else must be
/// rejected, not silently mirrored as a phantom extra port on the peer.
/// Before the fix, `mirror_peer_processing_node_port_connect` only ever
/// checked the calling node's own port capacity — never the peer's — so this
/// connect would succeed at the primary side while corrupting the peer's own
/// bookkeeping (Eq5Band claiming two inputs it doesn't have) and leaving the
/// peer's persisted `input_sources` and live wiring disagreeing about who's
/// actually connected.
#[test]
fn chaining_into_a_peers_already_occupied_single_port_is_rejected() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let eq = engine
        .create_processing_node(
            "EQ",
            ProcessingNodeSpecKind::Eq5Band { eq_sub: 0, eq_bass: 0, eq_mid: 0, eq_treble: 0, eq_air: 0, output_gain: 0 },
        )
        .expect("create eq node");
    let fan_out = engine
        .create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false })
        .expect("create fan-out");
    let device = engine.create_virtual_output("Device A").expect("create device a");

    engine
        .connect_processing_node_port(&eq.id, PortDirection::Input, &device.device_id)
        .expect("connect device into eq's single input");

    let error = engine
        .connect_processing_node_port(&fan_out.id, PortDirection::Output, &eq.id)
        .expect_err("chaining into eq's already-occupied input should be rejected");
    assert!(error.to_string().contains("only one input"), "{error}");

    // The rejected connect must leave both sides' bookkeeping untouched —
    // eq still shows exactly one input (the device), fan-out shows no
    // output connection at all.
    let eq_after = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == eq.id).unwrap().clone();
    assert_eq!(eq_after.inputs.len(), 1);
    assert_eq!(eq_after.inputs[0].connected_id.as_deref(), Some(device.device_id.as_str()));

    let fan_out_after = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == fan_out.id).unwrap().clone();
    assert!(fan_out_after.outputs.is_empty() || fan_out_after.outputs[0].connected_id.is_none());
}

/// PD-032's "ambiguous relink is rejected, never guessed" rule — the direct
/// #105 lesson applied to node removal. `apply_graph_update` is used here
/// (rather than any live connect command, which doesn't exist until later
/// phases) purely to seed both of a Mixer's input ports as connected.
#[test]
fn removing_a_processing_node_with_multiple_connected_inputs_is_rejected() {
    use pipe_deck_lib::core::models::{ProcessingNodePort, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Voice Mix", ProcessingNodeSpecKind::Mixer)
        .expect("create mixer node");

    let mut graph = engine.runtime_graph().clone();
    if let Some(n) = graph.processing_nodes.iter_mut().find(|n| n.id == node.id) {
        n.inputs = vec![
            ProcessingNodePort { index: 0, connected_id: Some("device-a".into()), feed_key: None },
            ProcessingNodePort { index: 1, connected_id: Some("device-b".into()), feed_key: None },
        ];
    }
    engine.apply_graph_update(graph);

    let error = engine.remove_processing_node(&node.id).expect_err("ambiguous removal should be rejected");
    assert!(error.to_string().contains("ambiguous"), "{error}");
    assert!(engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// A Fan-out Node's defining shape: one input, N outputs, each output
/// growing the port list on connect and shrinking it (dense, re-indexed) on
/// disconnect — proven here against two virtual outputs as targets.
#[test]
fn fan_out_node_output_ports_grow_and_shrink_on_connect_disconnect() {
    use pipe_deck_lib::core::models::PortDirection;

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Stream Fan-out", pipe_deck_lib::core::models::ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false })
        .expect("create fan-out node");
    let output_a = engine.create_virtual_output("Fan A").expect("create target a");
    let output_b = engine.create_virtual_output("Fan B").expect("create target b");

    engine
        .connect_processing_node_port(&node.id, PortDirection::Output, &output_a.device_id)
        .expect("connect output a");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Output, &output_b.device_id)
        .expect("connect output b");

    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.outputs.len(), 2);
    assert_eq!(refreshed.outputs[0].connected_id.as_deref(), Some(output_a.device_id.as_str()));
    assert_eq!(refreshed.outputs[1].connected_id.as_deref(), Some(output_b.device_id.as_str()));

    engine.disconnect_processing_node_port(&node.id, PortDirection::Output, 0).expect("disconnect output a");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.outputs.len(), 1);
    assert_eq!(refreshed.outputs[0].connected_id.as_deref(), Some(output_b.device_id.as_str()));
}

/// A Group Node (issue #80, PD-035) has the identical shape to Fan-out —
/// one non-growable input, N growable outputs — proven here for coverage
/// parity so a missed `ProcessingNodeKind::Group`/`ProcessingNodeSpecKind::Group`
/// match arm shows up as a failing test, not just a `FanOut`-only blind spot.
#[test]
fn group_node_output_ports_grow_and_shrink_on_connect_disconnect() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Speaker Group", ProcessingNodeSpecKind::Group { volume_percent: 100, muted: false })
        .expect("create group node");
    let output_a = engine.create_virtual_output("Group A").expect("create target a");
    let output_b = engine.create_virtual_output("Group B").expect("create target b");

    engine
        .connect_processing_node_port(&node.id, PortDirection::Output, &output_a.device_id)
        .expect("connect output a");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Output, &output_b.device_id)
        .expect("connect output b");

    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.outputs.len(), 2);

    engine.disconnect_processing_node_port(&node.id, PortDirection::Output, 0).expect("disconnect output a");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.outputs.len(), 1);
    assert_eq!(refreshed.outputs[0].connected_id.as_deref(), Some(output_b.device_id.as_str()));
}

/// The core of issue #80: selecting 2+ existing outputs and grouping them in
/// one gesture wires every member's output atomically, and — critically —
/// grouping a device doesn't claim it exclusively. It stays independently
/// routable as a normal Fan-out/Group output-side peer elsewhere afterward
/// (the "doesn't read as two unrelated, disconnected devices" requirement
/// from the issue itself).
#[test]
fn create_output_group_wires_all_members_atomically() {
    use pipe_deck_lib::core::models::PortDirection;

    let (mut engine, _guard) = mock_engine();

    let output_a = engine.create_virtual_output("Speakers").expect("create output a");
    let output_b = engine.create_virtual_output("Recorder").expect("create output b");

    let node = engine
        .create_output_group("Speakers + Recorder", &[output_a.device_id.clone(), output_b.device_id.clone()])
        .expect("create group");

    assert_eq!(node.outputs.len(), 2);
    assert_eq!(node.outputs[0].connected_id.as_deref(), Some(output_a.device_id.as_str()));
    assert_eq!(node.outputs[1].connected_id.as_deref(), Some(output_b.device_id.as_str()));

    // A grouped member device is still independently usable as a Fan-out
    // output-side peer elsewhere — grouping doesn't claim exclusivity.
    let other_fan_out = engine
        .create_processing_node(
            "Other Fan-out",
            pipe_deck_lib::core::models::ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false },
        )
        .expect("create other fan-out node");
    engine
        .connect_processing_node_port(&other_fan_out.id, PortDirection::Output, &output_a.device_id)
        .expect("output_a should still be independently connectable outside the group");
}

/// A Group's entire identity is its member set — disconnecting the last
/// remaining member removes the group itself outright, rather than leaving
/// a zero-output husk on the canvas.
#[test]
fn disconnecting_a_groups_last_member_removes_the_group_entirely() {
    use pipe_deck_lib::core::models::PortDirection;

    let (mut engine, _guard) = mock_engine();

    let output_a = engine.create_virtual_output("Speakers").expect("create output a");
    let output_b = engine.create_virtual_output("Recorder").expect("create output b");

    let node = engine
        .create_output_group("Speakers + Recorder", &[output_a.device_id.clone(), output_b.device_id.clone()])
        .expect("create group");

    engine
        .disconnect_processing_node_port(&node.id, PortDirection::Output, 0)
        .expect("disconnect first member");
    assert!(
        engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id),
        "group should still exist with one member remaining"
    );

    engine
        .disconnect_processing_node_port(&node.id, PortDirection::Output, 0)
        .expect("disconnect last remaining member");
    assert!(
        !engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id),
        "group should be auto-removed once its last member is disconnected"
    );
}

/// The auto-remove-when-empty behavior is Group-specific — a Fan-out node
/// with its last output disconnected stays around (a transient 0-output
/// state may be a deliberate mid-edit step there, unlike a Group).
#[test]
fn disconnecting_a_fan_outs_last_output_does_not_remove_the_node() {
    use pipe_deck_lib::core::models::PortDirection;

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node(
            "Solo Fan-out",
            pipe_deck_lib::core::models::ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false },
        )
        .expect("create fan-out node");
    let output_a = engine.create_virtual_output("Target").expect("create target");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Output, &output_a.device_id)
        .expect("connect output");

    engine
        .disconnect_processing_node_port(&node.id, PortDirection::Output, 0)
        .expect("disconnect only output");
    assert!(
        engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id),
        "fan-out should remain even with zero connected outputs"
    );
}

/// A group needs at least 2 members — a single-member "group" is rejected
/// outright rather than silently creating a degenerate one-output node.
#[test]
fn create_output_group_requires_at_least_two_members() {
    let (mut engine, _guard) = mock_engine();

    let output_a = engine.create_virtual_output("Speakers").expect("create output a");

    let error = engine
        .create_output_group("Solo Group", &[output_a.device_id])
        .expect_err("a single-member group should be rejected");
    assert!(error.to_string().contains("at least 2 members"), "{error}");
}

/// If wiring a member fails partway through, the freshly-created node (and
/// any members already wired) is torn down — a failed group creation never
/// leaves a half-wired node behind for the user to find and clean up.
#[test]
fn create_output_group_rolls_back_on_partial_wiring_failure() {
    let (mut engine, _guard) = mock_engine();

    let output_a = engine.create_virtual_output("Speakers").expect("create output a");

    let error = engine
        .create_output_group("Broken Group", &[output_a.device_id, "nonexistent-device".to_string()])
        .expect_err("wiring a nonexistent member should fail the whole group");
    assert!(error.to_string().contains("peer not found"), "{error}");

    assert!(
        !engine.runtime_graph().processing_nodes.iter().any(|n| n.label == "Broken Group"),
        "a partially-wired group should be torn down, not left behind"
    );
}

/// Deleting a Group/Fan-out node with many connected outputs succeeds in
/// one action — there's nothing ambiguous to relink on a growable side, only
/// disconnect. This is the fix that makes Group actually usable day-to-day
/// (a Group is expected to routinely have 3+ members); it also retroactively
/// fixes the same latent bug for a hand-built Fan-out chain.
#[test]
fn removing_a_fan_out_node_with_many_connected_outputs_succeeds() {
    let (mut engine, _guard) = mock_engine();

    let output_a = engine.create_virtual_output("Fan A").expect("create target a");
    let output_b = engine.create_virtual_output("Fan B").expect("create target b");
    let output_c = engine.create_virtual_output("Fan C").expect("create target c");

    let node = engine
        .create_output_group(
            "Big Group",
            &[output_a.device_id, output_b.device_id, output_c.device_id],
        )
        .expect("create group with 3 members");
    assert_eq!(node.outputs.len(), 3);

    engine.remove_processing_node(&node.id).expect("removing a many-output group should succeed");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// The removal fix must be side-specific, not a blanket relaxation. A
/// Mixer's growable side is its *input* (see
/// `removing_a_processing_node_with_multiple_connected_inputs_is_rejected`);
/// its *output* side is still capped at one, so 2+ connected outputs should
/// still reject removal exactly like before the Fan-out/Group fix — proving
/// the growable-side exemption is keyed off (kind, direction), not "any side
/// with 2+ connections is fine now."
#[test]
fn removing_a_mixer_node_with_multiple_connected_outputs_is_still_rejected() {
    use pipe_deck_lib::core::models::{ProcessingNodePort, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Mix", ProcessingNodeSpecKind::Mixer)
        .expect("create mixer node");

    let mut graph = engine.runtime_graph().clone();
    if let Some(n) = graph.processing_nodes.iter_mut().find(|n| n.id == node.id) {
        n.outputs = vec![
            ProcessingNodePort { index: 0, connected_id: Some("device-a".into()), feed_key: None },
            ProcessingNodePort { index: 1, connected_id: Some("device-b".into()), feed_key: None },
        ];
    }
    engine.apply_graph_update(graph);

    let error = engine.remove_processing_node(&node.id).expect_err("ambiguous removal should still be rejected");
    assert!(error.to_string().contains("ambiguous"), "{error}");
}

/// A non-growable port side rejects a second connection outright rather
/// than silently accepting one nothing downstream would even mean — checked
/// on both a growable-kind's *non*-growable side (Fan-out's single input)
/// and a different growable-kind's own non-growable side (Mixer's single
/// output), since both exercise the same `processing_node_port_growable`
/// false-branch and previously shared a real bug: the original cap only
/// checked the input side for every kind, so every kind's output side was
/// silently unlimited (a Mixer/EQ/stub showed a growable, wrong output
/// port). One test covers both sides deliberately, rather than one test per
/// kind, since the two prior tests never exercised a different code path —
/// only different (kind, direction) labels on the identical check.
#[test]
fn a_non_growable_port_side_rejects_a_second_connection() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let fan_out = engine
        .create_processing_node("Stream Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false })
        .expect("create fan-out node");
    let source_a = engine.create_virtual_output("Source A").expect("create source a");
    let source_b = engine.create_virtual_output("Source B").expect("create source b");
    engine
        .connect_processing_node_port(&fan_out.id, PortDirection::Input, &source_a.device_id)
        .expect("connect first input");
    let error = engine
        .connect_processing_node_port(&fan_out.id, PortDirection::Input, &source_b.device_id)
        .expect_err("fan-out's second input should be rejected");
    assert!(error.to_string().contains("only one input"), "{error}");

    let mixer = engine.create_processing_node("Mix", ProcessingNodeSpecKind::Mixer).expect("create mixer node");
    let target_a = engine.create_virtual_output("Target A").expect("create target a");
    let target_b = engine.create_virtual_output("Target B").expect("create target b");
    engine
        .connect_processing_node_port(&mixer.id, PortDirection::Output, &target_a.device_id)
        .expect("connect first output");
    let error = engine
        .connect_processing_node_port(&mixer.id, PortDirection::Output, &target_b.device_id)
        .expect_err("mixer's second output should be rejected");
    assert!(error.to_string().contains("only one output"), "{error}");
}

/// A Mixer Node's defining shape: N growable inputs, each with its own gain,
/// summed into a single output — the generalized replacement for mic-mix
/// (PD-032). Two sources connect, one gets a live gain update, then one
/// disconnects without disturbing the other's gain or connection.
#[test]
fn mixer_node_sums_inputs_with_independent_gain() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Voice Mix", ProcessingNodeSpecKind::Mixer)
        .expect("create mixer node");
    let source_a = engine.create_virtual_output("Mic A").expect("create source a");
    let source_b = engine.create_virtual_output("Mic B").expect("create source b");

    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source_a.device_id)
        .expect("connect input a");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source_b.device_id)
        .expect("connect input b");

    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.inputs.len(), 2);
    match &refreshed.kind {
        ProcessingNodeKind::Mixer { input_gains_percent } => assert_eq!(input_gains_percent, &vec![100, 100]),
        other => panic!("expected Mixer, got {other:?}"),
    }

    engine
        .update_processing_node_input_gain(&node.id, 0, 60, false)
        .expect("update gain for input a");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    match &refreshed.kind {
        ProcessingNodeKind::Mixer { input_gains_percent } => assert_eq!(input_gains_percent, &vec![60, 100]),
        other => panic!("expected Mixer, got {other:?}"),
    }

    engine.disconnect_processing_node_port(&node.id, PortDirection::Input, 0).expect("disconnect input a");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.inputs.len(), 1);
    assert_eq!(refreshed.inputs[0].connected_id.as_deref(), Some(source_b.device_id.as_str()));
    match &refreshed.kind {
        // Input b's gain (still at unity) survives disconnecting input a and
        // re-indexing down to slot 0.
        ProcessingNodeKind::Mixer { input_gains_percent } => assert_eq!(input_gains_percent, &vec![100]),
        other => panic!("expected Mixer, got {other:?}"),
    }
}

#[test]
fn mixer_node_input_gain_update_rejects_a_disconnected_port() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Voice Mix", ProcessingNodeSpecKind::Mixer).expect("create mixer node");

    let error = engine
        .update_processing_node_input_gain(&node.id, 0, 50, false)
        .expect_err("gain update on a disconnected port should be rejected");
    assert!(error.to_string().contains("isn't connected"), "{error}");
}

/// 5-Band EQ Node round-trip (issue #293's one fully-functional effect
/// kind): create, live-update band gains, remove.
#[test]
fn eq5band_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node(
            "Voice EQ",
            ProcessingNodeSpecKind::Eq5Band { eq_sub: 0, eq_bass: 0, eq_mid: 0, eq_treble: 0, eq_air: 0, output_gain: 0 },
        )
        .expect("create eq node");
    assert_eq!(node.system_name, "pipe-deck-proc-eq5band-voice-eq");

    engine
        .update_processing_node_eq_params(&node.id, 2, 4, 0, -2, 1, 3)
        .expect("update eq params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(
        refreshed.kind,
        ProcessingNodeKind::Eq5Band { eq_sub: 2, eq_bass: 4, eq_mid: 0, eq_treble: -2, eq_air: 1, output_gain: 3 }
    );

    engine.remove_processing_node(&node.id).expect("remove eq node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass keeps a node wired exactly as-is (nothing disconnects) while
/// flagging it as passed-through-unprocessed — same "isolate this one out"
/// meaning as the existing device-attached effect bypass.
#[test]
fn bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node(
            "Voice EQ",
            ProcessingNodeSpecKind::Eq5Band { eq_sub: 0, eq_bass: 0, eq_mid: 0, eq_treble: 0, eq_air: 0, output_gain: 0 },
        )
        .expect("create eq node");
    let source = engine.create_virtual_output("EQ Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn eq_param_update_rejects_a_non_eq_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_eq_params(&node.id, 0, 0, 0, 0, 0, 0)
        .expect_err("eq update on a non-EQ node should be rejected");
    assert!(error.to_string().contains("has no EQ params"), "{error}");
}

/// Delay Node round-trip (issue #313): create, live-update Delay/Feedback/
/// Feedforward, remove. Same pattern as `eq5band_node_create_update_remove_round_trips`.
#[test]
fn delay_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node(
            "Echo",
            ProcessingNodeSpecKind::Delay { delay_ms: 0, feedback_percent: 0, feedforward_percent: 0 },
        )
        .expect("create delay node");
    assert_eq!(node.system_name, "pipe-deck-proc-delay-echo");

    engine
        .update_processing_node_delay_params(&node.id, 350, 40, -10)
        .expect("update delay params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(
        refreshed.kind,
        ProcessingNodeKind::Delay { delay_ms: 350, feedback_percent: 40, feedforward_percent: -10 }
    );

    engine.remove_processing_node(&node.id).expect("remove delay node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles a Delay node's DSP without disturbing wiring — same
/// contract as `bypass_toggles_without_disturbing_wiring` for Eq5Band.
#[test]
fn delay_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node(
            "Echo",
            ProcessingNodeSpecKind::Delay { delay_ms: 200, feedback_percent: 30, feedforward_percent: 0 },
        )
        .expect("create delay node");
    let source = engine.create_virtual_output("Delay Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn delay_param_update_rejects_a_non_delay_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_delay_params(&node.id, 0, 0, 0)
        .expect_err("delay update on a non-Delay node should be rejected");
    assert!(error.to_string().contains("has no delay params"), "{error}");
}

/// Limiter Node round-trip (issue #311): create, live-update ceiling, remove.
/// Same pattern as `delay_node_create_update_remove_round_trips`.
#[test]
fn limiter_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node(
            "Safety Limiter",
            ProcessingNodeSpecKind::Limiter { ceiling_db: 0, floor_db: 0, symmetric: true },
        )
        .expect("create limiter node");
    assert_eq!(node.system_name, "pipe-deck-proc-limiter-safety-limiter");

    engine.update_processing_node_limiter_params(&node.id, -6, -6, true).expect("update limiter params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.kind, ProcessingNodeKind::Limiter { ceiling_db: -6, floor_db: -6, symmetric: true });

    // Asymmetric: ceiling and floor can be set independently once unlocked.
    engine.update_processing_node_limiter_params(&node.id, -3, -12, false).expect("update limiter params asymmetrically");
    let asymmetric = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(asymmetric.kind, ProcessingNodeKind::Limiter { ceiling_db: -3, floor_db: -12, symmetric: false });

    engine.remove_processing_node(&node.id).expect("remove limiter node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles a Limiter node's DSP without disturbing wiring — same
/// contract as `delay_bypass_toggles_without_disturbing_wiring`.
#[test]
fn limiter_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node(
            "Safety Limiter",
            ProcessingNodeSpecKind::Limiter { ceiling_db: -6, floor_db: -6, symmetric: true },
        )
        .expect("create limiter node");
    let source = engine.create_virtual_output("Limiter Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn limiter_param_update_rejects_a_non_limiter_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_limiter_params(&node.id, 0, 0, true)
        .expect_err("limiter update on a non-Limiter node should be rejected");
    assert!(error.to_string().contains("has no limiter params"), "{error}");
}

/// HPF Node round-trip (issue #312): create, live-update Freq/Resonance,
/// remove. Same pattern as `delay_node_create_update_remove_round_trips`.
#[test]
fn hpf_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Rumble Filter", ProcessingNodeSpecKind::Hpf { freq_hz: 20, resonance_x10: 7 })
        .expect("create hpf node");
    assert_eq!(node.system_name, "pipe-deck-proc-hpf-rumble-filter");

    engine.update_processing_node_hpf_params(&node.id, 150, 12).expect("update hpf params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.kind, ProcessingNodeKind::Hpf { freq_hz: 150, resonance_x10: 12 });

    engine.remove_processing_node(&node.id).expect("remove hpf node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles an HPF node's DSP without disturbing wiring — same
/// contract as `delay_bypass_toggles_without_disturbing_wiring`.
#[test]
fn hpf_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node("Rumble Filter", ProcessingNodeSpecKind::Hpf { freq_hz: 150, resonance_x10: 7 })
        .expect("create hpf node");
    let source = engine.create_virtual_output("HPF Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn hpf_param_update_rejects_a_non_hpf_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_hpf_params(&node.id, 0, 0)
        .expect_err("hpf update on a non-HPF node should be rejected");
    assert!(error.to_string().contains("has no HPF params"), "{error}");
}

/// Reverb Node round-trip (issue #327): create, live-update Mix, remove.
/// Same pattern as `delay_node_create_update_remove_round_trips`.
#[test]
fn reverb_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Room Verb", ProcessingNodeSpecKind::Reverb { mix_percent: 0 })
        .expect("create reverb node");
    assert_eq!(node.system_name, "pipe-deck-proc-reverb-room-verb");

    engine.update_processing_node_reverb_params(&node.id, 35).expect("update reverb params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.kind, ProcessingNodeKind::Reverb { mix_percent: 35 });

    engine.remove_processing_node(&node.id).expect("remove reverb node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles a Reverb node's DSP without disturbing wiring — same
/// contract as `delay_bypass_toggles_without_disturbing_wiring`.
#[test]
fn reverb_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node("Room Verb", ProcessingNodeSpecKind::Reverb { mix_percent: 35 })
        .expect("create reverb node");
    let source = engine.create_virtual_output("Reverb Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn reverb_param_update_rejects_a_non_reverb_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_reverb_params(&node.id, 0)
        .expect_err("reverb update on a non-Reverb node should be rejected");
    assert!(error.to_string().contains("has no reverb params"), "{error}");
}

/// Widener Node round-trip (issue #314): create, live-update Width, remove.
/// Same pattern as `delay_node_create_update_remove_round_trips`.
#[test]
fn widener_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Wide Stereo", ProcessingNodeSpecKind::Widener { width_percent: 100 })
        .expect("create widener node");
    assert_eq!(node.system_name, "pipe-deck-proc-widener-wide-stereo");

    engine.update_processing_node_widener_params(&node.id, 150).expect("update widener params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.kind, ProcessingNodeKind::Widener { width_percent: 150 });

    engine.remove_processing_node(&node.id).expect("remove widener node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles a Widener node's DSP without disturbing wiring — same
/// contract as `delay_bypass_toggles_without_disturbing_wiring`.
#[test]
fn widener_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node("Wide Stereo", ProcessingNodeSpecKind::Widener { width_percent: 150 })
        .expect("create widener node");
    let source = engine.create_virtual_output("Widener Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn widener_param_update_rejects_a_non_widener_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_widener_params(&node.id, 100)
        .expect_err("widener update on a non-Widener node should be rejected");
    assert!(error.to_string().contains("has no widener params"), "{error}");
}

/// Pan Node round-trip (issue #16): create, live-update Balance, remove.
/// Same pattern as `delay_node_create_update_remove_round_trips`.
#[test]
fn pan_node_create_update_remove_round_trips() {
    use pipe_deck_lib::core::models::{ProcessingNodeKind, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();

    let node = engine
        .create_processing_node("Mic Balance", ProcessingNodeSpecKind::Pan { balance_percent: 0 })
        .expect("create pan node");
    assert_eq!(node.system_name, "pipe-deck-proc-pan-mic-balance");

    engine.update_processing_node_pan_params(&node.id, 40).expect("update pan params");
    let refreshed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert_eq!(refreshed.kind, ProcessingNodeKind::Pan { balance_percent: 40 });

    engine.remove_processing_node(&node.id).expect("remove pan node");
    assert!(!engine.runtime_graph().processing_nodes.iter().any(|n| n.id == node.id));
}

/// Bypass toggles a Pan node's DSP without disturbing wiring — same
/// contract as `delay_bypass_toggles_without_disturbing_wiring`.
#[test]
fn pan_bypass_toggles_without_disturbing_wiring() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let node = engine
        .create_processing_node("Mic Balance", ProcessingNodeSpecKind::Pan { balance_percent: 40 })
        .expect("create pan node");
    let source = engine.create_virtual_output("Pan Source").expect("create source");
    engine
        .connect_processing_node_port(&node.id, PortDirection::Input, &source.device_id)
        .expect("connect input");

    engine.set_processing_node_bypassed(&node.id, true).expect("bypass on");
    let bypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(bypassed.bypassed);
    assert_eq!(bypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));

    engine.set_processing_node_bypassed(&node.id, false).expect("bypass off");
    let unbypassed = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == node.id).unwrap().clone();
    assert!(!unbypassed.bypassed);
    assert_eq!(unbypassed.inputs[0].connected_id.as_deref(), Some(source.device_id.as_str()));
}

#[test]
fn pan_param_update_rejects_a_non_pan_node() {
    use pipe_deck_lib::core::models::ProcessingNodeSpecKind;

    let (mut engine, _guard) = mock_engine();
    let node = engine.create_processing_node("Fan-out", ProcessingNodeSpecKind::FanOut { volume_percent: 100, muted: false }).expect("create fan-out node");

    let error = engine
        .update_processing_node_pan_params(&node.id, 0)
        .expect_err("pan update on a non-Pan node should be rejected");
    assert!(error.to_string().contains("has no pan params"), "{error}");
}

/// Any device kind can feed a Mixer node's input — a virtual output device
/// (regression coverage for #293's VirtualRole::Bus removal: a plain
/// virtual output device can no longer route onward to another device, but
/// feeding its monitor into a Mixer's input is a separate, unaffected
/// capability) and a physical input device (Mixer Node's replacement for
/// the old mic-mix mechanism, PD-032, must still accept a physical source
/// the same way mic-mix did). `connect_processing_node_port`'s validation
/// doesn't branch on device kind at all, so one test connecting both kinds
/// into the same Mixer covers this rather than two near-identical ones;
/// gain/mute tracking itself is covered generically by
/// `mixer_node_sums_inputs_with_independent_gain`.
#[test]
fn a_mixer_node_input_accepts_any_device_kind() {
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let mixer = engine.create_processing_node("Mix", ProcessingNodeSpecKind::Mixer).expect("create mixer");

    let output = engine.create_virtual_output("Output Source").expect("create output");
    let result = engine
        .connect_processing_node_port(&mixer.id, PortDirection::Input, &output.device_id)
        .expect("a virtual output device should be a valid mixer input");
    assert!(result.success, "{:?}", result.message);

    let physical_source = engine
        .runtime_graph()
        .devices
        .iter()
        .find(|d| d.kind == DeviceKind::Physical && d.direction == DeviceDirection::Input)
        .expect("sample graph should have a physical input")
        .id
        .clone();
    let result = engine
        .connect_processing_node_port(&mixer.id, PortDirection::Input, &physical_source)
        .expect("a physical input device should also be a valid mixer input");
    assert!(result.success, "{:?}", result.message);

    let node = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == mixer.id).unwrap();
    assert_eq!(node.inputs.len(), 2);
    assert_eq!(node.inputs[0].connected_id.as_deref(), Some(output.device_id.as_str()));
    assert_eq!(node.inputs[1].connected_id.as_deref(), Some(physical_source.as_str()));
}

#[test]
fn virtual_output_device_rejects_effect_attach() {
    // Device-attached output effects (the old Bus mechanism, #287) were
    // retired alongside VirtualRole::Bus — a virtual output device can no
    // longer host effects directly at all; the dedicated EQ5Band processing
    // node is the replacement. Virtual input (mic) effects are unaffected.
    let (mut engine, _guard) = mock_engine();
    let output = engine.create_virtual_output("Effects Test Output").expect("create output");

    let config = pipe_deck_lib::core::models::EffectChainConfig::default();
    let error = engine
        .apply_effect_chain_structural(&output.device_id, &config)
        .expect_err("a virtual output device must not accept a device-attached effect chain");
    assert!(error.to_string().contains("virtual input"), "{error}");
}

#[test]
fn device_alias_rename_is_visible_after_refresh() {
    let (mut engine, _guard) = mock_engine();
    let output = engine.create_virtual_output("Original Label").expect("create output");

    engine.apply_device_alias(&output.system_name, "Renamed Label").unwrap();
    engine.refresh_graph().unwrap();

    let renamed = engine.runtime_graph().devices.iter().find(|d| d.id == output.device_id).unwrap();
    assert_eq!(renamed.label, "Renamed Label");
}

#[test]
fn mixer_node_accepts_a_physical_mic_as_an_input() {
    // Mixer Node's replacement for the old mic-mix mechanism (PD-032) must
    // still accept a physical input device as a source, same as mic-mix did
    // — gain/mute tracking itself is covered generically by
    // `mixer_node_sums_inputs_with_independent_gain`.
    use pipe_deck_lib::core::models::{PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let physical_source = engine
        .runtime_graph()
        .devices
        .iter()
        .find(|d| d.kind == DeviceKind::Physical && d.direction == DeviceDirection::Input)
        .expect("sample graph should have a physical input")
        .id
        .clone();
    let mixer = engine.create_processing_node("Mic Mix", ProcessingNodeSpecKind::Mixer).expect("create mixer");

    let result = engine
        .connect_processing_node_port(&mixer.id, PortDirection::Input, &physical_source)
        .expect("connect physical mic to mixer");
    assert!(result.success, "{:?}", result.message);

    let node = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == mixer.id).unwrap();
    assert_eq!(node.inputs.len(), 1);
    assert_eq!(node.inputs[0].connected_id.as_deref(), Some(physical_source.as_str()));
}

#[test]
fn apply_effect_chain_structural_validates_even_in_mock_mode() {
    // #147: apply_effect_chain_structural no longer short-circuits to a
    // canned mock success before validation runs — it now always routes
    // through `self.adapter` (real subprocess calls for
    // LinuxPipeWireBackend, in-memory no-ops for MockAudioBackend), so the
    // is_pipe_deck_device guard has to actually fire for a non-pipe-deck
    // device even under PIPE_DECK_USE_MOCK=1. Before this change, this
    // would have silently returned a canned success instead.
    let (mut engine, _guard) = mock_engine();
    let physical_output = engine
        .runtime_graph()
        .devices
        .iter()
        .find(|device| device.id == "sink-headphones")
        .expect("mock sample graph should seed a physical output device")
        .id
        .clone();

    let config = pipe_deck_lib::core::models::EffectChainConfig::default();
    let result = engine.apply_effect_chain_structural(&physical_output, &config);
    assert!(result.is_err(), "effects on a non-pipe-deck device must be rejected, even in mock mode");
}

#[test]
fn remove_effect_chain_structural_runs_the_real_adapter_call_path_in_mock_mode() {
    // #147/#149: remove_effect_chain_structural's own precondition guard (is
    // a chain actually loaded, per `AudioBackend::is_effect_chain_loaded`)
    // is a real check, not a mock short-circuit — so exercising its adapter
    // calls (hold/release sink inputs, revert-to-plain-device, mic-feed
    // relink) needs a chain to actually be loaded first via a real
    // `apply_effect_chain_structural`, which `MockAudioBackend` tracks
    // in-memory the same way it tracks routing/mixer state.
    let (mut engine, _guard) = mock_engine();
    let mic = engine.create_virtual_input("Integration Remove Path Mic").expect("create mic");

    let config = pipe_deck_lib::core::models::EffectChainConfig {
        stages: vec![pipe_deck_lib::core::models::EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_bass: 4,
            eq_sub: 0,
            eq_mid: 0,
            eq_treble: 0,
            eq_air: 0,
            output_gain: 0,
        }],
        ..Default::default()
    };
    engine
        .apply_effect_chain_structural(&mic.device_id, &config)
        .expect("structural apply should succeed");
    assert!(
        engine.is_effect_chain_live(&mic.device_id),
        "chain should be live right after apply"
    );

    let result = engine
        .remove_effect_chain_structural(&mic.device_id)
        .expect("remove_effect_chain_structural should succeed once the adapter calls all no-op successfully");
    assert!(result.success);
    assert!(
        !engine.is_effect_chain_live(&mic.device_id),
        "remove_effect_chain_structural should have unloaded the chain"
    );
}

#[test]
fn effect_chain_applies_and_removes_on_a_virtual_input_device() {
    // PD-024: effects extend from virtual output-only to virtual input
    // (mic) devices too. #147 routes both apply and remove through
    // `self.adapter` (real subprocess calls for LinuxPipeWireBackend,
    // in-memory no-ops for MockAudioBackend) rather than a top-of-function
    // mock short-circuit — this locks in that the direction-aware guard in
    // `apply_effect_chain_structural`/`remove_effect_chain_structural`
    // accepts an Input-direction device at all (previously only
    // `DeviceDirection::Output` was permitted), and that the persisted
    // chain round-trips through `get_effect_chains` the same way it
    // already does for outputs.
    let (mut engine, _guard) = mock_engine();
    let mic = engine.create_virtual_input("Integration Effects Mic").expect("create input");

    let config = pipe_deck_lib::core::models::EffectChainConfig {
        stages: vec![pipe_deck_lib::core::models::EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_bass: 6,
            eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
        }],
        ..Default::default()
    };

    engine
        .apply_effect_chain_structural(&mic.device_id, &config)
        .expect("apply_effect_chain_structural should succeed for a virtual input device");
    engine
        .remove_effect_chain_structural(&mic.device_id)
        .expect("remove_effect_chain_structural should succeed for a virtual input device");

    // `set_device_effects` (the persist-only path `Effects.vue` uses before
    // live effects are ever enabled) must round-trip through
    // `get_effect_chains` for an input device the same way it already does
    // for outputs.
    engine.set_device_effects(&mic.device_id, config).expect("set_device_effects");
    let chains = engine.get_effect_chains().expect("get_effect_chains");
    assert_eq!(chains.get(&mic.device_id).map(|c| c.eq_stage().eq_bass), Some(6));
}

#[test]
fn add_remove_reorder_effect_stage_round_trips() {
    // PD-025: the node-scoped effects UI entry points — no separate
    // "enable live effects" step, add/remove/reorder apply immediately.
    // `add_effect_stage`/`remove_effect_stage`/`reorder_effect_stages` are
    // built on `apply_effect_chain_structural`/`remove_effect_chain_structural`.
    // #147: apply routes through `self.adapter`'s real call path even in
    // mock mode (MockAudioBackend no-ops rather than short-circuiting);
    // remove's own precondition guard (no conf file exists, since nothing
    // in this test writes one) still returns an early success without
    // reaching the adapter — see
    // `remove_effect_chain_structural_runs_the_real_adapter_call_path_in_mock_mode`
    // for a test that does reach it. This test locks in that each call
    // succeeds and reads back its own in-flight config correctly (stage
    // appended/reordered/removed), not that mock-mode persists across a
    // fresh `get_effect_chains()` fetch.
    use pipe_deck_lib::core::models::EffectStage;

    let (mut engine, _guard) = mock_engine();
    let mic = engine.create_virtual_input("Integration Stage Mic").expect("create mic");

    let add_result = engine
        .add_effect_stage(
            &mic.device_id,
            EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 0,
                eq_bass: 4,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            },
        )
        .expect("add_effect_stage");
    assert!(add_result.success);

    let reorder_result = engine
        .reorder_effect_stages(&mic.device_id, &["eq".to_string()])
        .expect("reorder_effect_stages should accept the only stage's id unchanged");
    assert!(reorder_result.success);

    let remove_result = engine.remove_effect_stage(&mic.device_id, "eq").expect("remove_effect_stage");
    assert!(remove_result.success);
}

#[test]
fn removing_effects_from_one_mic_does_not_disturb_an_unrelated_mics_live_chain() {
    // Regression for #229/#149, narrowed to virtual input (mic) devices —
    // the only kind that can host device-attached effects at all now that
    // VirtualRole::Bus (and output-direction device effects with it) is
    // retired (#293). Before the native-transport cutover, effect removal
    // restarted the single shared `filter-chain.service`, tearing down
    // every device's effect-hosted node at once; native transport's
    // per-device `unload_chain` makes that class of collateral damage
    // structurally impossible — this locks that in.
    use pipe_deck_lib::core::models::{EffectStage, PortDirection, ProcessingNodeSpecKind};

    let (mut engine, _guard) = mock_engine();
    let mic_a = engine.create_virtual_input("Mic A").expect("create mic a");
    let mic_b = engine.create_virtual_input("Mic B").expect("create mic b");
    let source = engine.create_virtual_output("Mic Feed Source").expect("create mic feed source");
    let mixer = engine.create_processing_node("Mic Mixer", ProcessingNodeSpecKind::Mixer).expect("create mixer");

    engine
        .connect_processing_node_port(&mixer.id, PortDirection::Input, &source.device_id)
        .expect("connect mixer input");
    engine
        .connect_processing_node_port(&mixer.id, PortDirection::Output, &mic_a.device_id)
        .expect("connect mixer output to mic a");
    engine.refresh_graph().unwrap();

    let eq_stage = |id: &str| EffectStage::Eq5Band {
        id: id.to_string(),
        eq_sub: 0,
        eq_bass: 4,
        eq_mid: 0,
        eq_treble: 0,
        eq_air: 0,
        output_gain: 0,
    };

    engine.add_effect_stage(&mic_a.device_id, eq_stage("mic-a-eq")).expect("add effects to mic a");
    engine.add_effect_stage(&mic_b.device_id, eq_stage("mic-b-eq")).expect("add effects to mic b");
    engine.refresh_graph().unwrap();
    assert!(engine.is_effect_chain_live(&mic_a.device_id), "mic a's chain should be live before touching mic b");

    engine.remove_effect_stage(&mic_b.device_id, "mic-b-eq").expect("remove effects from mic b");
    engine.refresh_graph().unwrap();

    assert!(
        engine.is_effect_chain_live(&mic_a.device_id),
        "removing effects from an unrelated mic must not disturb mic a's own live chain"
    );
    let mixer_after = engine.runtime_graph().processing_nodes.iter().find(|n| n.id == mixer.id).unwrap().clone();
    assert_eq!(
        mixer_after.outputs.first().and_then(|port| port.connected_id.clone()),
        Some(mic_a.device_id.clone()),
        "mic a's mixer feed must survive an unrelated mic's effect removal"
    );
}

/// Same isolated-config-dir setup as `mock_engine()`, but hands back a bare
/// `MockAudioBackend` + `ConfigStore` instead of a `CoreEngine` — the
/// `restore` module's functions take `&dyn AudioBackend` directly and are
/// never reached through `CoreEngine` in mock mode (it skips them itself,
/// since a fresh `MockAudioBackend` never has anything to adopt/orphan-clean
/// on startup).
fn mock_backend_with_config() -> (MockAudioBackend, ConfigStore, MutexGuard<'static, ()>) {
    let guard = lock_env();
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let config_dir = std::env::temp_dir().join(format!(
        "pipe-deck-mock-restore-test-config-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&config_dir);
    std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
    std::env::set_var("PIPE_DECK_USE_MOCK", "1");
    (MockAudioBackend::new(), ConfigStore::new(), guard)
}

fn virtual_device_spec(id: &str, slug: &str, direction: DeviceDirection) -> VirtualDeviceSpec {
    VirtualDeviceSpec {
        id: id.into(),
        slug: slug.into(),
        label: format!("Restore Test {slug}"),
        direction,
        created_at: "2026-07-21T00:00:00Z".into(),
        multi: false,
        mix_sources: Vec::new(),
    }
}

#[test]
fn restore_session_recreates_configured_virtual_devices_missing_from_the_backend() {
    let (backend, store, _guard) = mock_backend_with_config();
    store
        .add_virtual_device(virtual_device_spec("vdev-1", "restore-output", DeviceDirection::Output))
        .expect("save spec");

    let result = restore::restore_session(&backend).expect("restore_session");

    assert_eq!(result.created, vec!["pipe-deck-restore-output".to_string()]);
    assert!(result.adopted.is_empty());
    assert!(result.errors.is_empty());
    assert!(backend
        .list_virtual_devices()
        .iter()
        .any(|module| module.system_name == "pipe-deck-restore-output"));
}

#[test]
fn restore_session_adopts_a_device_the_backend_already_has_instead_of_recreating_it() {
    let (backend, store, _guard) = mock_backend_with_config();
    store
        .add_virtual_device(virtual_device_spec("vdev-1", "restore-output", DeviceDirection::Output))
        .expect("save spec");
    backend
        .restore_virtual_device("pipe-deck-restore-output", "Restore Test", DeviceDirection::Output, false, &[])
        .expect("pre-seed backend");

    let result = restore::restore_session(&backend).expect("restore_session");

    assert!(result.created.is_empty());
    assert_eq!(result.adopted, vec!["pipe-deck-restore-output".to_string()]);
    assert_eq!(
        backend
            .list_virtual_devices()
            .iter()
            .filter(|module| module.system_name == "pipe-deck-restore-output")
            .count(),
        1,
        "adopting an already-live device must not create a duplicate"
    );
}

#[test]
fn restore_session_removes_orphaned_modules_not_listed_in_config() {
    // `restore_session` treats an *empty* config plus existing modules as a
    // first-run migration (it adopts everything into config rather than
    // orphan-removing it — see the `config.virtual_devices.is_empty()`
    // branch), so this needs at least one real spec in config to avoid
    // tripping that path and exercise orphan removal instead.
    let (backend, store, _guard) = mock_backend_with_config();
    store
        .add_virtual_device(virtual_device_spec("vdev-1", "keep-me", DeviceDirection::Output))
        .expect("save spec");
    backend
        .restore_virtual_device("pipe-deck-keep-me", "Keep Me", DeviceDirection::Output, false, &[])
        .expect("pre-seed backend with the configured module");
    backend
        .restore_virtual_device("pipe-deck-orphan", "Orphan", DeviceDirection::Output, false, &[])
        .expect("pre-seed backend with an unconfigured module");

    let result = restore::restore_session(&backend).expect("restore_session");

    assert!(result.removed_orphans.contains(&"pipe-deck-orphan".to_string()));
    assert!(result.adopted.contains(&"pipe-deck-keep-me".to_string()));
    let system_names: Vec<_> = backend
        .list_virtual_devices()
        .into_iter()
        .map(|module| module.system_name)
        .collect();
    assert!(!system_names.contains(&"pipe-deck-orphan".to_string()));
    assert!(system_names.contains(&"pipe-deck-keep-me".to_string()));
}

#[test]
fn remove_all_virtual_devices_unloads_every_live_module_regardless_of_config() {
    // Unlike restore_session's orphan pass, this ignores config.yaml
    // entirely — a full teardown (package uninstall/purge) has no reason to
    // spare a device just because it's still listed there.
    let (backend, store, _guard) = mock_backend_with_config();
    store
        .add_virtual_device(virtual_device_spec("vdev-1", "keep-me", DeviceDirection::Output))
        .expect("save spec");
    backend
        .restore_virtual_device("pipe-deck-keep-me", "Keep Me", DeviceDirection::Output, false, &[])
        .expect("pre-seed configured module");
    backend
        .restore_virtual_device("pipe-deck-orphan", "Orphan", DeviceDirection::Output, false, &[])
        .expect("pre-seed unconfigured module");

    let (removed, errors) = restore::remove_all_virtual_devices(&backend);

    assert!(errors.is_empty());
    assert!(removed.contains(&"pipe-deck-keep-me".to_string()));
    assert!(removed.contains(&"pipe-deck-orphan".to_string()));
    assert!(backend.list_virtual_devices().is_empty());
}

#[test]
fn restore_profile_virtual_devices_recreates_devices_a_profile_depends_on() {
    let (backend, store, _guard) = mock_backend_with_config();
    store
        .add_virtual_device(virtual_device_spec("vdev-1", "profile-output", DeviceDirection::Output))
        .expect("save spec");

    let mut profile = Profile {
        version: 1,
        id: "gaming".into(),
        name: "Gaming".into(),
        created: "2026-07-21T00:00:00Z".into(),
        updated: "2026-07-21T00:00:00Z".into(),
        routing_intents: vec![],
        volume_state: Default::default(),
        device_assumptions: Default::default(),
        effect_state: Default::default(),
    };
    profile.device_assumptions.insert("vdev-1".into(), "pipe-deck-profile-output".into());

    let result = restore::restore_profile_virtual_devices(&backend, &profile).expect("restore_profile_virtual_devices");

    assert_eq!(result.created, vec!["pipe-deck-profile-output".to_string()]);
    assert!(backend
        .list_virtual_devices()
        .iter()
        .any(|module| module.system_name == "pipe-deck-profile-output"));
}

#[test]
fn engine_reinitializes_cleanly_against_a_fresh_backend_instance() {
    // Roughly simulates an app restart in mock mode: a brand new CoreEngine
    // (and therefore a brand new MockAudioBackend) must still produce a
    // usable graph without needing state from a previous instance.
    let (engine, _guard) = mock_engine();
    assert!(!engine.runtime_graph().devices.is_empty());
    assert!(!engine.runtime_graph().streams.is_empty());
    let _ = engine.simulate_rules(&std::collections::HashMap::new());
}

fn headset_device() -> Device {
    Device {
        id: "device-headset".into(),
        system_name: "headset-out".into(),
        label: "Headset".into(),
        kind: DeviceKind::Physical,
        direction: DeviceDirection::Output,
        sink_mode: None,
        volume_percent: Some(100),
        muted: Some(false),
        current_target: None,
        current_targets: Vec::new(),
        mix_sources: Vec::new(),
        sample_rate: None,
        channels: None,
    }
}

fn firefox_stream(id: &str) -> Stream {
    Stream {
        id: id.into(),
        app_name: "Firefox".into(),
        executable: Some("firefox".into()),
        window_class: None,
        system_name: None,
        direction: StreamDirection::Playback,
        current_target: None,
        media_name: None,
        is_system: false,
        volume_percent: None,
        muted: None,
        route_explanation: None,
        sample_rate: None,
        channels: None,
    }
}

fn firefox_rule() -> Rule {
    Rule {
        id: "firefox-to-headset".into(),
        name: "Firefox to headset".into(),
        enabled: true,
        priority: 10,
        conditions: vec![RuleCondition::Executable {
            value: "firefox".into(),
        }],
        action: RuleAction {
            target_system_name: Some("headset-out".into()),
            target_system_names: Vec::new(),
        },
        safeguards: Default::default(),
    }
}

/// Regression for issue #277 / #116: a routing rule for Firefox was silently
/// never applied to *any* Firefox stream once one had already been seen —
/// including a Firefox stream that already existed when the rule was added.
/// Firefox tears down/recreates its PipeWire node per tab while reporting
/// identical `app_name`/`executable`/`media_name` across tabs, so the old
/// "new stream" gate (keyed on that coarse identity) permanently marked all
/// future Firefox streams "already seen" after the first one — see
/// `CoreEngine::apply_rules_for_new_streams`.
#[test]
fn rule_added_after_a_stream_already_exists_is_applied_on_next_refresh() {
    let (mut engine, _guard) = mock_engine();

    let mut graph = RuntimeGraph {
        devices: vec![headset_device()],
        streams: vec![firefox_stream("node-1001")],
        links: Vec::new(),
        data_source: "mock".into(),
        notice: None,
        recent_stream_identities: Vec::new(),
        processing_nodes: Vec::new(),
    };
    engine.apply_graph_update(graph.clone());

    // No rule yet: the stream is observed and marked "seen" without a route.
    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == "node-1001").unwrap();
    assert_eq!(stream.current_target, None);

    // Now the user adds a matching rule — with the old identity-keyed seen
    // set, the still-live stream would never be reconsidered.
    engine.save_rule(firefox_rule()).expect("save rule");

    // Simulate the next graph refresh (same PipeWire node, still alive).
    graph.streams = vec![firefox_stream("node-1001")];
    engine.apply_graph_update(graph);

    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == "node-1001").unwrap();
    assert_eq!(stream.current_target.as_deref(), Some("device-headset"));
}

/// Companion regression: a Firefox tab closes (its PipeWire node/stream id
/// disappears) and a new tab opens (a *different* node id, identical
/// app_name/executable/media_name). The new stream must be independently
/// evaluated against the existing rule, not skipped as "already seen" just
/// because a same-identity stream was seen before.
#[test]
fn a_new_stream_instance_with_the_same_app_identity_is_still_auto_routed() {
    let (mut engine, _guard) = mock_engine();
    engine.save_rule(firefox_rule()).expect("save rule");

    let base_graph = RuntimeGraph {
        devices: vec![headset_device()],
        streams: vec![firefox_stream("node-1001")],
        links: Vec::new(),
        data_source: "mock".into(),
        notice: None,
        recent_stream_identities: Vec::new(),
        processing_nodes: Vec::new(),
    };
    engine.apply_graph_update(base_graph);
    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == "node-1001").unwrap();
    assert_eq!(stream.current_target.as_deref(), Some("device-headset"));

    // Tab closes: node-1001 disappears. A new tab opens: node-1002, same
    // app-level identity as node-1001.
    let next_graph = RuntimeGraph {
        devices: vec![headset_device()],
        streams: vec![firefox_stream("node-1002")],
        links: Vec::new(),
        data_source: "mock".into(),
        notice: None,
        recent_stream_identities: Vec::new(),
        processing_nodes: Vec::new(),
    };
    engine.apply_graph_update(next_graph);

    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == "node-1002").unwrap();
    assert_eq!(stream.current_target.as_deref(), Some("device-headset"));
}

#[test]
fn latency_ping_sums_hops_present_in_the_graph() {
    let (mut engine, _guard) = mock_engine();
    let device_id = engine.runtime_graph().devices[0].id.clone();
    let stream_id = engine.runtime_graph().streams[0].id.clone();

    let path = vec![
        LatencyPathNode { id: stream_id.clone(), system_name: None },
        LatencyPathNode { id: device_id.clone(), system_name: None },
    ];
    let result = engine.measure_latency_ping(&path).expect("mock backend should measure latency");

    assert_eq!(result.hops.len(), 2);
    assert!(result.hops.iter().all(|hop| hop.latency_ms.is_some()));
    let expected_total: f64 = result.hops.iter().map(|hop| hop.latency_ms.unwrap()).sum();
    assert_eq!(result.total_latency_ms, Some(expected_total));
    assert!(result.total_latency_ms.unwrap() > 0.0);
}

#[test]
fn latency_ping_reports_no_total_when_a_hop_has_no_data() {
    let (mut engine, _guard) = mock_engine();
    let device_id = engine.runtime_graph().devices[0].id.clone();

    let path = vec![
        LatencyPathNode { id: device_id, system_name: None },
        LatencyPathNode { id: "node-not-in-graph".to_string(), system_name: None },
    ];
    let result = engine.measure_latency_ping(&path).expect("mock backend should measure latency");

    assert_eq!(result.hops.len(), 2);
    assert!(result.hops[0].latency_ms.is_some());
    assert!(result.hops[1].latency_ms.is_none());
    assert_eq!(result.total_latency_ms, None);
}

#[test]
fn latency_ping_handles_an_empty_path() {
    let (engine, _guard) = mock_engine();

    let result = engine.measure_latency_ping(&[]).expect("mock backend should measure latency");

    assert!(result.hops.is_empty());
    assert_eq!(result.total_latency_ms, Some(0.0));
}
