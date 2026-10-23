//! Wire format for GUI<->daemon native-effects IPC (issue #148).
//! Line-delimited JSON, one full value per line, request/response correlated
//! by numeric `id` — mirrors `plugins/host.rs`'s existing framing convention
//! rather than introducing a dedicated IPC crate.

use crate::core::models::EffectChainConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// `$XDG_RUNTIME_DIR/pipe-deck-native-host.sock` when available (tmpfs,
/// user-scoped, auto-cleaned on logout) — falls back to
/// `daemon::state_dir()` otherwise, matching that function's own
/// `XDG_STATE_HOME`/`$HOME` fallback chain on Unix. Windows uses the app
/// state directory as a plain path; named-pipe transport is out of scope.
///
/// `PIPE_DECK_NATIVE_HOST_SOCKET` overrides this for tests that need a
/// disposable socket path instead of the real one — mirrors
/// `PIPE_DECK_FILTER_CHAIN_CONF_DIR`'s role for `pipewire::filter_chain`.
pub fn socket_path() -> PathBuf {
    socket_path_for(crate::paths::Platform::current())
}

fn socket_path_for(platform: crate::paths::Platform) -> PathBuf {
    if let Ok(path) = std::env::var("PIPE_DECK_NATIVE_HOST_SOCKET") {
        return PathBuf::from(path);
    }
    if let Some(dir) = crate::paths::runtime_dir_for(platform) {
        return platform.join(dir, "pipe-deck-native-host.sock");
    }
    platform.join(crate::paths::state_dir_for(platform), "native-host.sock")
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    #[serde(flatten)]
    pub op: IpcOp,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", content = "params", rename_all = "snake_case")]
pub enum IpcOp {
    Ping,
    LoadChain {
        device_system_name: String,
        is_input: bool,
        config: EffectChainConfig,
    },
    UnloadChain {
        device_system_name: String,
    },
    IsLoaded {
        device_system_name: String,
    },
    SetParam {
        device_system_name: String,
        /// `(control_name, value)` pairs — same naming/shape as
        /// `pipewire::fx_validate::live_params`'s output (e.g.
        /// `"eq_bass:Gain"`), pushed as a single `Props` param update.
        params: Vec<(String, f64)>,
    },
    /// Portable-DSP equivalent of `LoadChain` (issue #74) — routed to
    /// `pipewire::native_dsp_host` instead of `pipewire::native_host`.
    /// `is_input` selects the same naming template `LoadChain`'s own flag
    /// does (issue #86, PD-052) — `native_dsp_host` implements both now.
    LoadDspChain {
        device_system_name: String,
        is_input: bool,
        config: EffectChainConfig,
    },
    UnloadDspChain {
        device_system_name: String,
    },
    IsDspChainLoaded {
        device_system_name: String,
    },
    /// Portable-DSP equivalent of `SetParam` — takes the whole
    /// `EffectChainConfig` rather than flattened `(name, value)` pairs,
    /// since `native_dsp_host::set_live_chain` rebuilds the chain's biquad
    /// coefficients from it directly rather than pushing named PipeWire
    /// filter-chain controls.
    SetDspChainLiveParams {
        device_system_name: String,
        config: EffectChainConfig,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: u64,
    pub result: IpcResult,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IpcResult {
    Ok { payload: IpcOkPayload },
    Error { message: String },
}

/// Internally tagged rather than `#[serde(untagged)]`: an untagged `Pong`
/// and `Unit` both serialize as bare JSON `null` and are indistinguishable
/// on the wire, so an untagged enum here would always decode a `null`
/// response as whichever unit-like variant is listed first — silently
/// wrong, not a parse failure. Every variant carries an explicit `kind` tag.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcOkPayload {
    Pong,
    PlaybackName { name: String },
    Unit,
    Loaded { loaded: bool },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::EffectStage;

    #[test]
    fn socket_path_platform_and_environment_matrix() {
        use crate::paths::Platform::{Unix, Windows};
        type Case<'a> = (crate::paths::Platform, &'a [(&'a str, &'a str)], &'a str);
        let cases: &[Case<'_>] = &[
            (
                Unix,
                &[
                    ("XDG_RUNTIME_DIR", "/run/user/1000"),
                    ("XDG_STATE_HOME", "/state"),
                ],
                "/run/user/1000/pipe-deck-native-host.sock",
            ),
            (
                Unix,
                &[("XDG_RUNTIME_DIR", "")],
                "pipe-deck-native-host.sock",
            ),
            (
                Unix,
                &[("XDG_RUNTIME_DIR", "relative/run")],
                "relative/run/pipe-deck-native-host.sock",
            ),
            (
                Unix,
                &[("XDG_STATE_HOME", "/state")],
                "/state/pipe-deck/native-host.sock",
            ),
            (
                Unix,
                &[("XDG_STATE_HOME", "")],
                "pipe-deck/native-host.sock",
            ),
            (
                Unix,
                &[("XDG_STATE_HOME", "relative/state")],
                "relative/state/pipe-deck/native-host.sock",
            ),
            (
                Unix,
                &[("HOME", "/home/u")],
                "/home/u/.local/state/pipe-deck/native-host.sock",
            ),
            (Unix, &[], ".pipe-deck-state/native-host.sock"),
            (
                Windows,
                &[
                    ("APPDATA", r"C:\Users\u\AppData\Roaming"),
                    ("XDG_RUNTIME_DIR", "/ignored"),
                ],
                r"C:\Users\u\AppData\Roaming\pipe-deck\native-host.sock",
            ),
            (
                Windows,
                &[
                    ("LOCALAPPDATA", r"C:\Users\u\AppData\Local"),
                    ("HOME", r"C:\ignored"),
                ],
                r"C:\Users\u\AppData\Local\pipe-deck\native-host.sock",
            ),
            (Windows, &[], r".pipe-deck-state\native-host.sock"),
            (
                Windows,
                &[("HOME", r"C:\ignored"), ("XDG_STATE_HOME", "/ignored")],
                r".pipe-deck-state\native-host.sock",
            ),
        ];
        for (platform, values, expected) in cases {
            let _env = crate::paths::tests::PathEnv::new(values);
            assert_eq!(
                socket_path_for(*platform).as_os_str(),
                std::ffi::OsStr::new(expected)
            );
        }
    }

    #[test]
    fn socket_override_including_empty_takes_precedence() {
        for platform in [
            crate::paths::Platform::Unix,
            crate::paths::Platform::Windows,
        ] {
            for value in ["override.sock", ""] {
                let _env = crate::paths::tests::PathEnv::new(&[
                    ("PIPE_DECK_NATIVE_HOST_SOCKET", value),
                    ("XDG_RUNTIME_DIR", "/ignored"),
                    ("APPDATA", r"C:\ignored"),
                ]);
                assert_eq!(
                    socket_path_for(platform).as_os_str(),
                    std::ffi::OsStr::new(value)
                );
                assert_eq!(socket_path().as_os_str(), std::ffi::OsStr::new(value));
            }
        }
    }

    fn roundtrips(op: IpcOp) {
        let request = IpcRequest { id: 7, op };
        let json = serde_json::to_string(&request).expect("serialize request");
        let decoded: IpcRequest = serde_json::from_str(&json).expect("deserialize request");
        // Must decode back to the exact same variant/value, not just *some*
        // value that happens to re-serialize to the same JSON — two distinct
        // variants that both serialize to the same bytes (e.g. two unit
        // variants under `#[serde(untagged)]`) would pass a
        // re-serialization-only check while silently decoding as the wrong
        // one. This is the check that would have caught that class of bug.
        assert_eq!(decoded, request);
    }

    #[test]
    fn ping_roundtrips() {
        roundtrips(IpcOp::Ping);
    }

    #[test]
    fn load_chain_roundtrips() {
        roundtrips(IpcOp::LoadChain {
            device_system_name: "pipe-deck-game".to_string(),
            is_input: false,
            config: EffectChainConfig {
                stages: vec![EffectStage::Eq5Band {
                    id: "eq".to_string(),
                    eq_sub: 0,
                    eq_bass: 3,
                    eq_mid: 0,
                    eq_treble: 0,
                    eq_air: 0,
                    output_gain: 0,
                }],
                ..Default::default()
            },
        });
    }

    #[test]
    fn unload_chain_roundtrips() {
        roundtrips(IpcOp::UnloadChain {
            device_system_name: "pipe-deck-game".to_string(),
        });
    }

    #[test]
    fn is_loaded_roundtrips() {
        roundtrips(IpcOp::IsLoaded {
            device_system_name: "pipe-deck-mic".to_string(),
        });
    }

    #[test]
    fn set_param_roundtrips() {
        roundtrips(IpcOp::SetParam {
            device_system_name: "pipe-deck-proc-eq5band-test".to_string(),
            params: vec![("eq_bass:Gain".to_string(), 3.0)],
        });
    }

    fn eq5band_config() -> EffectChainConfig {
        EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 0,
                eq_bass: 3,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn load_dsp_chain_roundtrips() {
        roundtrips(IpcOp::LoadDspChain {
            device_system_name: "pipe-deck-mic".to_string(),
            is_input: true,
            config: eq5band_config(),
        });
    }

    #[test]
    fn unload_dsp_chain_roundtrips() {
        roundtrips(IpcOp::UnloadDspChain {
            device_system_name: "pipe-deck-mic".to_string(),
        });
    }

    #[test]
    fn is_dsp_chain_loaded_roundtrips() {
        roundtrips(IpcOp::IsDspChainLoaded {
            device_system_name: "pipe-deck-mic".to_string(),
        });
    }

    #[test]
    fn set_dsp_chain_live_params_roundtrips() {
        roundtrips(IpcOp::SetDspChainLiveParams {
            device_system_name: "pipe-deck-mic".to_string(),
            config: eq5band_config(),
        });
    }

    fn response_roundtrips(result: IpcResult) {
        let response = IpcResponse { id: 3, result };
        let json = serde_json::to_string(&response).expect("serialize response");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn ok_pong_roundtrips() {
        response_roundtrips(IpcResult::Ok {
            payload: IpcOkPayload::Pong,
        });
    }

    #[test]
    fn ok_playback_name_roundtrips() {
        response_roundtrips(IpcResult::Ok {
            payload: IpcOkPayload::PlaybackName {
                name: "effect_output.pipe-deck-game".to_string(),
            },
        });
    }

    #[test]
    fn ok_unit_roundtrips() {
        response_roundtrips(IpcResult::Ok {
            payload: IpcOkPayload::Unit,
        });
    }

    #[test]
    fn ok_loaded_roundtrips() {
        response_roundtrips(IpcResult::Ok {
            payload: IpcOkPayload::Loaded { loaded: true },
        });
    }

    #[test]
    fn error_roundtrips() {
        response_roundtrips(IpcResult::Error {
            message: "boom".to_string(),
        });
    }

    /// Regression: `Pong` and `Unit` are both zero-field variants — under
    /// `#[serde(untagged)]` they'd both serialize to bare `null` and be
    /// indistinguishable on the wire (an earlier version of this protocol
    /// had exactly this bug, caught by the real-session IPC test rather
    /// than by these unit tests, since the old re-serialization-only check
    /// couldn't see it). Confirms the two now produce different JSON.
    #[test]
    fn pong_and_unit_are_distinguishable_on_the_wire() {
        let pong_json = serde_json::to_string(&IpcOkPayload::Pong).unwrap();
        let unit_json = serde_json::to_string(&IpcOkPayload::Unit).unwrap();
        assert_ne!(pong_json, unit_json);
    }
}
