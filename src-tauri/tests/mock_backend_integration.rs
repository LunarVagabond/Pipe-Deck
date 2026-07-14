//! End-to-end regression coverage for `CoreEngine` against `MockAudioBackend`.
//!
//! Before issue #68's `AudioBackend` refactor, none of this had automated
//! coverage — mixer/routing/virtual-device mutations against the mock data
//! source were only ever checked by hand via `PIPE_DECK_USE_MOCK=1 make dev`.
//! These tests exercise the same call paths `cargo test`-style so a future
//! change to the trait or its Linux/mock implementations gets a real signal
//! before it ships, not just a clean `cargo check`.

use pipe_deck_lib::core::engine::CoreEngine;
use pipe_deck_lib::core::models::{ConnectionEffectKind, DeviceDirection, DeviceKind, MixSource};

fn mock_engine() -> CoreEngine {
    std::env::set_var("PIPE_DECK_USE_MOCK", "1");
    let mut engine = CoreEngine::new();
    engine.refresh_graph().expect("initial refresh should succeed");
    engine
}

#[test]
fn mixer_mutations_persist_across_refresh() {
    let mut engine = mock_engine();
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
    let mut engine = mock_engine();
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

#[test]
fn device_routing_supports_multi_target_fanout() {
    let mut engine = mock_engine();
    let graph = engine.runtime_graph().clone();
    let source_id = graph.devices[3].id.clone();
    let targets = vec![graph.devices[1].id.clone(), graph.devices[2].id.clone()];

    let result = engine.set_device_targets(&source_id, &targets).unwrap();
    assert!(result.success, "{:?}", result.message);
    let current = engine.runtime_graph().devices.iter().find(|d| d.id == source_id).unwrap().current_targets.clone();
    assert_eq!(current, targets);
}

#[test]
fn virtual_device_create_remove_cycle_leaves_no_residue() {
    let mut engine = mock_engine();

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

#[test]
fn device_alias_rename_is_visible_after_refresh() {
    let mut engine = mock_engine();
    let output = engine.create_virtual_output("Original Label").expect("create output");

    engine.apply_device_alias(&output.system_name, "Renamed Label").unwrap();
    engine.refresh_graph().unwrap();

    let renamed = engine.runtime_graph().devices.iter().find(|d| d.id == output.device_id).unwrap();
    assert_eq!(renamed.label, "Renamed Label");
}

#[test]
fn virtual_mic_mix_add_and_volume_adjust() {
    let mut engine = mock_engine();
    let input = engine.create_virtual_input("Integration Mic").expect("create input");
    let physical_source = engine
        .runtime_graph()
        .devices
        .iter()
        .find(|d| d.kind == DeviceKind::Physical && d.direction == DeviceDirection::Input)
        .expect("sample graph should have a physical input")
        .id
        .clone();

    let result = engine
        .set_virtual_mic_mix(&input.device_id, &[MixSource {
            device_id: physical_source.clone(),
            volume_percent: 80,
            muted: false,
        }])
        .expect("set_virtual_mic_mix");
    assert!(result.success, "{:?}", result.message);

    let mic = engine.runtime_graph().devices.iter().find(|d| d.id == input.device_id).unwrap();
    assert_eq!(mic.mix_sources.len(), 1);
    assert_eq!(mic.mix_sources[0].device_id, physical_source);

    engine.set_mix_source_volume(&input.device_id, &physical_source, 55).expect("set_mix_source_volume");
    let mic = engine.runtime_graph().devices.iter().find(|d| d.id == input.device_id).unwrap();
    assert_eq!(mic.mix_sources[0].volume_percent, 55);

    engine.set_mix_source_mute(&input.device_id, &physical_source, true).expect("set_mix_source_mute");
    let mic = engine.runtime_graph().devices.iter().find(|d| d.id == input.device_id).unwrap();
    assert!(mic.mix_sources[0].muted);
}

#[test]
fn connection_effect_add_adjust_and_remove_round_trip() {
    let mut engine = mock_engine();
    // The sample graph's "stream-spotify -> sink-music" link is a stream
    // (app playback) routed to a virtual device — exercising the stream
    // insertion path, not just device-to-device.
    let link = engine
        .runtime_graph()
        .links
        .iter()
        .find(|link| link.id == "link-spotify-music")
        .expect("sample graph should have the spotify->music link")
        .clone();

    let result = engine
        .add_connection_effect(&link.source_id, &link.target_id)
        .expect("add_connection_effect");
    assert!(result.success, "{:?}", result.message);

    let updated_link = engine
        .runtime_graph()
        .links
        .iter()
        .find(|l| l.id == link.id)
        .unwrap();
    assert_eq!(
        updated_link.effects,
        vec![ConnectionEffectKind::Volume {
            volume_percent: 100,
            muted: false
        }]
    );

    engine
        .set_connection_volume(&link.source_id, &link.target_id, 40)
        .expect("set_connection_volume");
    let updated_link = engine
        .runtime_graph()
        .links
        .iter()
        .find(|l| l.id == link.id)
        .unwrap();
    assert_eq!(
        updated_link.effects,
        vec![ConnectionEffectKind::Volume {
            volume_percent: 40,
            muted: false
        }]
    );

    engine
        .set_connection_mute(&link.source_id, &link.target_id, true)
        .expect("set_connection_mute");
    let updated_link = engine
        .runtime_graph()
        .links
        .iter()
        .find(|l| l.id == link.id)
        .unwrap();
    assert_eq!(
        updated_link.effects,
        vec![ConnectionEffectKind::Volume {
            volume_percent: 40,
            muted: true
        }]
    );

    engine
        .remove_connection_effect(&link.source_id, &link.target_id)
        .expect("remove_connection_effect");
    let updated_link = engine
        .runtime_graph()
        .links
        .iter()
        .find(|l| l.id == link.id)
        .unwrap();
    assert!(updated_link.effects.is_empty());
}

#[test]
fn engine_reinitializes_cleanly_against_a_fresh_backend_instance() {
    // Roughly simulates an app restart in mock mode: a brand new CoreEngine
    // (and therefore a brand new MockAudioBackend) must still produce a
    // usable graph without needing state from a previous instance.
    let engine = mock_engine();
    assert!(!engine.runtime_graph().devices.is_empty());
    assert!(!engine.runtime_graph().streams.is_empty());
    let _ = engine.simulate_rules();
}
