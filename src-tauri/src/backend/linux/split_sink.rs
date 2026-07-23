use crate::core::models::{Device, DeviceDirection, DeviceKind, RuntimeGraph, VirtualRole};
use crate::backend::BackendError;
use crate::backend::linux::pactl;
use crate::backend::linux::pw_link;
use crate::pipewire::filter_chain;
use std::collections::HashSet;

/// A virtual output currently hosting live effects (PD-020) has its
/// identity pinned to the *capture* side — `system_name`'s own "monitor"
/// port only ever carries the raw, pre-processing signal now, since the
/// processed audio leaves via the separately-named `effect_output.*` node
/// instead (see `pipewire::filter_chain`/`core::engine::effects_ops`).
/// Every caller that fans this device's audio out to a target must resolve
/// through this first — linking straight to `system_name`'s monitor while
/// effects are live bypasses the effect chain entirely (the target hears
/// unprocessed audio), and on a source that's ALSO already correctly linked
/// via `effect_output.*`, doing so on top of that means the target hears
/// both the raw and the processed signal mixed together. Checked against
/// live port state (not persisted config) because that's the only source of
/// truth for whether the swap has actually happened yet.
pub fn effective_fan_out_source(system_name: &str) -> String {
    let effect_output_name = filter_chain::effect_output_name_for_device(system_name);
    if pw_link::has_output_ports(&effect_output_name) {
        effect_output_name
    } else {
        system_name.to_string()
    }
}

pub fn apply_stream_to_sink(
    graph: &RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), BackendError> {
    pactl::move_stream_to_target(graph, stream_id, target_device_id)
}

pub fn apply_sink_targets(
    graph: &RuntimeGraph,
    sink_device_id: &str,
    target_device_ids: &[String],
) -> Result<(), BackendError> {
    let sink = graph
        .devices
        .iter()
        .find(|device| device.id == sink_device_id)
        .ok_or_else(|| BackendError::Message(format!("sink device not found: {sink_device_id}")))?;

    // A terminal Output (#287) is a true dead end — no forward routing of
    // any kind. Only a Bus can fan out to targets.
    if sink.kind != DeviceKind::Virtual
        || sink.direction != DeviceDirection::Output
        || sink.virtual_role != Some(VirtualRole::Bus)
    {
        return Err(BackendError::Message(
            "only virtual bus devices can fan out to targets".into(),
        ));
    }

    if would_create_cycle(graph, sink_device_id, target_device_ids) {
        return Err(BackendError::Message(format!(
            "routing \"{}\" here would create a cycle",
            sink.label
        )));
    }

    let link_source = effective_fan_out_source(&sink.system_name);

    if target_device_ids.is_empty() {
        return prune_stale_fan_out_links(&link_source, &HashSet::new());
    }

    if target_device_ids.len() == 1 && !sink.is_multi_sink() {
        let target = graph
            .devices
            .iter()
            .find(|device| device.id == target_device_ids[0])
            .ok_or_else(|| {
                BackendError::Message(format!(
                    "target device not found: {}",
                    target_device_ids[0]
                ))
            })?;
        validate_fan_out_target(target)?;
        let target_is_virtual_source =
            target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
        pw_link::link_sink_monitor_to_target(
            &link_source,
            &target.system_name,
            target_is_virtual_source,
        )?;
        let mut allowed = HashSet::new();
        allowed.insert(target.system_name.clone());
        prune_stale_fan_out_links(&link_source, &allowed)?;
        return Ok(());
    }

    fan_out_sink(graph, &sink.system_name, target_device_ids)
}

/// Links `sink_system_name`'s monitor to every target in `target_device_ids`
/// — or, if `sink_system_name` currently hosts live effects, the processed
/// `effect_output.*` node instead (see `effective_fan_out_source`).
///
/// Each target is attempted independently and failures are collected rather
/// than aborting the whole batch on the first `?` — otherwise a single
/// incompatible target (e.g. one whose port layout `link_sink_monitor_to_target`
/// can't yet handle) would both leave already-linked targets untouched *and*
/// skip `prune_stale_fan_out_links` entirely, silently freezing the group in
/// whatever state it was in before this call.
pub fn fan_out_sink(
    graph: &RuntimeGraph,
    sink_system_name: &str,
    target_device_ids: &[String],
) -> Result<(), BackendError> {
    let sink_system_name = &effective_fan_out_source(sink_system_name);
    let mut linked = HashSet::new();
    let mut errors = Vec::new();

    for target_id in target_device_ids {
        let Some(target) = graph.devices.iter().find(|device| device.id == *target_id) else {
            errors.push(format!("target device not found: {target_id}"));
            continue;
        };
        if let Err(error) = validate_fan_out_target(target) {
            errors.push(format!("{}: {error}", target.label));
            continue;
        }
        let target_is_virtual_source =
            target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
        match pw_link::link_sink_monitor_to_target(
            sink_system_name,
            &target.system_name,
            target_is_virtual_source,
        ) {
            Ok(()) => {
                linked.insert(target.system_name.clone());
            }
            Err(error) => errors.push(format!("{}: {error}", target.label)),
        }
    }

    prune_stale_fan_out_links(sink_system_name, &linked)?;

    if !errors.is_empty() {
        return Err(BackendError::Message(errors.join("; ")));
    }
    Ok(())
}

pub fn prune_stale_fan_out_links(
    sink_system_name: &str,
    allowed_targets: &HashSet<String>,
) -> Result<(), BackendError> {
    let routes = pw_link::list_all_monitor_routes_for_source(sink_system_name);
    for target_name in routes {
        if !allowed_targets.contains(&target_name) {
            pw_link::disconnect_sink_monitor_route(sink_system_name, &target_name)?;
        }
    }
    Ok(())
}

fn targets_of(device: &Device) -> Vec<String> {
    if !device.current_targets.is_empty() {
        device.current_targets.clone()
    } else if let Some(target) = &device.current_target {
        vec![target.clone()]
    } else {
        Vec::new()
    }
}

/// Walks the already-persisted routing graph forward from each proposed
/// target to see whether it can already reach back to `sink_device_id` —
/// i.e. whether applying this fan-out would close a loop (A -> B -> A).
/// Only virtual-output -> virtual-output chaining can introduce cycles, so
/// this only needs to follow existing sink targets, not stream routing.
fn would_create_cycle(graph: &RuntimeGraph, sink_device_id: &str, target_device_ids: &[String]) -> bool {
    let mut stack: Vec<String> = target_device_ids.to_vec();
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(current) = stack.pop() {
        if current == sink_device_id {
            return true;
        }
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(device) = graph.devices.iter().find(|entry| entry.id == current) {
            stack.extend(targets_of(device));
        }
    }
    false
}

fn validate_fan_out_target(device: &Device) -> Result<(), BackendError> {
    match device.direction {
        DeviceDirection::Output | DeviceDirection::Duplex => Ok(()),
        DeviceDirection::Input if device.kind == DeviceKind::Virtual => Ok(()),
        _ => Err(BackendError::Message(
            "fan-out targets must be outputs or virtual inputs".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::DeviceKind;

    fn sample_sink(id: &str, multi: bool) -> Device {
        Device {
            id: id.into(),
            system_name: format!("pipe-deck-{id}"),
            label: id.into(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Output,
            sink_mode: Some(if multi {
                crate::core::models::SinkMode::Multi
            } else {
                crate::core::models::SinkMode::Single
            }),
            virtual_role: Some(VirtualRole::Bus),
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        }
    }

    #[test]
    fn allows_clearing_all_sink_targets() {
        let graph = RuntimeGraph {
            devices: vec![sample_sink("bus", true)],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        };
        match apply_sink_targets(&graph, "bus", &[]) {
            Ok(()) => {}
            Err(error) => {
                assert!(!error.to_string().contains("at least one"));
            }
        }
    }

    #[test]
    fn virtual_mic_input_is_a_valid_fan_out_target() {
        let mic = Device {
            id: "mic".into(),
            system_name: "pipe-deck-mic".into(),
            label: "Mic".into(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Input,
            sink_mode: None,
            virtual_role: None,
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        };
        assert!(validate_fan_out_target(&mic).is_ok());
    }

    #[test]
    fn physical_input_is_not_a_valid_fan_out_target() {
        let mic = Device {
            id: "mic".into(),
            system_name: "alsa_input.mic".into(),
            label: "Mic".into(),
            kind: DeviceKind::Physical,
            direction: DeviceDirection::Input,
            sink_mode: None,
            virtual_role: None,
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        };
        assert!(validate_fan_out_target(&mic).is_err());
    }

    #[test]
    fn rejects_non_virtual_sink_fan_out() {
        let graph = RuntimeGraph {
            devices: vec![Device {
                id: "hw".into(),
                system_name: "alsa_output.test".into(),
                label: "Speakers".into(),
                kind: DeviceKind::Physical,
                direction: DeviceDirection::Output,
                sink_mode: None,
                virtual_role: None,
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
            }],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
            ..Default::default()
        };
        let error = apply_sink_targets(&graph, "hw", &["node-2".into()])
            .expect_err("physical device cannot fan out");
        assert!(error.to_string().contains("virtual bus devices"));
    }

    #[test]
    fn terminal_output_rejects_fan_out() {
        // #287: a terminal Output (virtual) is a true dead end — unlike a
        // Bus, it must never be a valid fan-out source.
        let mut terminal = sample_sink("terminal", false);
        terminal.virtual_role = Some(VirtualRole::Output);
        let graph = RuntimeGraph {
            devices: vec![terminal],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
            ..Default::default()
        };
        let error = apply_sink_targets(&graph, "terminal", &["node-2".into()])
            .expect_err("terminal output cannot fan out");
        assert!(error.to_string().contains("virtual bus devices"));
    }

    #[test]
    fn rejects_fan_out_that_would_create_a_cycle() {
        let mut submix = sample_sink("submix", false);
        let mut master = sample_sink("master", false);
        master.current_target = Some("submix".into());
        submix.current_target = None;

        let graph = RuntimeGraph {
            devices: vec![submix, master],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
            ..Default::default()
        };

        // "master" already routes to "submix"; routing "submix" back to
        // "master" would close the loop.
        let error = apply_sink_targets(&graph, "submix", &["master".into()])
            .expect_err("cycle must be rejected");
        assert!(error.to_string().contains("cycle"));
    }
}
