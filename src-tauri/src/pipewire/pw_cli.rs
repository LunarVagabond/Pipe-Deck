use crate::backend::BackendError;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long to wait for `pw-cli set-param` before giving up and killing it.
/// This call runs synchronously while a Tauri command holds `CoreEngine`'s
/// write lock (see `core/engine/effects_ops.rs::set_effect_chain_live_params`)
/// — an unbounded wait here would freeze every other command in the app
/// indefinitely if `pw-cli` ever hangs (observed happening for `set-param`
/// specifically, though not `info`/`ls`/`enum-params`, against a real
/// PipeWire 1.5.85 session — a `pw-cli` behavior, not something Pipe Deck
/// controls). 5s matches this codebase's other external-wait timeouts
/// (`daemon::ipc::client::REQUEST_TIMEOUT`, `filter_chain::wait_for_sink`).
const SET_PARAM_TIMEOUT: Duration = Duration::from_secs(5);

/// Live parameter updates for an already-running filter-chain node — the
/// "Live Params" half of the two-speed effects design (see
/// `core/engine/effects_ops.rs`). Never touches topology, never restarts
/// anything; safe to call at high frequency for a slider drag. Verified
/// live against a real PipeWire 1.5.85 session: `pw-cli set-param <id> Props
/// '{ "params": [ "<name>", <value>, ... ] }'` (the *array* form of the
/// struct — the object/dict form silently no-ops).
pub fn find_node_id_by_name(node_name: &str) -> Result<Option<u32>, BackendError> {
    let output = Command::new("pw-dump")
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run pw-dump: {error}")))?;
    if !output.status.success() {
        return Err(BackendError::Message(
            "pw-dump failed while looking up an effects node".to_string(),
        ));
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| BackendError::Message(format!("failed to parse pw-dump output: {error}")))?;

    let Some(objects) = parsed.as_array() else {
        return Ok(None);
    };

    for object in objects {
        if object.get("type").and_then(|v| v.as_str()) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let name = object
            .pointer("/info/props/node.name")
            .and_then(|v| v.as_str());
        if name == Some(node_name) {
            return Ok(object.get("id").and_then(|v| v.as_u64()).map(|id| id as u32));
        }
    }

    Ok(None)
}

/// Pushes `(control_name, value)` pairs to a live filter-chain node's `Props`
/// param in one call. `control_name` must match the internal filter-graph
/// node/control naming from `fx_validate::render_conf` (e.g. `"eq_bass:Gain"`).
pub fn set_params(node_id: u32, params: &[(String, f64)]) -> Result<(), BackendError> {
    if params.is_empty() {
        return Ok(());
    }

    let entries: Vec<String> = params
        .iter()
        .map(|(name, value)| format!("\"{name}\", {value}"))
        .collect();
    let param_json = format!(r#"{{ "params": [ {} ] }}"#, entries.join(", "));

    // stdout redirected to null rather than piped: we never read it in the
    // poll loop below, and pw-cli's own trace-style stdout output (seen live
    // — it echoes the parsed param object before doing anything else) could
    // otherwise fill the OS pipe buffer and deadlock the child against us.
    // stderr stays piped since we need it for the `Error` check below, and
    // pw-cli's error output is always short.
    let mut child = Command::new("pw-cli")
        .args(["set-param", &node_id.to_string(), "Props", &param_json])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| BackendError::Message(format!("failed to run pw-cli set-param: {error}")))?;

    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|error| BackendError::Message(format!("failed to poll pw-cli set-param: {error}")))?.is_some() {
            break;
        }
        if start.elapsed() > SET_PARAM_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BackendError::Message(format!(
                "pw-cli set-param did not respond within {SET_PARAM_TIMEOUT:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let mut stderr = String::new();
    if let Some(mut stderr_pipe) = child.stderr.take() {
        let _ = stderr_pipe.read_to_string(&mut stderr);
    }

    // `pw-cli` always exits 0 regardless of outcome (verified live) — it only
    // reports failure as an "Error: ..." line on stderr, so that's what we
    // have to check instead of the exit status.
    if stderr.contains("Error") {
        return Err(BackendError::Message(format!(
            "pw-cli set-param failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}
