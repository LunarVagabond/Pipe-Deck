use crate::pipewire::adapter::AdapterError;
use std::collections::HashMap;
use std::process::Command;

const STEREO_MONITOR_SUFFIXES: [(&str, &str); 2] = [(":monitor_FL", ":playback_FL"), (":monitor_FR", ":playback_FR")];
const STEREO_INPUT_SUFFIXES: [(&str, &str); 2] = [(":monitor_FL", ":input_FL"), (":monitor_FR", ":input_FR")];

pub fn link_sink_monitor_to_target(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> Result<(), AdapterError> {
    let suffix_pairs = if target_is_virtual_source {
        &STEREO_INPUT_SUFFIXES[..]
    } else {
        &STEREO_MONITOR_SUFFIXES[..]
    };

    if monitor_route_matches(source_system_name, target_system_name, suffix_pairs) {
        return Ok(());
    }

    disconnect_sink_monitor_route(source_system_name, target_system_name)?;

    for (monitor_suffix, input_suffix) in suffix_pairs {
        let output_port = format!("{source_system_name}{monitor_suffix}");
        let input_port = format!("{target_system_name}{input_suffix}");
        run_pw_link(&["-L", &output_port, &input_port])?;
    }

    Ok(())
}

pub fn is_sink_monitor_routed_to(
    source_system_name: &str,
    target_system_name: &str,
    target_is_virtual_source: bool,
) -> bool {
    let suffix_pairs = if target_is_virtual_source {
        &STEREO_INPUT_SUFFIXES[..]
    } else {
        &STEREO_MONITOR_SUFFIXES[..]
    };
    monitor_route_matches(source_system_name, target_system_name, suffix_pairs)
}

fn monitor_route_matches(
    source_system_name: &str,
    target_system_name: &str,
    suffix_pairs: &[(&str, &str)],
) -> bool {
    let existing = list_monitor_links_for_source(source_system_name);
    suffix_pairs.iter().all(|(monitor_suffix, input_suffix)| {
        let output_port = format!("{source_system_name}{monitor_suffix}");
        let input_port = format!("{target_system_name}{input_suffix}");
        existing
            .iter()
            .any(|(output, input)| output == &output_port && input == &input_port)
    })
}

pub fn list_all_monitor_routes_for_source(source_system_name: &str) -> Vec<String> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut targets = Vec::new();
    let mut current_target_port: Option<String> = None;
    let prefix = format!("{source_system_name}:");

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            if source_port.starts_with(&prefix) {
                if let Some(target_port) = current_target_port.as_deref() {
                    if let Some((_, target_name)) = parse_stereo_route_pair(source_port, target_port)
                    {
                        if !targets.contains(&target_name) {
                            targets.push(target_name);
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

    targets
}

pub fn disconnect_sink_monitor_route(
    source_system_name: &str,
    target_system_name: &str,
) -> Result<(), AdapterError> {
    disconnect_stereo_route(source_system_name, target_system_name, &STEREO_MONITOR_SUFFIXES)?;
    disconnect_stereo_route(source_system_name, target_system_name, &STEREO_INPUT_SUFFIXES)
}

fn disconnect_stereo_route(
    source_system_name: &str,
    target_system_name: &str,
    suffix_pairs: &[(&str, &str)],
) -> Result<(), AdapterError> {
    for (source_suffix, target_suffix) in suffix_pairs {
        let output_port = format!("{source_system_name}{source_suffix}");
        let input_port = format!("{target_system_name}{target_suffix}");
        let _ = run_pw_link(&["-d", &output_port, &input_port]);
    }
    Ok(())
}

pub fn disconnect_sink_monitor(source_system_name: &str) -> Result<(), AdapterError> {
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
) -> Result<(), AdapterError> {
    link_capture_source_to_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

pub fn disconnect_capture_source_from_virtual_input(
    capture_source_system_name: &str,
    virtual_input_system_name: &str,
) -> Result<(), AdapterError> {
    disconnect_capture_source_from_target_ports(capture_source_system_name, virtual_input_system_name, "input_")
}

/// Links a physical capture source into a regular sink's playback ports
/// (as opposed to a virtual-input's `input_*` ports). Used to feed a
/// per-mix-source gain node (a plain null-sink "feed sink") ahead of
/// summing it into a virtual mic via that sink's monitor.
pub fn link_capture_source_to_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Result<(), AdapterError> {
    link_capture_source_to_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

pub fn disconnect_capture_source_from_sink(
    capture_source_system_name: &str,
    sink_system_name: &str,
) -> Result<(), AdapterError> {
    disconnect_capture_source_from_target_ports(capture_source_system_name, sink_system_name, "playback_")
}

fn link_capture_source_to_target_ports(
    capture_source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Result<(), AdapterError> {
    let source_ports = output_ports_for(capture_source_system_name);
    if source_ports.is_empty() {
        return Err(AdapterError::Message(format!(
            "capture source {capture_source_system_name} has no output ports"
        )));
    }

    let target_ports = target_ports_with_prefix(target_system_name, target_port_prefix);
    if target_ports.is_empty() {
        return Err(AdapterError::Message(format!(
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
) -> Result<(), AdapterError> {
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

pub fn list_monitor_routes() -> HashMap<String, String> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return HashMap::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut routes = HashMap::new();
    let mut current_target_port: Option<String> = None;

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            let Some(target_port) = current_target_port.as_deref() else {
                continue;
            };

            if let Some((source_name, target_name)) = parse_stereo_route_pair(source_port, target_port) {
                routes.insert(source_name, target_name);
            }
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() && trimmed.contains(':') {
            current_target_port = Some(trimmed.to_string());
        }
    }

    routes
}

fn parse_stereo_route_pair(source_port: &str, target_port: &str) -> Option<(String, String)> {
    parse_route_pair(source_port, target_port, &STEREO_MONITOR_SUFFIXES)
        .or_else(|| parse_route_pair(source_port, target_port, &STEREO_INPUT_SUFFIXES))
}

fn parse_route_pair(
    source_port: &str,
    target_port: &str,
    suffix_pairs: &[(&str, &str)],
) -> Option<(String, String)> {
    for (source_suffix, target_suffix) in suffix_pairs {
        if source_port.ends_with(source_suffix) && target_port.ends_with(target_suffix) {
            let source_name = source_port.strip_suffix(source_suffix)?;
            let target_name = target_port.strip_suffix(target_suffix)?;
            return Some((source_name.to_string(), target_name.to_string()));
        }
    }
    None
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

fn run_pw_link(args: &[&str]) -> Result<(), AdapterError> {
    let output = Command::new("pw-link")
        .args(args)
        .output()
        .map_err(|error| AdapterError::Message(format!("failed to run pw-link: {error}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AdapterError::Message(format!(
        "pw-link {} failed: {stderr}",
        args.join(" ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stereo_route_pair() {
        let pair = parse_stereo_route_pair(
            "pipe-deck-asdf:monitor_FL",
            "alsa_output.pci-0000_01_00.1.hdmi-stereo:playback_FL",
        );
        assert_eq!(
            pair,
            Some((
                "pipe-deck-asdf".into(),
                "alsa_output.pci-0000_01_00.1.hdmi-stereo".into()
            ))
        );
    }

    #[test]
    fn parses_virtual_source_route_pair() {
        let pair = parse_stereo_route_pair(
            "soundux_sink:monitor_FL",
            "pipe-deck-mic:input_FL",
        );
        assert_eq!(
            pair,
            Some(("soundux_sink".into(), "pipe-deck-mic".into()))
        );
    }

    #[test]
    fn missing_route_is_not_considered_linked() {
        assert!(!monitor_route_matches(
            "soundux_sink",
            "pipe-deck-mic",
            &STEREO_INPUT_SUFFIXES,
        ));
    }
}
