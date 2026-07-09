use chrono::Utc;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct AuditEntry<'a> {
    timestamp: String,
    plugin_id: &'a str,
    action: &'a str,
    result: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

pub fn audit_log_path() -> PathBuf {
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(path).join("pipe-deck/plugin-audit.jsonl");
    }
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".local/state/pipe-deck/plugin-audit.jsonl"))
        .unwrap_or_else(|_| PathBuf::from(".pipe-deck-state/plugin-audit.jsonl"))
}

pub fn log(plugin_id: &str, action: &str, result: &str, detail: Option<&str>) {
    let entry = AuditEntry {
        timestamp: Utc::now().to_rfc3339(),
        plugin_id,
        action,
        result,
        detail,
    };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    let path = audit_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}
