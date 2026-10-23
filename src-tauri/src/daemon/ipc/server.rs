//! Daemon-side listener for native-effects IPC (issue #148). The only place
//! in the daemon binary that calls `pipewire::native_host` directly — the
//! GUI binary talks to this over the socket via `daemon::ipc::client`
//! instead of calling `native_host` in-process.

use super::protocol::{socket_path, IpcOkPayload, IpcOp, IpcRequest, IpcResponse, IpcResult};
use crate::pipewire::{native_dsp_host, native_host};
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
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let reader = BufReader::new(reader_stream);
    let mut writer = stream;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(request) = serde_json::from_str::<IpcRequest>(&line) else {
            continue;
        };
        let result = dispatch(request.op);
        let response = IpcResponse {
            id: request.id,
            result,
        };
        let Ok(encoded) = serde_json::to_string(&response) else {
            continue;
        };
        if writeln!(writer, "{encoded}").is_err() {
            break;
        }
    }
}

fn dispatch(op: IpcOp) -> IpcResult {
    match op {
        IpcOp::Ping => IpcResult::Ok {
            payload: IpcOkPayload::Pong,
        },
        IpcOp::LoadChain {
            device_system_name,
            is_input,
            config,
        } => match native_host::load_chain(&device_system_name, is_input, &config) {
            Ok(name) => IpcResult::Ok {
                payload: IpcOkPayload::PlaybackName { name },
            },
            Err(error) => IpcResult::Error {
                message: error.to_string(),
            },
        },
        IpcOp::UnloadChain { device_system_name } => {
            match native_host::unload_chain(&device_system_name) {
                Ok(()) => IpcResult::Ok {
                    payload: IpcOkPayload::Unit,
                },
                Err(error) => IpcResult::Error {
                    message: error.to_string(),
                },
            }
        }
        IpcOp::IsLoaded { device_system_name } => IpcResult::Ok {
            payload: IpcOkPayload::Loaded {
                loaded: native_host::is_loaded(&device_system_name),
            },
        },
        IpcOp::SetParam {
            device_system_name,
            params,
        } => match native_host::set_param(&device_system_name, &params) {
            Ok(()) => IpcResult::Ok {
                payload: IpcOkPayload::Unit,
            },
            Err(error) => IpcResult::Error {
                message: error.to_string(),
            },
        },
        IpcOp::LoadDspChain {
            device_system_name,
            is_input,
            config,
        } => match native_dsp_host::load_chain(&device_system_name, is_input, &config) {
            Ok(name) => IpcResult::Ok {
                payload: IpcOkPayload::PlaybackName { name },
            },
            Err(error) => IpcResult::Error {
                message: error.to_string(),
            },
        },
        IpcOp::UnloadDspChain { device_system_name } => {
            unload_dsp_chain_result(native_dsp_host::unload_chain(&device_system_name))
        }
        IpcOp::IsDspChainLoaded { device_system_name } => IpcResult::Ok {
            payload: IpcOkPayload::Loaded {
                loaded: native_dsp_host::is_loaded(&device_system_name),
            },
        },
        IpcOp::SetDspChainLiveParams {
            device_system_name,
            config,
        } => match native_dsp_host::set_live_chain(&device_system_name, &config) {
            Ok(()) => IpcResult::Ok {
                payload: IpcOkPayload::Unit,
            },
            Err(error) => IpcResult::Error {
                message: error.to_string(),
            },
        },
    }
}

fn unload_dsp_chain_result(result: Result<(), native_dsp_host::NativeDspHostError>) -> IpcResult {
    match result {
        Ok(()) => IpcResult::Ok {
            payload: IpcOkPayload::Unit,
        },
        Err(error) => IpcResult::Error {
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unload_dsp_chain_dispatch_forwards_actual_result() {
        let source = include_str!("server.rs");
        let dispatch = source
            .split("fn dispatch(op: IpcOp) -> IpcResult {")
            .nth(1)
            .expect("server.rs: dispatch function must exist")
            .split("\nfn unload_dsp_chain_result")
            .next()
            .expect("server.rs: dispatch body must exist");
        let forwarding = regex::Regex::new(concat!(
            r"IpcOp::UnloadDspChain\s*\{\s*device_system_name\s*\}\s*=>\s*\{\s*",
            r"unload_dsp_chain_result\(\s*native_dsp_host::unload_chain\(\s*&device_system_name\s*\)\s*\)\s*\}"
        ))
        .expect("valid unload dispatch forwarding pattern");
        assert!(
            forwarding.is_match(dispatch),
            "server.rs: dispatch UnloadDspChain arm must forward the actual native_dsp_host::unload_chain result into unload_dsp_chain_result"
        );
    }

    #[test]
    fn unload_dsp_chain_without_loaded_host_returns_unit() {
        let host = Err::<(), _>(native_dsp_host::NativeDspHostError::InitFailed(
            "unavailable host".to_string(),
        ));
        let result = native_dsp_host::unload_if_initialized(host, |()| {
            panic!("an unavailable host must not unload a chain");
        });
        assert_eq!(
            unload_dsp_chain_result(result),
            IpcResult::Ok {
                payload: IpcOkPayload::Unit,
            }
        );
    }

    #[test]
    fn unload_dsp_chain_poison_returns_ipc_error() {
        assert_eq!(
            unload_dsp_chain_result(Err(native_dsp_host::NativeDspHostError::LockPoisoned)),
            IpcResult::Error {
                message: "native dsp host mutex poisoned".to_string(),
            }
        );
    }
}
