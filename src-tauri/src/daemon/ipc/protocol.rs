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
/// `XDG_STATE_HOME`/`$HOME` fallback chain.
///
/// `PIPE_DECK_NATIVE_HOST_SOCKET` overrides this for tests that need a
/// disposable socket path instead of the real one — mirrors
/// `PIPE_DECK_FILTER_CHAIN_CONF_DIR`'s role for `pipewire::filter_chain`.
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("PIPE_DECK_NATIVE_HOST_SOCKET") {
        return PathBuf::from(path);
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(dir).join("pipe-deck-native-host.sock");
    }
    super::super::state_dir().join("native-host.sock")
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

    fn response_roundtrips(result: IpcResult) {
        let response = IpcResponse { id: 3, result };
        let json = serde_json::to_string(&response).expect("serialize response");
        let decoded: IpcResponse = serde_json::from_str(&json).expect("deserialize response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn ok_pong_roundtrips() {
        response_roundtrips(IpcResult::Ok { payload: IpcOkPayload::Pong });
    }

    #[test]
    fn ok_playback_name_roundtrips() {
        response_roundtrips(IpcResult::Ok {
            payload: IpcOkPayload::PlaybackName { name: "effect_output.pipe-deck-game".to_string() },
        });
    }

    #[test]
    fn ok_unit_roundtrips() {
        response_roundtrips(IpcResult::Ok { payload: IpcOkPayload::Unit });
    }

    #[test]
    fn ok_loaded_roundtrips() {
        response_roundtrips(IpcResult::Ok { payload: IpcOkPayload::Loaded { loaded: true } });
    }

    #[test]
    fn error_roundtrips() {
        response_roundtrips(IpcResult::Error { message: "boom".to_string() });
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
