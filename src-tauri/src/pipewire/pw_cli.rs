use crate::backend::BackendError;
use std::process::Command;

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

    let output = Command::new("pw-cli")
        .args(["set-param", &node_id.to_string(), "Props", &param_json])
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run pw-cli set-param: {error}")))?;

    // `pw-cli` always exits 0 regardless of outcome (verified live) — it only
    // reports failure as an "Error: ..." line on stderr, so that's what we
    // have to check instead of the exit status.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Error") {
        return Err(BackendError::Message(format!(
            "pw-cli set-param failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}
