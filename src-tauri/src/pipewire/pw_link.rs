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
