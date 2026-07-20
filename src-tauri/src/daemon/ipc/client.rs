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

    /// Spawns `server::run_at` on a background thread against a fresh temp
    /// socket, points `NativeHostClient` at it via
    /// `PIPE_DECK_NATIVE_HOST_SOCKET`, and blocks until the first successful
    /// `ping()` (or panics if it never comes up). Returns the socket path so
    /// the caller can remove it on cleanup. Shared setup for every test in
    /// this module — each test still gets its own socket path/thread, so
    /// they don't interfere with each other even if run in the same process.
    fn spawn_test_server() -> PathBuf {
        static NEXT_TEST_SOCKET_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let socket_path = PathBuf::from(format!(
            "/tmp/pipe-deck-native-host-test-{}-{}.sock",
            std::process::id(),
            NEXT_TEST_SOCKET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::env::set_var("PIPE_DECK_NATIVE_HOST_SOCKET", &socket_path);

        let server_socket_path = socket_path.clone();
        thread::spawn(move || {
            let _ = server::run_at(&server_socket_path);
        });
        let mut bound = false;
        for _ in 0..50 {
            if NativeHostClient::ping() {
                bound = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(bound, "server did not start accepting connections in time");
        socket_path
    }

    #[test]
    #[ignore]
    fn daemon_ipc_round_trips_load_chain_over_the_socket() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let cleanup_path = spawn_test_server();

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

    /// Current process RSS in KB, or `None` if `/proc/self/status` can't be
    /// read/parsed — same approach `examples/filter_chain_spike.rs` used.
    /// Valid here because the test server (`server::run_at`) runs on a
    /// background thread inside *this* test process, so `native_host`'s
    /// actual memory usage shows up in our own RSS.
    fn rss_kb() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        status.lines().find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|kb| kb.parse().ok())
        })
    }

    const SOAK_CYCLES: u32 = 50;
    /// RSS is sampled once here and treated as "warmup done" — the first
    /// several cycles reliably show a one-time jump (library/allocator
    /// warmup, confirmed in a real run: baseline 8.4MB -> 12.9MB by cycle
    /// 10, then flat), which would swamp a genuine but small per-cycle leak
    /// if measured from the very start. Growth is checked from here to the
    /// end instead, isolating the thing this test actually cares about.
    const WARMUP_CYCLES: u32 = 10;
    /// Tight, not generous: this only covers *post-warmup* growth over the
    /// remaining `SOAK_CYCLES - WARMUP_CYCLES` cycles, where a real run
    /// showed ~130kB total (~3kB/cycle, allocator noise) — a genuine
    /// per-cycle leak would blow through this cap long before 50 cycles.
    const MAX_ACCEPTABLE_POST_WARMUP_GROWTH_KB: u64 = 1024;

    /// Answers PD-027's open "does repeated load/unload genuinely never
    /// leak" question with a real assertion across many cycles, not the
    /// single manual cycle the day-1/day-2 work verified by hand. Checks two
    /// independent things every cycle: (1) state cleanliness — `is_loaded`
    /// and the pactl sink both go back to "not there" after every unload,
    /// completely independent of memory; (2) RSS growth stays under a
    /// generous cap across the whole run.
    #[test]
    #[ignore]
    fn native_host_soak_test_many_load_unload_cycles() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let cleanup_path = spawn_test_server();
        let device_system_name = "pipe-deck-native-ipc-soak-test";
        let cleanup = || {
            let _ = NativeHostClient::unload_chain(device_system_name);
            let _ = std::fs::remove_file(&cleanup_path);
        };

        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 3,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let rss_before = rss_kb();
        println!("soak test baseline RSS: {rss_before:?} kB");
        let mut rss_after_warmup = None;

        for cycle in 1..=SOAK_CYCLES {
            if let Err(error) = NativeHostClient::load_chain(device_system_name, false, &config) {
                cleanup();
                panic!("cycle {cycle}: load_chain failed: {error}");
            }
            if !pactl::sink_exists(device_system_name).unwrap_or(false) {
                cleanup();
                panic!("cycle {cycle}: effects sink did not appear after load_chain");
            }

            if let Err(error) = NativeHostClient::unload_chain(device_system_name) {
                cleanup();
                panic!("cycle {cycle}: unload_chain failed: {error}");
            }
            if NativeHostClient::is_loaded(device_system_name) {
                cleanup();
                panic!("cycle {cycle}: daemon still reports the chain as loaded after unload_chain");
            }
            if pactl::sink_exists(device_system_name).unwrap_or(false) {
                cleanup();
                panic!("cycle {cycle}: effects sink still present after unload_chain (leaked)");
            }

            if cycle == WARMUP_CYCLES {
                rss_after_warmup = rss_kb();
            }
            if cycle % 10 == 0 {
                println!("cycle {cycle}/{SOAK_CYCLES}: RSS = {:?} kB", rss_kb());
            }
        }

        let rss_final = rss_kb();
        println!("soak test RSS: baseline {rss_before:?} kB, post-warmup {rss_after_warmup:?} kB, final {rss_final:?} kB");
        let _ = std::fs::remove_file(&cleanup_path);

        if let (Some(after_warmup), Some(final_rss)) = (rss_after_warmup, rss_final) {
            let post_warmup_growth = final_rss.saturating_sub(after_warmup);
            assert!(
                post_warmup_growth <= MAX_ACCEPTABLE_POST_WARMUP_GROWTH_KB,
                "RSS grew {post_warmup_growth}kB over the {} cycles after warmup, exceeding the \
                 {MAX_ACCEPTABLE_POST_WARMUP_GROWTH_KB}kB cap — looks like a real per-cycle leak, not warmup",
                SOAK_CYCLES - WARMUP_CYCLES
            );
        }
    }
}
