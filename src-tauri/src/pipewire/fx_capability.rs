use serde::Serialize;
use std::path::Path;

/// What the installed PipeWire (and optional LADSPA plugins) can actually
/// back for live effects processing. Probed via static file/version checks
/// only — never by loading anything into the live PipeWire graph, since a
/// live load-and-unload probe would repeat the exact "automated live-graph
/// mutation" pattern that caused the incident this safety path exists for.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FxCapabilities {
    /// `bq_lowshelf` / `bq_peaking` / `bq_highshelf` / `param_eq` — part of
    /// PipeWire's builtin filter-chain plugin set since it was introduced.
    pub builtin_eq: bool,
    /// The `linear` builtin plugin (Mult/Add), used for a master trim stage.
    pub builtin_gain: bool,
    /// Verified against `man 7 libpipewire-module-filter-chain`: the builtin
    /// set is mixer/copy/bq_*/param_eq/convolver/delay/invert/clamp/linear/
    /// sine/ramp — there is no builtin dynamics (limiter/compressor/gate)
    /// plugin, so this is `false` on every currently known PipeWire version.
    pub builtin_limiter: bool,
    /// Path to a noise-suppression LADSPA plugin if one is installed
    /// (e.g. `librnnoise_ladspa`, the one PipeWire ships a worked example
    /// for at `/usr/share/pipewire/filter-chain/source-rnnoise.conf`).
    /// `None` means the noise-gate/suppression stage must stay disabled in
    /// the UI — there is nothing safe to back it with.
    pub ladspa_noise_gate: Option<String>,
}

const FILTER_CHAIN_MODULE_CANDIDATES: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu/pipewire-0.3/libpipewire-module-filter-chain.so",
    "/usr/lib/aarch64-linux-gnu/pipewire-0.3/libpipewire-module-filter-chain.so",
    "/usr/lib/pipewire-0.3/libpipewire-module-filter-chain.so",
    "/usr/lib64/pipewire-0.3/libpipewire-module-filter-chain.so",
];

const LADSPA_SEARCH_DIRS: &[&str] = &[
    "/usr/lib/ladspa",
    "/usr/lib/x86_64-linux-gnu/ladspa",
    "/usr/lib/aarch64-linux-gnu/ladspa",
    "/usr/lib64/ladspa",
];

/// The only noise-suppression plugin this codebase has a documented, tested
/// PipeWire integration path for. Intentionally narrow — see module doc.
const NOISE_GATE_LADSPA_CANDIDATES: &[&str] = &["librnnoise_ladspa.so"];

pub fn probe_capabilities() -> FxCapabilities {
    let module_present = filter_chain_module_present();
    FxCapabilities {
        builtin_eq: module_present,
        builtin_gain: module_present,
        builtin_limiter: false,
        ladspa_noise_gate: find_ladspa_plugin(NOISE_GATE_LADSPA_CANDIDATES),
    }
}

fn filter_chain_module_present() -> bool {
    FILTER_CHAIN_MODULE_CANDIDATES
        .iter()
        .any(|path| Path::new(path).is_file())
}

fn find_ladspa_plugin(candidates: &[&str]) -> Option<String> {
    let mut search_dirs: Vec<String> = LADSPA_SEARCH_DIRS.iter().map(|dir| dir.to_string()).collect();
    if let Ok(path_var) = std::env::var("LADSPA_PATH") {
        search_dirs.extend(std::env::split_paths(&path_var).map(|path| path.to_string_lossy().to_string()));
    }

    for dir in &search_dirs {
        for candidate in candidates {
            let full_path = Path::new(dir).join(candidate);
            if full_path.is_file() {
                return Some(full_path.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    // `LADSPA_PATH` is process-global; serialize tests that touch it so they
    // don't race under the default parallel test runner.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn find_ladspa_plugin_returns_none_when_absent_from_search_dirs() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("LADSPA_PATH");
        assert_eq!(
            find_ladspa_plugin(&["definitely_not_a_real_plugin.so"]),
            None
        );
    }

    #[test]
    fn find_ladspa_plugin_honors_ladspa_path_env_override() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!("pipe-deck-ladspa-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let plugin_path = dir.join("fake_gate.so");
        std::fs::write(&plugin_path, b"not a real plugin").unwrap();

        std::env::set_var("LADSPA_PATH", &dir);
        let found = find_ladspa_plugin(&["fake_gate.so"]);
        std::env::remove_var("LADSPA_PATH");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(found, Some(plugin_path.to_string_lossy().to_string()));
    }

    #[test]
    fn builtin_limiter_is_never_reported_as_available() {
        // Not probed dynamically on purpose: PipeWire's builtin filter-chain
        // plugin set has no dynamics processor as of this writing, so
        // claiming otherwise would let the UI enable a stage nothing backs.
        assert!(!probe_capabilities().builtin_limiter);
    }
}
