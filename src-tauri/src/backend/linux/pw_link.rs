use crate::backend::BackendError;
use std::process::Command;

/// Links a virtual sink's monitor ports into a target's playback (or, for a
/// virtual-input target, input) ports.
///
/// Ports are discovered rather than assumed to be a stereo FL/FR pair (see
/// `link_capture_source_to_target_ports` for the same reasoning). A Bluetooth
/// output in a mono profile (HSP/HFP, as opposed to stereo A2DP) exposes a
/// single `playback_MONO` port; hardcoding `playback_FL`/`playback_FR` meant
/// adding such a device to an existing multi-output group failed outright —
/// and since fan-out previously aborted the whole batch on the first failing
/// target, this was order-dependent: linking the Bluetooth device *first*
/// (as the sole target) never hit the stereo-pair code path building a
/// group, only the mono/stereo cycling below, so it worked, while adding it
/// *after* a stereo hardware target had already been fanned out did not.
pub fn link_sink_monitor_to_target(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> Result<(), BackendError> {
    let target_port_prefix = if target_is_virtual_source { "input_" } else { "playback_" };

    let source_ports = output_ports_for(source_system_name);
    if source_ports.is_empty() {
        return Err(BackendError::Message(format!(
            "sink {source_system_name} has no monitor ports"
        )));
    }

    let target_ports = target_ports_with_prefix(target_system_name, target_port_prefix);
    if target_ports.is_empty() {
        return Err(BackendError::Message(format!(
            "{target_system_name} has no {target_port_prefix}* ports to route into"
        )));
    }

    let desired = pair_capture_ports(&source_ports, &target_ports);
    if monitor_route_matches(source_system_name, &desired) {
        return Ok(());
    }

    disconnect_sink_monitor_route(source_system_name, target_system_name)?;

    for (output_port, input_port) in &desired {
        run_pw_link(&["-L", output_port, input_port])?;
    }

    Ok(())
}

pub fn is_sink_monitor_routed_to(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> bool {
    let target_port_prefix = if target_is_virtual_source { "input_" } else { "playback_" };
    let source_ports = output_ports_for(source_system_name);
    let target_ports = target_ports_with_prefix(target_system_name, target_port_prefix);
    if source_ports.is_empty() || target_ports.is_empty() {
        return false;
    }
    let desired = pair_capture_ports(&source_ports, &target_ports);
    monitor_route_matches(source_system_name, &desired)
}

fn monitor_route_matches(source_system_name: &str, desired: &[(String, String)]) -> bool {
    let existing = list_monitor_links_for_source(source_system_name);
    desired
        .iter()
        .all(|(output, input)| existing.iter().any(|(o, i)| o == output && i == input))
}

pub fn list_all_monitor_routes_for_source(source_system_name: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for (_, target_port) in list_monitor_links_for_source(source_system_name) {
        if let Some(target_name) = capture_source_name_from_port(&target_port) {
            if !targets.contains(&target_name) {
                targets.push(target_name);
            }
        }
    }
    targets
}

pub fn disconnect_sink_monitor_route(
    source_system_name: &str,
    target_system_name: &str,
) -> Result<(), BackendError> {
    let target_prefix = format!("{target_system_name}:");
    for (output_port, input_port) in list_monitor_links_for_source(source_system_name) {
        if input_port.starts_with(&target_prefix) {
            let _ = run_pw_link(&["-d", &output_port, &input_port]);
        }
    }
    Ok(())
}

pub fn disconnect_sink_monitor(source_system_name: &str) -> Result<(), BackendError> {
    for (output_port, input_port) in list_monitor_links_for_source(source_system_name) {
        let _ = run_pw_link(&["-d", &output_port, &input_port]);
    }
    Ok(())
}

/// Mix a hardware capture source into a virtual microphone's sink inputs.
///
/// Ports are discovered rather than assumed to be a stereo FL/FR pair, since
/// mono devices (e.g. a headset mic reported as "...mono-fallback") expose a
/// single MONO port. The source's ports are cycled across every target port,
/// which fans a mono source out to both channels of a stereo target and pairs
/// a stereo source 1:1 with a stereo target.
pub fn link_capture_source_to_virtual_input(
    capture_source_system_name: &str,
    virtual_input_system_name: &str,
) -> Result<(), BackendError> {
    link_capture_source_to_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

pub fn disconnect_capture_source_from_virtual_input(
    capture_source_system_name: &str,
    virtual_input_system_name: &str,
) -> Result<(), BackendError> {
    disconnect_capture_source_from_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

/// Links a physical capture source into a regular sink's playback ports
/// (as opposed to a virtual-input's `input_*` ports). Used to feed a
/// per-mix-source gain node (a plain null-sink "feed sink") ahead of
/// summing it into a virtual mic via that sink's monitor.
pub fn link_capture_source_to_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Result<(), BackendError> {
    link_capture_source_to_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

pub fn disconnect_capture_source_from_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Result<(), BackendError> {
    disconnect_capture_source_from_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

fn link_capture_source_to_target_ports(
    capture_source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Result<(), BackendError> {
    let source_ports = output_ports_for(capture_source_system_name);
    if source_ports.is_empty() {
        return Err(BackendError::Message(format!(
            "capture source {capture_source_system_name} has no output ports"
        )));
    }

    let target_ports = target_ports_with_prefix(target_system_name, target_port_prefix);
    if target_ports.is_empty() {
        return Err(BackendError::Message(format!(
            "{target_system_name} has no {target_port_prefix}* ports to mix into"
        )));
    }

    let desired = pair_capture_ports(&source_ports, &target_ports);
    let existing = list_capture_links_for_source(capture_source_system_name);

    let already_linked = desired
        .iter()
        .all(|(output, input)| existing.iter().any(|(o, i)| o == output && i == input));
    if already_linked {
        return Ok(());
    }

    disconnect_capture_source_from_target_ports(capture_source_system_name, target_system_name, target_port_prefix)?;

    for (output_port, input_port) in &desired {
        run_pw_link(&["-L", output_port, input_port])?;
    }

    Ok(())
}

fn disconnect_capture_source_from_target_ports(
    capture_source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Result<(), BackendError> {
    let target_prefix = format!("{target_system_name}:{target_port_prefix}");
    for (output_port, input_port) in list_capture_links_for_source(capture_source_system_name) {
        if input_port.starts_with(&target_prefix) {
            let _ = run_pw_link(&["-d", &output_port, &input_port]);
        }
    }
    Ok(())
}

fn list_ports(flag: &str) -> Vec<String> {
    let output = match Command::new("pw-link").arg(flag).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn output_ports_for(system_name: &str) -> Vec<String> {
    let prefix = format!("{system_name}:");
    list_ports("-o")
        .into_iter()
        .filter(|port| port.starts_with(&prefix))
        .collect()
}

/// Whether `pw-link -o` currently reports any output port for `system_name` —
/// used to confirm a node has actually registered its ports before wiring
/// anything to it (a node/sink can exist slightly before its ports do).
pub fn has_output_ports(system_name: &str) -> bool {
    !output_ports_for(system_name).is_empty()
}

/// Whether `pw-link -i` currently reports any input port for `system_name` —
/// the counterpart to `has_output_ports`, used to confirm a capture-direction
/// effects inlet (`effect_input.*`) has registered its ports before wiring
/// the mic-mix feed into it.
pub fn has_input_ports(system_name: &str) -> bool {
    let prefix = format!("{system_name}:");
    list_ports("-i").into_iter().any(|port| port.starts_with(&prefix))
}

fn target_ports_with_prefix(system_name: &str, port_prefix: &str) -> Vec<String> {
    let prefix = format!("{system_name}:{port_prefix}");
    list_ports("-i")
        .into_iter()
        .filter(|port| port.starts_with(&prefix))
        .collect()
}

/// Pair source ports with target ports, cycling the (sorted) source list
/// across every (sorted) target port so channel counts need not match.
fn pair_capture_ports(source_ports: &[String], target_ports: &[String]) -> Vec<(String, String)> {
    let mut sorted_sources = source_ports.to_vec();
    sorted_sources.sort();
    let mut sorted_targets = target_ports.to_vec();
    sorted_targets.sort();

    sorted_targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| (sorted_sources[index % sorted_sources.len()].clone(), target))
        .collect()
}

pub fn list_capture_sources_for_virtual_input(virtual_input_system_name: &str) -> Vec<String> {
    list_capture_sources_for_target_ports(virtual_input_system_name, "input_")
}

/// Same discovery as `list_capture_sources_for_virtual_input`, but against a
/// regular sink's playback ports (a per-mix-source feed sink).
pub fn list_capture_sources_for_sink(sink_system_name: &str) -> Vec<String> {
    list_capture_sources_for_target_ports(sink_system_name, "playback_")
}

fn list_capture_sources_for_target_ports(target_system_name: &str, target_port_prefix: &str) -> Vec<String> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut sources = Vec::new();
    let mut current_target_port: Option<String> = None;
    let target_prefix = format!("{target_system_name}:{target_port_prefix}");

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            if let Some(target_port) = current_target_port.as_deref() {
                if target_port.starts_with(&target_prefix) {
                    if let Some(source_name) = capture_source_name_from_port(source_port) {
                        if !sources.contains(&source_name) {
                            sources.push(source_name);
                        }
                    }
                }
            }
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.contains(':') {
            current_target_port = Some(trimmed.to_string());
        }
    }

    sources
}

fn list_capture_links_for_source(capture_source_system_name: &str) -> Vec<(String, String)> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut links = Vec::new();
    let mut current_target_port: Option<String> = None;
    let prefix = format!("{capture_source_system_name}:");

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            if source_port.starts_with(&prefix) {
                if let Some(target_port) = current_target_port.as_deref() {
                    links.push((source_port.to_string(), target_port.to_string()));
                }
            }
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.contains(':') {
            current_target_port = Some(trimmed.to_string());
        }
    }

    links
}

fn capture_source_name_from_port(port: &str) -> Option<String> {
    port.rsplit_once(':').map(|(name, _port)| name.to_string())
}

fn list_monitor_links_for_source(source_system_name: &str) -> Vec<(String, String)> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut links = Vec::new();
    let mut current_target_port: Option<String> = None;
    let prefix = format!("{source_system_name}:");

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            if source_port.starts_with(&prefix) {
                if let Some(target_port) = current_target_port.as_deref() {
                    links.push((source_port.to_string(), target_port.to_string()));
                }
            }
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.contains(':') {
            current_target_port = Some(trimmed.to_string());
        }
    }

    links
}

fn run_pw_link(args: &[&str]) -> Result<(), BackendError> {
    let output = Command::new("pw-link")
        .args(args)
        .output()
        .map_err(|error| BackendError::Message(format!("failed to run pw-link: {error}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(BackendError::Message(format!(
        "pw-link {} failed: {stderr}",
        args.join(" ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_target_system_name_from_a_stereo_playback_port() {
        assert_eq!(
            capture_source_name_from_port("alsa_output.pci-0000_01_00.1.hdmi-stereo:playback_FL"),
            Some("alsa_output.pci-0000_01_00.1.hdmi-stereo".into())
        );
    }

    #[test]
    fn extracts_target_system_name_from_a_mono_bluetooth_playback_port() {
        // A Bluetooth output in a mono profile (HSP/HFP) exposes a single
        // playback_MONO port rather than a stereo FL/FR pair.
        assert_eq!(
            capture_source_name_from_port("bluez_output.AA_BB_CC_DD_EE_FF.1:playback_MONO"),
            Some("bluez_output.AA_BB_CC_DD_EE_FF.1".into())
        );
    }

    #[test]
    fn pairs_a_stereo_source_with_a_mono_target_by_cycling() {
        let source_ports = vec![
            "pipe-deck-asdf:monitor_FL".to_string(),
            "pipe-deck-asdf:monitor_FR".to_string(),
        ];
        let target_ports = vec!["bluez_output.AA_BB_CC_DD_EE_FF.1:playback_MONO".to_string()];
        let pairs = pair_capture_ports(&source_ports, &target_ports);
        assert_eq!(pairs.len(), 1, "a mono target should only need one link, not two: {pairs:?}");
        assert_eq!(pairs[0].1, "bluez_output.AA_BB_CC_DD_EE_FF.1:playback_MONO");
    }

    #[test]
    fn missing_route_is_not_considered_linked() {
        let desired = vec![(
            "soundux_sink:monitor_FL".to_string(),
            "pipe-deck-mic:input_FL".to_string(),
        )];
        assert!(!monitor_route_matches("soundux_sink", &desired));
    }
}
