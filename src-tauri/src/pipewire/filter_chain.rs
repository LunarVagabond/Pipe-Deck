use crate::pipewire::adapter::AdapterError;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectChainConfig {
    #[serde(default)]
    pub eq_low: i32,
    #[serde(default)]
    pub eq_mid: i32,
    #[serde(default)]
    pub eq_high: i32,
    #[serde(default)]
    pub compressor: bool,
}

pub fn is_pipe_deck_device(system_name: &str) -> bool {
    system_name.starts_with("pipe-deck-")
        && !system_name.starts_with("pipe-deck-feed-")
        && !system_name.starts_with("pipe-deck-split-")
}

pub fn apply_effect_chain(
    device_system_name: &str,
    config: &EffectChainConfig,
) -> Result<(), AdapterError> {
    if !is_pipe_deck_device(device_system_name) {
        return Err(AdapterError::Message(
            "effects may only be applied to pipe-deck-owned devices".into(),
        ));
    }

    // MVP: store config in plugin state; PipeWire filter-chain integration is best-effort
    // when module-filter-chain and LADSPA plugins are available on the host.
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }

    let graph = build_filter_graph(device_system_name, config);
    let _ = unload_filter_chain(device_system_name);
    run_pactl_load_module(&[
        "load-module",
        "module-filter-chain",
        &format!("sink_name={device_system_name}-fx"),
        &format!("filter.graph={graph}"),
    ])
}

pub fn unload_filter_chain(device_system_name: &str) -> Result<(), AdapterError> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Ok(());
    }
    let module_name = format!("{device_system_name}-fx");
    let output = Command::new("pactl")
        .args(["list", "short", "modules"])
        .output()
        .map_err(|error| AdapterError::Message(format!("pactl failed: {error}")))?;
    if !output.status.success() {
        return Ok(());
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.contains(&module_name) {
            if let Some(index) = line.split_whitespace().next() {
                let _ = Command::new("pactl")
                    .args(["unload-module", index])
                    .status();
            }
        }
    }
    Ok(())
}

fn build_filter_graph(device_system_name: &str, config: &EffectChainConfig) -> String {
    let mut nodes = vec![
        format!("node.name={device_system_name}-capture"),
        "type=capture".into(),
        format!("audio.position=[FL FR]"),
        format!("capture.master={device_system_name}"),
    ];
    if config.compressor {
        nodes.push("ladspa.plugin=mcompressor".into());
        nodes.push("ladspa.label=Compressor".into());
    }
    if config.eq_low != 0 || config.eq_mid != 0 || config.eq_high != 0 {
        nodes.push("ladspa.plugin=eq".into());
        nodes.push(format!("control.0={}", config.eq_low));
        nodes.push(format!("control.1={}", config.eq_mid));
        nodes.push(format!("control.2={}", config.eq_high));
    }
    nodes.push(format!("node.name={device_system_name}-playback"));
    nodes.push("type=playback".into());
    nodes.push(format!("audio.position=[FL FR]"));
    nodes.push(format!("playback.master={device_system_name}"));
    nodes.join(";")
}

fn run_pactl_load_module(args: &[&str]) -> Result<(), AdapterError> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| AdapterError::Message(format!("pactl failed: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    // Filter chain may be unavailable without LADSPA — non-fatal for MVP.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_pipe_deck_virtual_devices() {
        assert!(is_pipe_deck_device("pipe-deck-game-mix"));
        assert!(!is_pipe_deck_device("pipe-deck-split-firefox"));
        assert!(!is_pipe_deck_device("alsa_output.pci"));
    }

    #[test]
    fn builds_filter_graph_with_eq_nodes() {
        let graph = build_filter_graph(
            "pipe-deck-test",
            &EffectChainConfig {
                eq_low: 1,
                eq_mid: 2,
                eq_high: 3,
                compressor: true,
            },
        );
        assert!(graph.contains("pipe-deck-test-capture"));
        assert!(graph.contains("control.0=1"));
    }
}
