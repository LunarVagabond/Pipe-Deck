use crate::core::models::{Device, DeviceDirection, DeviceKind, RuntimeGraph};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl;
use crate::pipewire::pw_link;
use std::collections::HashSet;

pub fn apply_stream_to_sink(
    graph: &RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), AdapterError> {
    pactl::move_stream_to_target(graph, stream_id, target_device_id)
}

pub fn apply_sink_targets(
    graph: &RuntimeGraph,
    sink_device_id: &str,
    target_device_ids: &[String],
) -> Result<(), AdapterError> {
    if target_device_ids.is_empty() {
        return Err(AdapterError::Message("at least one sink target is required".into()));
    }

    let sink = graph
        .devices
        .iter()
        .find(|device| device.id == sink_device_id)
        .ok_or_else(|| AdapterError::Message(format!("sink device not found: {sink_device_id}")))?;

    if sink.kind != DeviceKind::Virtual || sink.direction != DeviceDirection::Output {
        return Err(AdapterError::Message(
            "only virtual output sinks can fan out to targets".into(),
        ));
    }

    if target_device_ids.len() == 1 && !sink.is_multi_sink() {
        let target = graph
            .devices
            .iter()
            .find(|device| device.id == target_device_ids[0])
            .ok_or_else(|| {
                AdapterError::Message(format!(
                    "target device not found: {}",
                    target_device_ids[0]
                ))
            })?;
        validate_fan_out_target(target)?;
        let target_is_virtual_source =
            target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
        pw_link::link_sink_monitor_to_target(
            &sink.system_name,
            &target.system_name,
            target_is_virtual_source,
        )?;
        return Ok(());
    }

    fan_out_sink(graph, &sink.system_name, target_device_ids)
}

pub fn fan_out_sink(
    graph: &RuntimeGraph,
    sink_system_name: &str,
    target_device_ids: &[String],
) -> Result<(), AdapterError> {
    let mut linked = HashSet::new();
    for target_id in target_device_ids {
        let target = graph
            .devices
            .iter()
            .find(|device| device.id == *target_id)
            .ok_or_else(|| AdapterError::Message(format!("target device not found: {target_id}")))?;
        validate_fan_out_target(target)?;
        let target_is_virtual_source =
            target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
        pw_link::link_sink_monitor_to_target(
            sink_system_name,
            &target.system_name,
            target_is_virtual_source,
        )?;
        linked.insert(target.system_name.clone());
    }

    prune_stale_fan_out_links(sink_system_name, &linked)?;
    Ok(())
}

pub fn prune_stale_fan_out_links(
    sink_system_name: &str,
    allowed_targets: &HashSet<String>,
) -> Result<(), AdapterError> {
    let routes = pw_link::list_all_monitor_routes_for_source(sink_system_name);
    for target_name in routes {
        if !allowed_targets.contains(&target_name) {
            pw_link::disconnect_sink_monitor_route(sink_system_name, &target_name)?;
        }
    }
    Ok(())
}

fn validate_fan_out_target(device: &Device) -> Result<(), AdapterError> {
    match device.direction {
        DeviceDirection::Output | DeviceDirection::Duplex => Ok(()),
        DeviceDirection::Input if device.kind == DeviceKind::Virtual => Ok(()),
        _ => Err(AdapterError::Message(
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
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
        }
    }

    #[test]
    fn rejects_empty_sink_targets() {
        let graph = RuntimeGraph {
            devices: vec![sample_sink("bus", true)],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
        };
        let error = apply_sink_targets(&graph, "bus", &[])
            .expect_err("empty targets should fail");
        assert!(error.to_string().contains("at least one"));
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
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
            }],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "mock".into(),
            notice: None,
        };
        let error = apply_sink_targets(&graph, "hw", &["node-2".into()])
            .expect_err("physical device cannot fan out");
        assert!(error.to_string().contains("virtual output sinks"));
    }
}
