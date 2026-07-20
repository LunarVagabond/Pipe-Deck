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

    let target_prefix = format!("{target_system_name}:");
    let existing: Vec<(String, String)> = list_monitor_links_for_source(source_system_name)
        .into_iter()
        .filter(|(_, input_port)| input_port.starts_with(&target_prefix))
        .collect();

    apply_link_diff(&existing, &desired)
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
    disconnect_links(
        list_monitor_links_for_source(source_system_name)
            .into_iter()
            .filter(|(_, input_port)| input_port.starts_with(&target_prefix)),
    )
}

pub fn disconnect_sink_monitor(source_system_name: &str) -> Result<(), BackendError> {
    disconnect_links(list_monitor_links_for_source(source_system_name))
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
    let target_prefix = format!("{target_system_name}:{target_port_prefix}");
    let existing: Vec<(String, String)> = list_capture_links_for_source(capture_source_system_name)
        .into_iter()
        .filter(|(_, input_port)| input_port.starts_with(&target_prefix))
        .collect();

    let already_linked = desired
        .iter()
        .all(|(output, input)| existing.iter().any(|(o, i)| o == output && i == input));
    if already_linked {
        return Ok(());
    }

    apply_link_diff(&existing, &desired)
}

fn disconnect_capture_source_from_target_ports(
    capture_source_system_name: &str,
    target_system_name: &str,
    target_port_prefix: &str,
) -> Result<(), BackendError> {
    let target_prefix = format!("{target_system_name}:{target_port_prefix}");
    disconnect_links(
        list_capture_links_for_source(capture_source_system_name)
            .into_iter()
            .filter(|(_, input_port)| input_port.starts_with(&target_prefix)),
    )
}

/// Reconciles `existing` against `desired` by disconnecting only the pairs
/// that shouldn't be there anymore and linking only the pairs that are
/// missing, instead of tearing down every existing link for a target and
/// relinking the whole desired set unconditionally. The old blind
/// disconnect-then-relink approach caused an audible dropout on legs that
/// hadn't actually changed on every reroute, plus a window where a
/// concurrent `pw-dump` snapshot could catch the target with zero live
/// links. Both `existing`/`desired` are small (per-target port counts), so
/// a linear `contains` scan per side is simpler than reaching for a `HashSet`
/// and no less correct.
fn apply_link_diff(existing: &[(String, String)], desired: &[(String, String)]) -> Result<(), BackendError> {
    let to_remove = existing.iter().filter(|pair| !desired.contains(pair)).cloned();
    disconnect_links(to_remove)?;

    for (output_port, input_port) in desired {
        let already_present = existing.iter().any(|(o, i)| o == output_port && i == input_port);
        if !already_present {
            run_pw_link(&["-L", output_port, input_port])?;
        }
    }

    Ok(())
}

/// Runs `-d` for every `(output_port, input_port)` pair, continuing past
/// individual failures (a link already gone by the time we get to it isn't
/// fatal to the rest of the batch) but — unlike the `let _ = run_pw_link(...)`
/// this replaced — collecting and returning them instead of discarding them,
/// so a genuine failure (not just "already disconnected") is visible to the
/// caller rather than silently treated as success.
fn disconnect_links(links: impl IntoIterator<Item = (String, String)>) -> Result<(), BackendError> {
    let mut failures = Vec::new();
    for (output_port, input_port) in links {
        if let Err(error) = run_pw_link(&["-d", &output_port, &input_port]) {
            failures.push(format!("{output_port} -> {input_port}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(BackendError::Message(format!(
            "failed to disconnect {} link(s): {}",
            failures.len(),
            failures.join("; ")
        )))
    }
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
    let target_prefix = format!("{target_system_name}:{target_port_prefix}");
    let mut sources = Vec::new();
    for (source_port, target_port) in run_pw_link_list() {
        if target_port.starts_with(&target_prefix) {
            if let Some(source_name) = capture_source_name_from_port(&source_port) {
                if !sources.contains(&source_name) {
                    sources.push(source_name);
                }
            }
        }
    }
    sources
}

fn list_capture_links_for_source(capture_source_system_name: &str) -> Vec<(String, String)> {
    links_from_source(capture_source_system_name)
}

fn capture_source_name_from_port(port: &str) -> Option<String> {
    port.rsplit_once(':').map(|(name, _port)| name.to_string())
}

fn list_monitor_links_for_source(source_system_name: &str) -> Vec<(String, String)> {
    links_from_source(source_system_name)
}

/// Every currently-linked `(output_port, input_port)` pair whose output
/// port belongs to `source_system_name` — the shared implementation behind
/// `list_capture_links_for_source`/`list_monitor_links_for_source`, kept as
/// two named call sites for readability (capture-into-target vs
/// monitor-into-target mean different things to a reader) even though the
/// underlying lookup is identical.
fn links_from_source(source_system_name: &str) -> Vec<(String, String)> {
    let prefix = format!("{source_system_name}:");
    run_pw_link_list()
        .into_iter()
        .filter(|(source_port, _)| source_port.starts_with(&prefix))
        .collect()
}

/// Runs `pw-link -l` and parses it via `parse_pw_link_list` — the one
/// subprocess call site every `list_*` lookup in this file goes through.
fn run_pw_link_list() -> Vec<(String, String)> {
    let output = match Command::new("pw-link").arg("-l").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    parse_pw_link_list(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `pw-link -l` output into every `(output_port, input_port)` pair
/// it reports. Previously this state machine was hand-duplicated three
/// times (once per `list_*` lookup), each silently falling through to
/// "treat as a port header" on any line it didn't recognize — a real format
/// change (different indentation, arrow characters, an unhandled relation
/// marker) would have silently misparsed or dropped links with no signal
/// anywhere. Consolidated here so format-drift detection only needs writing
/// once: the first line that doesn't fit either the input-link relation or
/// a port-header shape prints a warning (this crate has no logging
/// dependency — `eprintln!` is the existing diagnostic convention, see
/// `backend::mod::create_backend`) instead of being silently absorbed.
fn parse_pw_link_list(text: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    let mut current_target_port: Option<String> = None;
    let mut warned = false;

    for line in text.lines() {
        if let Some(source_port) = line.strip_prefix("  |<- ") {
            let source_port = source_port.trim();
            match current_target_port.as_deref() {
                Some(target_port) => links.push((source_port.to_string(), target_port.to_string())),
                None if !warned => {
                    eprintln!("pw-link -l: input-link line with no preceding port header, skipping: {line:?}");
                    warned = true;
                }
                None => {}
            }
            continue;
        }

        // Output-direction relation lines (`  |-> `) exist in real `pw-link -l`
        // output but nothing here has ever needed them — every lookup in this
        // file only cares about what feeds *into* a target — so they're
        // recognized and skipped rather than falling through to the
        // port-header branch and corrupting `current_target_port`.
        if line.starts_with("  |-> ") {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains(':') {
            current_target_port = Some(trimmed.to_string());
        } else if !warned {
            eprintln!("pw-link -l: unrecognized line (expected a port name or a relation), skipping: {line:?}");
            warned = true;
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

    /// Realistic `pw-link -l` output: a stereo hardware target, a mono
    /// Bluetooth target fed by both channels of a stereo monitor, a mic
    /// capture feeding a virtual input's two ports from one mono source, an
    /// output-direction (`|-> `) relation line that should be skipped, and
    /// a target header with no links under it at all.
    const PW_LINK_LIST_FIXTURE: &str = include_str!("../../../tests/fixtures/pw_link_list.txt");

    #[test]
    fn parse_pw_link_list_extracts_every_input_relation_pair() {
        let links = parse_pw_link_list(PW_LINK_LIST_FIXTURE);

        assert!(links.contains(&(
            "pipe-deck-game:monitor_FL".to_string(),
            "alsa_output.pci-0000_01_00.1.hdmi-stereo:playback_FL".to_string()
        )));
        assert!(links.contains(&(
            "pipe-deck-game:monitor_FR".to_string(),
            "alsa_output.pci-0000_01_00.1.hdmi-stereo:playback_FR".to_string()
        )));
        // Mono Bluetooth target fed by both channels of a stereo source.
        assert!(links.contains(&(
            "pipe-deck-chat:monitor_FL".to_string(),
            "bluez_output.AA_BB_CC_DD_EE_FF.1:playback_MONO".to_string()
        )));
        assert!(links.contains(&(
            "pipe-deck-chat:monitor_FR".to_string(),
            "bluez_output.AA_BB_CC_DD_EE_FF.1:playback_MONO".to_string()
        )));
        // A mono mic capture cycled across both input ports of a virtual mic.
        assert!(links.contains(&(
            "alsa_input.usb-SteelSeries_Arctis_Nova_Pro_Wireless-00.mono-fallback:capture_MONO".to_string(),
            "pipe-deck-mic:input_FL".to_string()
        )));

        // The `|-> ` (output-direction) lines must never be mistaken for
        // `|<- ` input relations or for port headers.
        assert!(!links.iter().any(|(output, _)| output.starts_with("alsa_output.pci-0000_01_00.1.hdmi-stereo")));

        // A header with no relation lines under it contributes no links.
        assert!(!links
            .iter()
            .any(|(_, input)| input.starts_with("alsa_output.pci-0000_02_00.0.analog-stereo")));
    }

    #[test]
    fn parse_pw_link_list_skips_an_input_relation_with_no_preceding_header_without_panicking() {
        let links = parse_pw_link_list("  |<- orphaned:monitor_FL\n");
        assert!(links.is_empty());
    }

    #[test]
    fn parse_pw_link_list_skips_an_unrecognized_line_without_panicking() {
        // No colon, not a relation line — format drift, not a crash.
        let links = parse_pw_link_list("some garbage line\ntarget:playback_FL\n  |<- source:monitor_FL\n");
        assert_eq!(links, vec![("source:monitor_FL".to_string(), "target:playback_FL".to_string())]);
    }

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
