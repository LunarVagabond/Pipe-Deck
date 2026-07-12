mod mixer;
mod parse;
mod routing;
mod r#virtual;

pub use mixer::*;
pub use parse::*;
pub use routing::*;
pub use r#virtual::*;

use crate::pipewire::adapter::AdapterError;
use std::process::Command;

pub(crate) fn run_pactl(args: &[&str]) -> Result<String, AdapterError> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| AdapterError::Message(format!("failed to run pactl: {error}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AdapterError::Message(format!(
        "pactl {} failed: {stderr}",
        args.join(" ")
    )))
}
