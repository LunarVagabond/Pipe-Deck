//! GUI-side client for native-effects IPC (issue #148). Used by
//! `backend::linux::live::LinuxPipeWireBackend` instead of calling
//! `pipewire::native_host` directly in-process — the native host lives in
//! the daemon process, reachable over `protocol::socket_path()`.

use super::protocol::{socket_path, IpcOkPayload, IpcOp, IpcRequest, IpcResponse, IpcResult};
use crate::core::models::EffectChainConfig;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;

/// Long enough for `load_chain`/`unload_chain`'s real PipeWire work; mirrors
/// `plugins/host.rs`'s existing `REQUEST_TIMEOUT` for the same reason (a
/// slow-but-not-hung daemon shouldn't look identical to an unreachable one).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Short enough that `effect_chain_capabilities()` never blocks the UI —
/// `ping` only needs to prove the daemon is alive and listening.
const PING_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Debug, Error)]
pub enum IpcClientError {
    #[error("native-effects daemon is unreachable: {0}")]
    Unreachable(String),
    #[error("native-effects daemon returned an error: {0}")]
    Remote(String),
    #[error("native-effects daemon sent an unexpected response")]
    Protocol,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Stateless — every call connects fresh rather than holding a long-lived
/// stream. This is the simplest thing that correctly handles "daemon
/// restarted mid-GUI-session": a stale connection to a dead socket would
/// just error on next use anyway, and a fresh `connect()` transparently picks
/// up whatever is listening right now. Call volume here is user-driven
/// (attach/detach/liveness-probe), not a hot path, so the reconnect cost is
/// a non-issue.
pub struct NativeHostClient;

impl NativeHostClient {
    /// Best-effort liveness probe. `false` on any connect/timeout/protocol
    /// error — callers (`effect_chain_capabilities()`) must silently fall
    /// back to restart-based transport rather than surfacing this as an
    /// error, since "no native daemon available" is an expected, common case
    /// (e.g. restore-on-login never enabled).
    pub fn ping() -> bool {
        matches!(request_with_timeout(IpcOp::Ping, PING_TIMEOUT), Ok(IpcOkPayload::Pong))
    }

    pub fn load_chain(device_system_name: &str, is_input: bool, config: &EffectChainConfig) -> Result<String, IpcClientError> {
        let op = IpcOp::LoadChain {
            device_system_name: device_system_name.to_string(),
            is_input,
            config: config.clone(),
        };
        match request_with_timeout(op, REQUEST_TIMEOUT)? {
            IpcOkPayload::PlaybackName { name } => Ok(name),
            _ => Err(IpcClientError::Protocol),
        }
    }

    pub fn unload_chain(device_system_name: &str) -> Result<(), IpcClientError> {
        let op = IpcOp::UnloadChain { device_system_name: device_system_name.to_string() };
        match request_with_timeout(op, REQUEST_TIMEOUT)? {
            IpcOkPayload::Unit => Ok(()),
            _ => Err(IpcClientError::Protocol),
        }
    }

    pub fn is_loaded(device_system_name: &str) -> bool {
        let op = IpcOp::IsLoaded { device_system_name: device_system_name.to_string() };
        matches!(request_with_timeout(op, REQUEST_TIMEOUT), Ok(IpcOkPayload::Loaded { loaded: true }))
    }
}

fn request_with_timeout(op: IpcOp, timeout: Duration) -> Result<IpcOkPayload, IpcClientError> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let mut stream = UnixStream::connect(socket_path()).map_err(|error| IpcClientError::Unreachable(error.to_string()))?;
    stream.set_read_timeout(Some(timeout)).map_err(|error| IpcClientError::Unreachable(error.to_string()))?;
    stream.set_write_timeout(Some(timeout)).map_err(|error| IpcClientError::Unreachable(error.to_string()))?;

    let request = IpcRequest { id, op };
    let encoded = serde_json::to_string(&request).map_err(|_| IpcClientError::Protocol)?;
    writeln!(stream, "{encoded}").map_err(|error| IpcClientError::Unreachable(error.to_string()))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|error| IpcClientError::Unreachable(error.to_string()))?;
    if line.is_empty() {
        return Err(IpcClientError::Unreachable("connection closed with no response".to_string()));
    }

    let response: IpcResponse = serde_json::from_str(line.trim_end()).map_err(|_| IpcClientError::Protocol)?;
    if response.id != id {
        return Err(IpcClientError::Protocol);
    }
    match response.result {
        IpcResult::Ok { payload } => Ok(payload),
        IpcResult::Error { message } => Err(IpcClientError::Remote(message)),
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d on purpose: hits a *real* PipeWire session, same
    //! convention as `core::engine::effects_ops::live_tests`. Only run via
    //! `cargo test --features native-effects --lib -- --ignored
    //! daemon_ipc_round_trips_load_chain_over_the_socket`, and only on a
    //! machine where that's safe. Exercises a disposable
    //! `pipe-deck-native-ipc-test` device name this test creates/destroys
    //! itself — never touches any device the user configured.
    use super::*;
    use crate::backend::linux::pactl;
    use crate::core::models::EffectStage;
    use crate::daemon::ipc::server;
    use std::path::PathBuf;
    use std::thread;

    #[test]
    #[ignore]
    fn daemon_ipc_round_trips_load_chain_over_the_socket() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let socket_path = PathBuf::from(format!("/tmp/pipe-deck-native-host-test-{}.sock", std::process::id()));
        std::env::set_var("PIPE_DECK_NATIVE_HOST_SOCKET", &socket_path);
        let cleanup_path = socket_path.clone();

        thread::spawn(move || {
            let _ = server::run_at(&socket_path);
        });
        // Give the listener a moment to bind before the client's first ping.
        let mut bound = false;
        for _ in 0..50 {
            if NativeHostClient::ping() {
                bound = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(bound, "server did not start accepting connections in time");

        let device_system_name = "pipe-deck-native-ipc-test";
        let cleanup = || {
            let _ = NativeHostClient::unload_chain(device_system_name);
            let _ = std::fs::remove_file(&cleanup_path);
        };

        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 6,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        if let Err(error) = NativeHostClient::load_chain(device_system_name, false, &config) {
            cleanup();
            panic!("load_chain over IPC failed: {error}");
        }

        let sink_live = pactl::sink_exists(device_system_name).unwrap_or(false);
        if !sink_live {
            cleanup();
            panic!("effects sink did not appear after load_chain over IPC");
        }
        if !NativeHostClient::is_loaded(device_system_name) {
            cleanup();
            panic!("daemon did not report the chain as loaded after load_chain over IPC");
        }

        let unload_result = NativeHostClient::unload_chain(device_system_name);
        let _ = std::fs::remove_file(&cleanup_path);
        unload_result.expect("unload_chain over IPC should succeed");
    }
}
