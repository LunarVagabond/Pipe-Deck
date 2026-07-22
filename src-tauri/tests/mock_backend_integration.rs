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
    Device, DeviceDirection, DeviceKind, MixSource, Profile, Rule, RuleAction, RuleCondition,
    RuntimeGraph, Stream, StreamDirection, VirtualDeviceSpec,
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

#[test]
fn device_routing_supports_multi_target_fanout() {
    let (mut engine, _guard) = mock_engine();
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

#[test]
fn virtual_output_can_chain_into_another_virtual_output() {
    let (mut engine, _guard) = mock_engine();

    let submix = engine.create_virtual_output("Submix").expect("create submix");
    let master = engine.create_virtual_output("Master Mix").expect("create master mix");

    let result = engine.set_device_targets(&submix.device_id, std::slice::from_ref(&master.device_id)).unwrap();
    assert!(result.success, "{:?}", result.message);
    engine.refresh_graph().unwrap();

    let chained = engine
        .runtime_graph()
        .devices
        .iter()
        .find(|d| d.id == submix.device_id)
        .unwrap();
    assert_eq!(chained.current_targets, vec![master.device_id.clone()]);

    engine.remove_virtual_device(&submix.system_name).unwrap();
    engine.remove_virtual_device(&master.system_name).unwrap();
}

#[test]
fn removing_effects_from_a_bus_device_preserves_its_upstream_chain_link() {
    // Regression: source -> test1 -> test2(effects) -> hardware. Removing
    // effects from test2 destroys and recreates test2's sink node
    // (`revert_to_plain_device`), which silently drops the raw pw-link
    // test1's monitor held into it (PD-026 bus-into-bus). Only test2's own
    // downstream target (hardware) was being re-linked after removal;
    // test1 -> test2 needs the same treatment `apply_effect_chain_structural`
    // already gives this case.
    use pipe_deck_lib::core::models::EffectStage;

    let (mut engine, _guard) = mock_engine();
    let test1 = engine.create_virtual_output("test1").expect("create test1");
    let test2 = engine.create_virtual_output("test2").expect("create test2");

    engine.set_device_targets(&test1.device_id, std::slice::from_ref(&test2.device_id)).unwrap();
    engine.set_device_targets(&test2.device_id, &["sink-headphones".to_string()]).unwrap();
    engine.refresh_graph().unwrap();

    engine
        .add_effect_stage(
            &test2.device_id,
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
        .expect("add effects to test2");
    engine.refresh_graph().unwrap();

    engine.remove_effect_stage(&test2.device_id, "eq").expect("remove effects from test2");
    engine.refresh_graph().unwrap();

    let test1_after = engine.runtime_graph().devices.iter().find(|d| d.id == test1.device_id).unwrap().clone();
    let test2_after = engine.runtime_graph().devices.iter().find(|d| d.id == test2.device_id).unwrap().clone();
    assert_eq!(
        test1_after.current_targets,
        vec![test2.device_id.clone()],
        "test1 -> test2 link must survive removing effects from test2"
    );
    assert_eq!(test2_after.current_targets, vec!["sink-headphones".to_string()]);
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
fn virtual_mic_mix_add_and_volume_adjust() {
    let (mut engine, _guard) = mock_engine();
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
    let output = engine.create_virtual_output("Integration Remove Path Output").expect("create output");

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
        .apply_effect_chain_structural(&output.device_id, &config)
        .expect("structural apply should succeed");
    assert!(
        engine.is_effect_chain_live(&output.device_id),
        "chain should be live right after apply"
    );

    let result = engine
        .remove_effect_chain_structural(&output.device_id)
        .expect("remove_effect_chain_structural should succeed once the adapter calls all no-op successfully");
    assert!(result.success);
    assert!(
        !engine.is_effect_chain_live(&output.device_id),
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
    let output = engine.create_virtual_output("Integration Stage Output").expect("create output");

    let add_result = engine
        .add_effect_stage(
            &output.device_id,
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
        .reorder_effect_stages(&output.device_id, &["eq".to_string()])
        .expect("reorder_effect_stages should accept the only stage's id unchanged");
    assert!(reorder_result.success);

    let remove_result = engine.remove_effect_stage(&output.device_id, "eq").expect("remove_effect_stage");
    assert!(remove_result.success);
}

#[test]
fn touching_effects_on_one_device_does_not_disturb_another_devices_routing() {
    // Regression for the "mass re-routing" bug: restarting the shared
    // filter-chain.service (see `pipewire::pipewire_restart`) reloads every
    // device's effect chain at once, so `apply_effect_chain_structural`/
    // `remove_effect_chain_structural` must repair *other* active-chain
    // devices' links without touching devices that aren't affected at all.
    // Two virtual outputs, each routed to a different physical sink and each
    // carrying its own effect chain — adding/removing a stage on one must
    // leave the other's target exactly where it was.
    use pipe_deck_lib::core::models::EffectStage;

    let (mut engine, _guard) = mock_engine();
    let a = engine.create_virtual_output("Effects A").expect("create A");
    let b = engine.create_virtual_output("Effects B").expect("create B");

    engine.set_device_targets(&a.device_id, &["sink-headphones".to_string()]).unwrap();
    engine.set_device_targets(&b.device_id, &["sink-speakers".to_string()]).unwrap();
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

    engine.add_effect_stage(&b.device_id, eq_stage("b-eq")).expect("add effects to B");
    engine.refresh_graph().unwrap();

    // Touch A: add then remove a stage. Neither should perturb B's routing.
    engine.add_effect_stage(&a.device_id, eq_stage("a-eq")).expect("add effects to A");
    engine.refresh_graph().unwrap();
    let b_after_add = engine.runtime_graph().devices.iter().find(|d| d.id == b.device_id).unwrap().clone();
    assert_eq!(b_after_add.current_targets, vec!["sink-speakers".to_string()]);

    engine.remove_effect_stage(&a.device_id, "a-eq").expect("remove effects from A");
    engine.refresh_graph().unwrap();
    let b_after_remove = engine.runtime_graph().devices.iter().find(|d| d.id == b.device_id).unwrap().clone();
    assert_eq!(b_after_remove.current_targets, vec!["sink-speakers".to_string()]);

    let a_final = engine.runtime_graph().devices.iter().find(|d| d.id == a.device_id).unwrap().clone();
    assert_eq!(a_final.current_targets, vec!["sink-headphones".to_string()]);
}

#[test]
fn removing_effects_from_one_device_does_not_disturb_an_unrelated_input_devices_live_chain() {
    // Regression for #229/#149: before the native-transport cutover, effect
    // removal restarted the single shared `filter-chain.service`, tearing
    // down every device's effect-hosted node at once — #210's repair pass
    // (`relink_other_active_effect_chains`) only ever covered output-
    // direction devices, explicitly skipping `DeviceDirection::Input`, so an
    // input-direction (mic) device with its own live chain was never
    // repaired. Native transport's per-device `unload_chain` makes this
    // whole class of collateral damage structurally impossible — this locks
    // that in for the input-direction case specifically.
    use pipe_deck_lib::core::models::{EffectStage, MixSource};

    let (mut engine, _guard) = mock_engine();
    let output = engine.create_virtual_output("Unrelated Output").expect("create output");
    let mic = engine.create_virtual_input("Live Mic").expect("create mic");
    let mic_source = engine.create_virtual_output("Mic Feed Source").expect("create mic feed source");

    engine.set_device_targets(&output.device_id, &["sink-headphones".to_string()]).unwrap();
    engine
        .set_virtual_mic_mix(
            &mic.device_id,
            &[MixSource { device_id: mic_source.device_id.clone(), volume_percent: 100, muted: false }],
        )
        .expect("set up mic mix");
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

    engine.add_effect_stage(&mic.device_id, eq_stage("mic-eq")).expect("add effects to mic");
    engine.add_effect_stage(&output.device_id, eq_stage("output-eq")).expect("add effects to output");
    engine.refresh_graph().unwrap();
    assert!(engine.is_effect_chain_live(&mic.device_id), "mic chain should be live before touching output");

    engine.remove_effect_stage(&output.device_id, "output-eq").expect("remove effects from output");
    engine.refresh_graph().unwrap();

    assert!(
        engine.is_effect_chain_live(&mic.device_id),
        "removing effects from an unrelated output device must not disturb the mic's own live chain"
    );
    let mic_after = engine.runtime_graph().devices.iter().find(|d| d.id == mic.device_id).unwrap().clone();
    assert_eq!(
        mic_after.mix_sources.len(),
        1,
        "mic's mix-source feed must survive an unrelated device's effect removal"
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
    let _ = engine.simulate_rules();
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
    };
    engine.apply_graph_update(next_graph);

    let stream = engine.runtime_graph().streams.iter().find(|s| s.id == "node-1002").unwrap();
    assert_eq!(stream.current_target.as_deref(), Some("device-headset"));
}
