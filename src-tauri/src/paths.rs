//! Shared config, state and IPC path policy; Linux keeps its historical XDG fallbacks.
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(crate) enum Platform {
    Unix,
    Windows,
}

impl Platform {
    /// Single platform dispatch point for the path resolvers. Compile-time,
    /// so each limb's body is exercised by tests compiled for that target
    /// (scripts/tests/paths-windows-dispatch.test.sh drives the Windows limb
    /// on any host; `cargo test` drives the non-Windows limb).
    #[cfg(target_os = "windows")]
    pub(crate) fn current() -> Self {
        Self::Windows
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn current() -> Self {
        Self::Unix
    }

    pub(crate) fn join(self, base: PathBuf, suffix: &str) -> PathBuf {
        match self {
            Self::Unix => base.join(suffix),
            Self::Windows => {
                // Use the selected platform's separators even when tested on a Unix host.
                let mut path = base.into_os_string();
                if !path.is_empty() && !path.to_string_lossy().ends_with(['\\', '/']) {
                    path.push("\\");
                }
                path.push(suffix.replace('/', "\\"));
                PathBuf::from(path)
            }
        }
    }
}

fn windows_app_dir(platform: Platform) -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(|base| platform.join(PathBuf::from(base), "pipe-deck"))
}

pub fn config_dir() -> PathBuf {
    config_dir_for(Platform::current())
}

pub(crate) fn config_dir_for(platform: Platform) -> PathBuf {
    if let Ok(path) = std::env::var("PIPE_DECK_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    match platform {
        Platform::Windows => {
            windows_app_dir(platform).unwrap_or_else(|| PathBuf::from(".pipe-deck"))
        }
        Platform::Unix => {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                return PathBuf::from(xdg).join("pipe-deck");
            }
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".config/pipe-deck"))
                .unwrap_or_else(|_| PathBuf::from(".pipe-deck"))
        }
    }
}

pub fn state_dir() -> PathBuf {
    state_dir_for(Platform::current())
}

pub(crate) fn state_dir_for(platform: Platform) -> PathBuf {
    match platform {
        Platform::Windows => {
            windows_app_dir(platform).unwrap_or_else(|| PathBuf::from(".pipe-deck-state"))
        }
        Platform::Unix => {
            if let Ok(path) = std::env::var("XDG_STATE_HOME") {
                return PathBuf::from(path).join("pipe-deck");
            }
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".local/state/pipe-deck"))
                .unwrap_or_else(|_| PathBuf::from(".pipe-deck-state"))
        }
    }
}

pub fn user_systemd_dir() -> PathBuf {
    user_systemd_dir_for(Platform::current())
}

pub(crate) fn user_systemd_dir_for(platform: Platform) -> PathBuf {
    match platform {
        Platform::Windows => {
            windows_app_dir(platform).unwrap_or_else(|| PathBuf::from(".config/systemd/user"))
        }
        Platform::Unix => {
            if let Ok(path) = std::env::var("XDG_CONFIG_HOME") {
                return PathBuf::from(path).join("systemd/user");
            }
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".config/systemd/user"))
                .unwrap_or_else(|_| PathBuf::from(".config/systemd/user"))
        }
    }
}

pub fn runtime_dir() -> Option<PathBuf> {
    runtime_dir_for(Platform::current())
}

pub(crate) fn runtime_dir_for(platform: Platform) -> Option<PathBuf> {
    match platform {
        Platform::Unix => std::env::var("XDG_RUNTIME_DIR").ok().map(PathBuf::from),
        Platform::Windows => None,
    }
}

pub fn audit_log_path() -> PathBuf {
    audit_log_path_for(Platform::current())
}

pub(crate) fn audit_log_path_for(platform: Platform) -> PathBuf {
    platform.join(state_dir_for(platform), "plugin-audit.jsonl")
}

/// Serializes the path-env tests added here and legacy callers of the config-env lock.
#[cfg(test)]
pub(crate) fn lock_path_env() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::sync::MutexGuard;

    const ENV_KEYS: [&str; 8] = [
        "PIPE_DECK_CONFIG_DIR",
        "PIPE_DECK_NATIVE_HOST_SOCKET",
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
        "APPDATA",
        "LOCALAPPDATA",
    ];

    pub(crate) struct PathEnv {
        previous: Vec<Option<OsString>>,
        _guard: MutexGuard<'static, ()>,
    }

    impl PathEnv {
        pub(crate) fn new(values: &[(&str, &str)]) -> Self {
            let guard = lock_path_env();
            let previous = ENV_KEYS.iter().map(std::env::var_os).collect();
            for key in ENV_KEYS {
                std::env::remove_var(key);
            }
            for (key, value) in values {
                assert!(ENV_KEYS.contains(key));
                std::env::set_var(key, value);
            }
            Self {
                previous,
                _guard: guard,
            }
        }
    }

    impl Drop for PathEnv {
        fn drop(&mut self) {
            for (key, previous) in ENV_KEYS.iter().zip(&self.previous) {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn assert_path(actual: PathBuf, expected: &str) {
        assert_eq!(actual.as_os_str(), OsStr::new(expected));
    }

    #[cfg(unix)]
    #[test]
    fn public_wrappers_use_unix_environment() {
        let _env = PathEnv::new(&[
            ("XDG_CONFIG_HOME", "/x"),
            ("XDG_STATE_HOME", "/s"),
            ("HOME", "/h"),
        ]);
        assert_path(config_dir(), "/x/pipe-deck");
        assert_path(state_dir(), "/s/pipe-deck");
    }

    #[test]
    fn unix_path_matrix_preserves_exact_bytes() {
        // These byte-exact expectations assume Unix-host PathBuf::join syntax (Linux CI);
        // portable policy is exercised through the explicit-platform matrices.
        let cases: &[(&[(&str, &str)], [&str; 4], Option<&str>)] = &[
            (
                &[
                    ("XDG_CONFIG_HOME", "/config"),
                    ("XDG_STATE_HOME", "/state"),
                    ("XDG_RUNTIME_DIR", "/run/user/1000"),
                    ("HOME", "/home/u"),
                    ("APPDATA", r"C:\ignored"),
                ],
                [
                    "/config/pipe-deck",
                    "/state/pipe-deck",
                    "/config/systemd/user",
                    "/state/pipe-deck/plugin-audit.jsonl",
                ],
                Some("/run/user/1000"),
            ),
            (
                &[("HOME", "/home/u"), ("APPDATA", r"C:\ignored")],
                [
                    "/home/u/.config/pipe-deck",
                    "/home/u/.local/state/pipe-deck",
                    "/home/u/.config/systemd/user",
                    "/home/u/.local/state/pipe-deck/plugin-audit.jsonl",
                ],
                None,
            ),
            (
                &[],
                [
                    ".pipe-deck",
                    ".pipe-deck-state",
                    ".config/systemd/user",
                    ".pipe-deck-state/plugin-audit.jsonl",
                ],
                None,
            ),
            // Byte-identity preserves set-but-empty XDG values rather than filtering them.
            (
                &[
                    ("XDG_CONFIG_HOME", ""),
                    ("XDG_STATE_HOME", ""),
                    ("XDG_RUNTIME_DIR", ""),
                    ("HOME", "/home/ignored"),
                ],
                [
                    "pipe-deck",
                    "pipe-deck",
                    "systemd/user",
                    "pipe-deck/plugin-audit.jsonl",
                ],
                Some(""),
            ),
            (
                &[
                    ("XDG_CONFIG_HOME", "relative/config"),
                    ("XDG_STATE_HOME", "relative/state"),
                    ("XDG_RUNTIME_DIR", "relative/run"),
                    ("HOME", "/home/ignored"),
                ],
                [
                    "relative/config/pipe-deck",
                    "relative/state/pipe-deck",
                    "relative/config/systemd/user",
                    "relative/state/pipe-deck/plugin-audit.jsonl",
                ],
                Some("relative/run"),
            ),
            (
                &[("HOME", "")],
                [
                    ".config/pipe-deck",
                    ".local/state/pipe-deck",
                    ".config/systemd/user",
                    ".local/state/pipe-deck/plugin-audit.jsonl",
                ],
                None,
            ),
        ];
        for (values, expected, runtime) in cases {
            let _env = PathEnv::new(values);
            assert_path(config_dir_for(Platform::Unix), expected[0]);
            assert_path(state_dir_for(Platform::Unix), expected[1]);
            assert_path(user_systemd_dir_for(Platform::Unix), expected[2]);
            assert_path(audit_log_path_for(Platform::Unix), expected[3]);
            assert_eq!(
                runtime_dir_for(Platform::Unix)
                    .as_deref()
                    .map(|p| p.as_os_str()),
                runtime.map(OsStr::new)
            );
        }
    }

    #[test]
    fn windows_path_matrix_ignores_unix_environment() {
        let cases: &[(&[(&str, &str)], [&str; 4])] = &[
            (
                &[("APPDATA", "")],
                [
                    "pipe-deck",
                    "pipe-deck",
                    "pipe-deck",
                    r"pipe-deck\plugin-audit.jsonl",
                ],
            ),
            (
                &[("APPDATA", "C:\\Users\\u\\AppData\\Roaming\\")],
                [
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck\plugin-audit.jsonl",
                ],
            ),
            (
                &[("APPDATA", r"C:\Users\u\AppData\Roaming")],
                [
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck\plugin-audit.jsonl",
                ],
            ),
            (
                &[
                    ("APPDATA", r"C:\Users\u\AppData\Roaming"),
                    ("LOCALAPPDATA", r"C:\Users\u\AppData\Local"),
                    ("HOME", r"C:\ignored"),
                    ("XDG_CONFIG_HOME", "ignored/config"),
                    ("XDG_STATE_HOME", "ignored/state"),
                    ("XDG_RUNTIME_DIR", "ignored/run"),
                ],
                [
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck",
                    r"C:\Users\u\AppData\Roaming\pipe-deck\plugin-audit.jsonl",
                ],
            ),
            (
                &[
                    ("LOCALAPPDATA", r"C:\Users\u\AppData\Local"),
                    ("HOME", r"C:\ignored"),
                    ("XDG_CONFIG_HOME", "ignored/config"),
                    ("XDG_STATE_HOME", "ignored/state"),
                    ("XDG_RUNTIME_DIR", "ignored/run"),
                ],
                [
                    r"C:\Users\u\AppData\Local\pipe-deck",
                    r"C:\Users\u\AppData\Local\pipe-deck",
                    r"C:\Users\u\AppData\Local\pipe-deck",
                    r"C:\Users\u\AppData\Local\pipe-deck\plugin-audit.jsonl",
                ],
            ),
            (
                &[],
                [
                    ".pipe-deck",
                    ".pipe-deck-state",
                    ".config/systemd/user",
                    r".pipe-deck-state\plugin-audit.jsonl",
                ],
            ),
            (
                &[
                    ("HOME", r"C:\ignored"),
                    ("XDG_CONFIG_HOME", "ignored/config"),
                    ("XDG_STATE_HOME", "ignored/state"),
                    ("XDG_RUNTIME_DIR", "ignored/run"),
                ],
                [
                    ".pipe-deck",
                    ".pipe-deck-state",
                    ".config/systemd/user",
                    r".pipe-deck-state\plugin-audit.jsonl",
                ],
            ),
        ];
        for (values, expected) in cases {
            let _env = PathEnv::new(values);
            assert_path(config_dir_for(Platform::Windows), expected[0]);
            assert_path(state_dir_for(Platform::Windows), expected[1]);
            assert_path(user_systemd_dir_for(Platform::Windows), expected[2]);
            assert_path(audit_log_path_for(Platform::Windows), expected[3]);
            assert_eq!(runtime_dir_for(Platform::Windows), None);
        }
    }

    #[test]
    fn config_override_including_empty_takes_precedence() {
        for platform in [Platform::Unix, Platform::Windows] {
            for value in ["/override", "relative/override", ""] {
                let _env = PathEnv::new(&[
                    ("PIPE_DECK_CONFIG_DIR", value),
                    ("HOME", "/ignored"),
                    ("XDG_CONFIG_HOME", "/ignored"),
                    ("APPDATA", r"C:\ignored"),
                ]);
                assert_path(config_dir_for(platform), value);
            }
        }
    }
}
