mod mixer;
mod parse;
mod routing;
mod r#virtual;

pub use mixer::*;
pub use parse::*;
pub use routing::*;
pub use r#virtual::*;

use crate::backend::BackendError;
use std::process::Command;

pub(crate) fn run_pactl(args: &[&str]) -> Result<String, BackendError> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run pactl: {error}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(BackendError::Message(format!(
        "pactl {} failed: {stderr}",
        args.join(" ")
    )))
}
