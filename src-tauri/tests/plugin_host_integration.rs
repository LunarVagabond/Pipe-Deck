//! Integration coverage for the plugin host lifecycle (`plugins::host`/`plugins::registry`),
//! which previously had no coverage beyond small unit tests in `capabilities.rs`/`manifest.rs`
//! (see #121). Spawns real (tiny Python) subprocess fixtures rather than mocking the transport,
//! since the thing under test is the JSON-RPC-over-stdio handshake itself.
//!
//! Mutates global process env (`PIPE_DECK_CONFIG_DIR`, `PIPE_DECK_BUNDLED_PLUGINS`,
//! `PIPE_DECK_USE_MOCK`) like the rest of the plugin/config suite — run with
//! `--test-threads=1` if this file flakes when run alongside others (see CLAUDE.md).

use pipe_deck_lib::core::engine::CoreEngine;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "pipe-deck-plugin-test-{label}-{}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const OK_PLUGIN_SCRIPT: &str = r#"#!/usr/bin/env python3
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    method = message.get("method")
    msg_id = message.get("id")
    if method == "initialize" and msg_id is not None:
        send({"jsonrpc": "2.0", "id": msg_id, "result": {"plugin_version": "0.1.0"}})
        continue
    if method == "shutdown":
        if msg_id is not None:
            send({"jsonrpc": "2.0", "id": msg_id, "result": {"status": "ok"}})
        break
"#;

const SUGGESTING_PLUGIN_SCRIPT: &str = r#"#!/usr/bin/env python3
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    method = message.get("method")
    msg_id = message.get("id")
    if method == "initialize" and msg_id is not None:
        send({"jsonrpc": "2.0", "id": msg_id, "result": {"plugin_version": "0.1.0"}})
        send({
            "jsonrpc": "2.0",
            "method": "routing.suggest",
            "params": {
                "stream_id": "stream-1",
                "target_system_name": "pipe-deck-game-mix",
                "reason": "test suggestion",
            },
        })
        continue
    if method == "shutdown":
        if msg_id is not None:
            send({"jsonrpc": "2.0", "id": msg_id, "result": {"status": "ok"}})
        break
"#;

const EFFECTS_APPLYING_PLUGIN_SCRIPT: &str = r#"#!/usr/bin/env python3
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

with open("target-device-id.txt") as f:
    device_id = f.read().strip()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    method = message.get("method")
    msg_id = message.get("id")
    if method == "initialize" and msg_id is not None:
        send({"jsonrpc": "2.0", "id": msg_id, "result": {"plugin_version": "0.1.0"}})
        send({
            "jsonrpc": "2.0",
            "method": "effects.apply",
            "params": {
                "device_id": device_id,
                "config": {"eq_bass": 6, "output_gain": -3},
            },
        })
        continue
    if method == "shutdown":
        if msg_id is not None:
            send({"jsonrpc": "2.0", "id": msg_id, "result": {"status": "ok"}})
        break
"#;

const PROFILE_AWARE_PLUGIN_SCRIPT: &str = r#"#!/usr/bin/env python3
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    method = message.get("method")
    msg_id = message.get("id")
    if method == "initialize" and msg_id is not None:
        send({"jsonrpc": "2.0", "id": msg_id, "result": {"plugin_version": "0.1.0"}})
        continue
    if method == "profile.updated":
        with open("received-profile.json", "w") as f:
            json.dump(message.get("params", {}), f)
        continue
    if method == "shutdown":
        if msg_id is not None:
            send({"jsonrpc": "2.0", "id": msg_id, "result": {"status": "ok"}})
        break
"#;

const FAILING_PLUGIN_SCRIPT: &str = r#"#!/usr/bin/env python3
import json, sys

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    method = message.get("method")
    msg_id = message.get("id")
    if method == "initialize" and msg_id is not None:
        sys.stderr.write("boom: simulated plugin failure\n")
        sys.stderr.flush()
        send({"jsonrpc": "2.0", "id": msg_id, "error": {"message": "simulated failure"}})
        continue
    if method == "shutdown":
        if msg_id is not None:
            send({"jsonrpc": "2.0", "id": msg_id, "result": {"status": "ok"}})
        break
"#;

fn write_plugin(bundled_dir: &Path, id: &str, capabilities: &[&str], script: &str) {
    let root = bundled_dir.join(id);
    fs::create_dir_all(root.join("bin")).unwrap();
    let script_path = root.join("bin/plugin.py");
    fs::write(&script_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let capabilities_yaml = if capabilities.is_empty() {
        String::new()
    } else {
        let lines: Vec<String> = capabilities.iter().map(|c| format!("  - {c}")).collect();
        format!("capabilities:\n{}\n", lines.join("\n"))
    };
    let manifest = format!(
        "id: {id}\nname: {id}\nversion: 0.1.0\napi_version: 1\nentry: bin/plugin.py\nbundled: true\n{capabilities_yaml}"
    );
    fs::write(root.join("plugin.yaml"), manifest).unwrap();
}

fn write_broken_plugin(bundled_dir: &Path, id: &str) {
    let root = bundled_dir.join(id);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("plugin.yaml"), "not: [valid yaml for a manifest").unwrap();
}

/// Sets up isolated `PIPE_DECK_CONFIG_DIR`/`PIPE_DECK_BUNDLED_PLUGINS` env vars for one
/// test and returns a fresh, refreshed `CoreEngine`. Each test gets its own temp config
/// dir and bundled-plugins dir, so tests don't see each other's plugin state even though
/// the env vars themselves are global process state.
fn isolated_engine(bundled_dir: &Path) -> CoreEngine {
    let config_dir = unique_temp_dir("config");
    std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
    std::env::set_var("PIPE_DECK_BUNDLED_PLUGINS", bundled_dir);
    std::env::set_var("PIPE_DECK_USE_MOCK", "1");
    let mut engine = CoreEngine::new();
    engine.refresh_graph().expect("initial refresh should succeed");
    engine
}

#[test]
fn discover_surfaces_malformed_manifest_as_a_discovery_error() {
    let bundled_dir = unique_temp_dir("bundled-discover");
    write_plugin(&bundled_dir, "echo", &["graph.read"], OK_PLUGIN_SCRIPT);
    write_broken_plugin(&bundled_dir, "broken");

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    let issues = engine.plugin_discovery_errors();
    assert_eq!(issues.len(), 1, "expected exactly one discovery issue, got {issues:?}");
    assert!(issues[0].path.ends_with("broken"));

    let plugins = engine.list_plugins();
    assert!(plugins.iter().any(|p| p.id == "echo"));
    assert!(!plugins.iter().any(|p| p.id == "broken"));
}

#[test]
fn plugin_lifecycle_start_and_stop_via_enable_toggle() {
    let bundled_dir = unique_temp_dir("bundled-lifecycle");
    write_plugin(&bundled_dir, "echo", &["graph.read"], OK_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    let plugins = engine.list_plugins();
    let echo = plugins.iter().find(|p| p.id == "echo").expect("echo plugin discovered");
    assert!(echo.enabled, "bundled plugin should be auto-enabled");
    assert_eq!(echo.runtime_status, pipe_deck_lib::core::models::PluginRuntimeStatus::Running);

    engine.set_plugin_enabled("echo", false).unwrap();
    let plugins = engine.list_plugins();
    let echo = plugins.iter().find(|p| p.id == "echo").unwrap();
    assert!(!echo.enabled);
    assert_eq!(echo.runtime_status, pipe_deck_lib::core::models::PluginRuntimeStatus::Stopped);

    engine.set_plugin_enabled("echo", true).unwrap();
    let plugins = engine.list_plugins();
    let echo = plugins.iter().find(|p| p.id == "echo").unwrap();
    assert!(echo.enabled);
    assert_eq!(echo.runtime_status, pipe_deck_lib::core::models::PluginRuntimeStatus::Running);
}

#[test]
fn rescan_discovers_a_newly_added_plugin_without_a_restart() {
    let bundled_dir = unique_temp_dir("bundled-rescan-add");
    write_plugin(&bundled_dir, "echo", &["graph.read"], OK_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();
    assert!(!engine.list_plugins().iter().any(|p| p.id == "second"));

    write_plugin(&bundled_dir, "second", &["graph.read"], OK_PLUGIN_SCRIPT);
    engine.rescan_plugins().unwrap();

    let second = engine.list_plugins().into_iter().find(|p| p.id == "second");
    let second = second.expect("newly-added plugin should be discovered by rescan");
    assert_eq!(second.runtime_status, pipe_deck_lib::core::models::PluginRuntimeStatus::Running);
}

#[test]
fn rescan_stops_an_orphaned_plugin_whose_directory_was_removed() {
    let bundled_dir = unique_temp_dir("bundled-rescan-remove");
    write_plugin(&bundled_dir, "echo", &["graph.read"], OK_PLUGIN_SCRIPT);
    write_plugin(&bundled_dir, "temp", &["graph.read"], OK_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();
    assert!(engine.list_plugins().iter().any(|p| p.id == "temp"));

    fs::remove_dir_all(bundled_dir.join("temp")).unwrap();
    engine.rescan_plugins().unwrap();

    let plugins = engine.list_plugins();
    assert!(!plugins.iter().any(|p| p.id == "temp"));
    assert!(plugins.iter().any(|p| p.id == "echo"));
}

#[test]
fn routing_suggest_capability_captures_plugin_suggestions() {
    let bundled_dir = unique_temp_dir("bundled-routing-suggest");
    write_plugin(&bundled_dir, "suggester", &["routing.suggest"], SUGGESTING_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();
    // Let the async stdout-reader thread catch up before draining (see #118's stderr
    // grace period for the same class of race, just on the stdout side here).
    std::thread::sleep(std::time::Duration::from_millis(30));
    engine.refresh_graph().expect("refresh should succeed and drain queued notifications");

    let suggestions = engine.plugin_routing_suggestions();
    assert_eq!(suggestions.len(), 1, "expected exactly one captured suggestion: {suggestions:?}");
    assert_eq!(suggestions[0].plugin_id, "suggester");
    assert_eq!(suggestions[0].stream_id, "stream-1");
    assert_eq!(suggestions[0].target_system_name, "pipe-deck-game-mix");
    assert_eq!(suggestions[0].reason.as_deref(), Some("test suggestion"));
}

#[test]
fn profile_read_capability_receives_active_profile_metadata_on_start() {
    let bundled_dir = unique_temp_dir("bundled-profile-read");
    write_plugin(&bundled_dir, "profile-aware", &["profile.read"], PROFILE_AWARE_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    let config_dir = pipe_deck_lib::config::ConfigStore::new().config_dir().clone();
    pipe_deck_lib::config::profile_store::ProfileStore::new(config_dir)
        .ensure_default_profile()
        .expect("should be able to write the default profile fixture");

    engine.initialize_plugins();

    let received_path = bundled_dir.join("profile-aware").join("received-profile.json");
    let mut content = String::new();
    for _ in 0..50 {
        if let Ok(text) = fs::read_to_string(&received_path) {
            content = text;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!content.is_empty(), "plugin never received a profile.updated notification");
    assert!(content.contains("default"), "expected the default profile id in: {content}");
}

#[test]
fn effects_manage_capability_applies_a_queued_request_to_a_pipe_deck_device() {
    let bundled_dir = unique_temp_dir("bundled-effects-manage");
    let mut engine = isolated_engine(&bundled_dir);

    let virtual_device = engine
        .create_virtual_output("EffectsTest")
        .expect("mock backend should support creating a virtual output");

    write_plugin(&bundled_dir, "effector", &["effects.manage"], EFFECTS_APPLYING_PLUGIN_SCRIPT);
    fs::write(
        bundled_dir.join("effector").join("target-device-id.txt"),
        &virtual_device.device_id,
    )
    .unwrap();

    engine.initialize_plugins();
    std::thread::sleep(std::time::Duration::from_millis(30));
    engine.refresh_graph().expect("refresh should apply the queued effects.apply request");

    let chains = engine.get_effect_chains().expect("effect chains should be readable");
    let applied = chains
        .get(&virtual_device.device_id)
        .expect("effects.apply request should have been applied to the target device");
    assert_eq!(applied.eq_bass, 6);
    assert_eq!(applied.output_gain, -3);
}

#[test]
fn grant_plugin_capabilities_reflects_granted_vs_requested_and_enforced_flag() {
    let bundled_dir = unique_temp_dir("bundled-capabilities");
    write_plugin(&bundled_dir, "echo", &["graph.read", "profile.read"], OK_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    let echo = engine.list_plugins().into_iter().find(|p| p.id == "echo").unwrap();
    assert_eq!(echo.requested_capabilities, vec!["graph.read", "profile.read"]);
    assert_eq!(echo.granted_capabilities, vec!["graph.read", "profile.read"]);

    engine.grant_plugin_capabilities("echo", vec!["graph.read".to_string()]).unwrap();
    let echo = engine.list_plugins().into_iter().find(|p| p.id == "echo").unwrap();
    assert_eq!(echo.granted_capabilities, vec!["graph.read"]);
    assert_eq!(echo.requested_capabilities, vec!["graph.read", "profile.read"]);

    let metadata = engine.plugin_capability_metadata();
    assert!(metadata.iter().all(|info| info.enforced), "all v1 capabilities should be enforced: {metadata:?}");
}

#[test]
fn plugin_initialize_failure_surfaces_stderr_tail_in_last_error() {
    let bundled_dir = unique_temp_dir("bundled-failing");
    write_plugin(&bundled_dir, "failing", &[], FAILING_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    let failing = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    assert_eq!(failing.runtime_status, pipe_deck_lib::core::models::PluginRuntimeStatus::Error);
    let last_error = failing.last_error.expect("expected a last_error to be recorded");
    assert!(last_error.contains("simulated failure"), "last_error was: {last_error}");
    assert!(last_error.contains("boom: simulated plugin failure"), "last_error was: {last_error}");
}

#[test]
fn repeated_rescan_of_a_crashing_plugin_backs_off_instead_of_respawning_immediately(
) {
    let bundled_dir = unique_temp_dir("bundled-crash-backoff");
    write_plugin(&bundled_dir, "failing", &[], FAILING_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    let before = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    let first_error = before.last_error.expect("first failed attempt should record last_error");

    // Rescanning again immediately should hit the backoff window and skip respawning
    // entirely — `last_errors` (and thus the surfaced message) is untouched by a
    // skipped attempt, so it stays exactly what the first crash produced.
    engine.rescan_plugins().unwrap();
    let after = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    assert_eq!(after.last_error.as_deref(), Some(first_error.as_str()));
    assert!(after.disabled_reason.is_none(), "should not be disabled after a single crash");
}

#[test]
fn plugin_is_disabled_after_max_consecutive_crashes() {
    let bundled_dir = unique_temp_dir("bundled-crash-disable");
    write_plugin(&bundled_dir, "failing", &[], FAILING_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();

    // First rescan attempt already happened via initialize_plugins (1 failure). Rescan
    // enough more times, sleeping past each short backoff window, to cross the
    // MAX_CONSECUTIVE_FAILURES threshold.
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        engine.rescan_plugins().unwrap();
    }

    let failing = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    let disabled_reason = failing
        .disabled_reason
        .expect("plugin should be disabled after repeated consecutive crashes");
    assert!(disabled_reason.contains("consecutive crashes"), "reason was: {disabled_reason}");
}

#[test]
fn re_enabling_a_disabled_plugin_clears_crash_state_and_retries_immediately() {
    let bundled_dir = unique_temp_dir("bundled-crash-reenable");
    write_plugin(&bundled_dir, "failing", &[], FAILING_PLUGIN_SCRIPT);

    let mut engine = isolated_engine(&bundled_dir);
    engine.initialize_plugins();
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_secs(1));
        engine.rescan_plugins().unwrap();
    }
    let failing = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    assert!(failing.disabled_reason.is_some(), "expected plugin to be disabled going into this test");

    // A user explicitly toggling the plugin off then back on is a deliberate retry and
    // should not be blocked by leftover crash-loop state.
    engine.set_plugin_enabled("failing", false).unwrap();
    let result = engine.set_plugin_enabled("failing", true);
    assert!(result.is_err(), "the plugin still crashes on init, so re-enabling should still fail");

    let failing = engine.list_plugins().into_iter().find(|p| p.id == "failing").unwrap();
    assert_eq!(
        failing.disabled_reason, None,
        "re-enabling should reset crash-loop state even though the retry itself failed"
    );
}
