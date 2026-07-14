use crate::backend::BackendError;
use std::process::Command;
use std::time::{Duration, Instant};

/// The dedicated PipeWire instance that hosts filter-chain modules
/// (`systemctl --user status filter-chain.service`, `ExecStart=/usr/bin/pipewire
/// -c filter-chain.conf`). It `BindsTo=pipewire.service` but is a *separate*
/// process — restarting it reloads only Pipe Deck's effects nodes and never
/// touches the main `pipewire.service`/`pipewire-pulse.service` graph, so
/// existing app audio and routing are never disrupted by an effects Apply.
const FILTER_CHAIN_SERVICE: &str = "filter-chain.service";

/// Restarts the filter-chain daemon and waits for it to report active.
/// Never touches `pipewire.service` / `pipewire-pulse.service` / `wireplumber.service`.
pub fn restart_filter_chain_service() -> Result<(), BackendError> {
    run_systemctl(&["--user", "restart", FILTER_CHAIN_SERVICE])?;
    wait_for_active(FILTER_CHAIN_SERVICE, Duration::from_secs(5))
}

fn run_systemctl(args: &[&str]) -> Result<(), BackendError> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run systemctl: {error}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(BackendError::Message(format!(
        "systemctl {} failed: {stderr}",
        args.join(" ")
    )))
}

fn wait_for_active(unit: &str, timeout: Duration) -> Result<(), BackendError> {
    let start = Instant::now();
    loop {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", unit])
            .output();
        if let Ok(output) = output {
            if String::from_utf8_lossy(&output.stdout).trim() == "active" {
                return Ok(());
            }
        }
        if start.elapsed() > timeout {
            return Err(BackendError::Message(format!(
                "{unit} did not report active within {timeout:?} after restart"
            )));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}
