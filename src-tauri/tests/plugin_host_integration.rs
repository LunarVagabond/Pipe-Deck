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
    let graph_read = metadata.iter().find(|c| c.id == "graph.read").unwrap();
    let profile_read = metadata.iter().find(|c| c.id == "profile.read").unwrap();
    assert!(graph_read.enforced);
    assert!(!profile_read.enforced);
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
