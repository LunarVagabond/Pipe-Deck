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
    crate::paths::audit_log_path()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_path_uses_shared_state_policy_and_filename() {
        for values in [
            vec![("XDG_STATE_HOME", "/state")],
            vec![("HOME", "/home/u")],
            vec![],
        ] {
            let _env = crate::paths::tests::PathEnv::new(&values);
            assert_eq!(
                audit_log_path().as_os_str(),
                crate::paths::audit_log_path().as_os_str()
            );
            assert_eq!(audit_log_path().file_name().unwrap(), "plugin-audit.jsonl");
        }
    }
}
