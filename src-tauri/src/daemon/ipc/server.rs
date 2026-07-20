//! Daemon-side listener for native-effects IPC (issue #148). The only place
//! in the daemon binary that calls `pipewire::native_host` directly — the
//! GUI binary talks to this over the socket via `daemon::ipc::client`
//! instead of calling `native_host` in-process.

use super::protocol::{socket_path, IpcOkPayload, IpcOp, IpcRequest, IpcResponse, IpcResult};
use crate::pipewire::native_host;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;

/// Binds the real socket path (`protocol::socket_path()`) and serves forever.
pub fn run() -> std::io::Result<()> {
    run_at(&socket_path())
}

/// Binds `path` and serves requests until the process is killed. Blocking
/// accept loop — no graceful-shutdown signal handling: `Type=notify` still
/// stops via the default SIGTERM (immediate process exit), which is fine here
/// since there's no in-memory state to flush (native-effects recovery across
/// a daemon restart is deferred, see `reconcile_live_effects_state` in
/// `daemon::run`) and a stale socket file left behind is silently replaced by
/// this same function's own cleanup on the next daemon start. Split out from
/// `run()` so tests can bind a disposable temp path instead of the real
/// `$XDG_RUNTIME_DIR` socket.
pub fn run_at(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                thread::spawn(move || handle_connection(stream));
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

fn handle_connection(stream: UnixStream) {
    let Ok(reader_stream) = stream.try_clone() else { return };
    let reader = BufReader::new(reader_stream);
    let mut writer = stream;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<IpcRequest>(&line) else { continue };
        let result = dispatch(request.op);
        let response = IpcResponse { id: request.id, result };
        let Ok(encoded) = serde_json::to_string(&response) else { continue };
        if writeln!(writer, "{encoded}").is_err() {
            break;
        }
    }
}

fn dispatch(op: IpcOp) -> IpcResult {
    match op {
        IpcOp::Ping => IpcResult::Ok { payload: IpcOkPayload::Pong },
        IpcOp::LoadChain { device_system_name, is_input, config } => {
            match native_host::load_chain(&device_system_name, is_input, &config) {
                Ok(name) => IpcResult::Ok { payload: IpcOkPayload::PlaybackName { name } },
                Err(error) => IpcResult::Error { message: error.to_string() },
            }
        }
        IpcOp::UnloadChain { device_system_name } => match native_host::unload_chain(&device_system_name) {
            Ok(()) => IpcResult::Ok { payload: IpcOkPayload::Unit },
            Err(error) => IpcResult::Error { message: error.to_string() },
        },
        IpcOp::IsLoaded { device_system_name } => {
            IpcResult::Ok { payload: IpcOkPayload::Loaded { loaded: native_host::is_loaded(&device_system_name) } }
        }
    }
}
