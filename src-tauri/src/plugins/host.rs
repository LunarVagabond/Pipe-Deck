use crate::core::models::RuntimeGraph;
use crate::plugins::audit;
use crate::plugins::capabilities::{is_granted, UI_PANEL_REGISTER};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STDERR_TAIL_LINES: usize = 20;

#[derive(Debug, Error)]
pub enum HostError {
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<RpcErrorBody>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    message: String,
}

#[derive(Debug, Deserialize)]
struct RpcNotification {
    method: String,
    params: Option<Value>,
}

pub struct PluginProcess {
    pub child: Child,
    stdin: ChildStdin,
    next_id: u64,
    rx: mpsc::Receiver<String>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    pub ui_panels: Vec<crate::core::models::PluginUiPanel>,
}

impl PluginProcess {
    pub fn spawn(entry: &Path, plugin_id: &str, working_dir: &Path) -> Result<Self, HostError> {
        let mut child = Command::new(entry)
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| HostError::Spawn(error.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HostError::Spawn("stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HostError::Spawn("stdout unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HostError::Spawn("stderr unavailable".into()))?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
        let stderr_tail_writer = stderr_tail.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut tail) = stderr_tail_writer.lock() {
                    if tail.len() == STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
        });

        audit::log(plugin_id, "spawn", "ok", None);
        Ok(Self {
            child,
            stdin,
            next_id: 1,
            rx,
            stderr_tail,
            ui_panels: Vec::new(),
        })
    }

    /// Joined tail of the plugin's stderr output (most recent lines only), or `None`
    /// if the plugin hasn't written anything to stderr. Used to enrich error messages
    /// that would otherwise be a bare RPC/timeout string (see #118).
    pub fn stderr_tail(&self) -> Option<String> {
        let tail = self.stderr_tail.lock().ok()?;
        if tail.is_empty() {
            return None;
        }
        Some(tail.iter().cloned().collect::<Vec<_>>().join("\n"))
    }

    pub fn initialize(
        &mut self,
        plugin_id: &str,
        granted: &[String],
        config_dir: &Path,
    ) -> Result<(), HostError> {
        let params = serde_json::json!({
            "api_version": 1,
            "plugin_id": plugin_id,
            "granted_capabilities": granted,
            "config_dir": config_dir.to_string_lossy(),
        });
        let _ = self.request("initialize", params)?;
        Ok(())
    }

    pub fn shutdown(&mut self, plugin_id: &str) {
        let rpc_result = self.request("shutdown", serde_json::json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();

        if rpc_result.is_err() {
            let detail = self.stderr_tail();
            audit::log(plugin_id, "shutdown", "error", detail.as_deref());
        } else {
            audit::log(plugin_id, "shutdown", "ok", None);
        }
    }

    pub fn notify_graph_updated(&mut self, graph: &RuntimeGraph) -> Result<(), HostError> {
        let params = serde_json::to_value(graph)
            .map_err(|error| HostError::Rpc(error.to_string()))?;
        self.notify("graph.updated", params)
    }

    pub fn drain_notifications(&mut self, plugin_id: &str, granted: &[String]) {
        while let Ok(line) = self.rx.try_recv() {
            self.handle_line(&line, plugin_id, granted);
        }
    }

    fn handle_line(&mut self, line: &str, plugin_id: &str, granted: &[String]) {
        let Ok(notification) = serde_json::from_str::<RpcNotification>(line) else {
            return;
        };
        if notification.method == UI_PANEL_REGISTER && is_granted(granted, UI_PANEL_REGISTER) {
            if let Some(params) = notification.params {
                if let Ok(panel) =
                    serde_json::from_value::<crate::core::models::PluginUiPanel>(params)
                {
                    self.ui_panels.retain(|entry| entry.id != panel.id);
                    self.ui_panels.push(panel);
                    audit::log(plugin_id, "ui.panel.register", "ok", None);
                }
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, HostError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = RpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let payload = serde_json::to_string(&request).map_err(|e| HostError::Rpc(e.to_string()))?;
        writeln!(self.stdin, "{payload}")
            .map_err(|error| HostError::Rpc(error.to_string()))?;
        self.stdin
            .flush()
            .map_err(|error| HostError::Rpc(error.to_string()))?;

        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
                Ok(line) => {
                    if let Ok(response) = serde_json::from_str::<RpcResponse>(&line) {
                        if response.id == Some(id) {
                            if let Some(error) = response.error {
                                return Err(HostError::Rpc(error.message));
                            }
                            return Ok(response.result.unwrap_or(Value::Null));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(HostError::Rpc("plugin stdout closed".into()));
                }
            }
        }
        Err(HostError::Timeout)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), HostError> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let text = serde_json::to_string(&payload).map_err(|e| HostError::Rpc(e.to_string()))?;
        writeln!(self.stdin, "{text}").map_err(|e| HostError::Rpc(e.to_string()))?;
        self.stdin.flush().map_err(|e| HostError::Rpc(e.to_string()))?;
        Ok(())
    }
}
