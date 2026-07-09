use crate::config::store::ConfigStore;
use crate::core::models::{
    Device, DeviceDirection, DeviceKind, DeviceRouteRule, RuntimeGraph, Stream, StreamRouteRule,
};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::{pactl, pw_link};

pub fn apply_persisted_routing_rules(graph: &mut RuntimeGraph) -> Result<(), AdapterError> {
    let rules = ConfigStore::new().routing_rules();

    for rule in &rules.stream_rules {
        let matches: Vec<(String, String)> = active_streams_matching(graph, rule)
            .iter()
            .filter_map(|stream| {
                find_device_by_system_name(graph, &rule.target_system_name)
                    .map(|target| (stream.id.clone(), target.id.clone()))
            })
            .collect();

        for (stream_id, target_id) in matches {
            let Some(stream) = graph.streams.iter().find(|stream| stream.id == stream_id) else {
                continue;
            };
            let Some(target) = graph.devices.iter().find(|device| device.id == target_id) else {
                continue;
            };
            if apply_stream_to_target(graph, stream, target).is_ok() {
                if let Some(stream) = graph.streams.iter_mut().find(|stream| stream.id == stream_id)
                {
                    stream.current_target = Some(target_id);
                }
            }
        }
    }

    for rule in &rules.device_rules {
        if let Some(source) = find_device_by_system_name(graph, &rule.source_system_name) {
            if source.kind != DeviceKind::Virtual || source.direction != DeviceDirection::Output {
                continue;
            }
            if let Some(target) = find_device_by_system_name(graph, &rule.target_system_name) {
                let source_id = source.id.clone();
                let target_id = target.id.clone();
                let already = source
                    .current_target
                    .as_ref()
                    .is_some_and(|id| id == &target_id)
                    || pw_link::is_sink_monitor_routed_to(
                        &source.system_name,
                        &target.system_name,
                        target.direction == DeviceDirection::Input,
                    );
                let routed = if already {
                    true
                } else {
                    pw_link::link_sink_monitor_to_target(
                        &source.system_name,
                        &target.system_name,
                        target.direction == DeviceDirection::Input,
                    )
                    .is_ok()
                };
                if routed {
                    if let Some(device) = graph
                        .devices
                        .iter_mut()
                        .find(|device| device.id == source_id)
                    {
                        device.current_target = Some(target_id);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn save_stream_route_rule(stream: &Stream, target: &Device) -> Result<(), AdapterError> {
    let mut rules = ConfigStore::new().routing_rules();
    rules.stream_rules.retain(|rule| rule.app_name != stream.app_name);
    rules.stream_rules.push(StreamRouteRule {
        app_name: stream.app_name.clone(),
        media_name: None,
        target_system_name: target.system_name.clone(),
    });
    ConfigStore::new()
        .save_routing_rules(&rules)
        .map_err(|error| AdapterError::Message(error.to_string()))
}

pub fn save_device_route_rule(source: &Device, target: &Device) -> Result<(), AdapterError> {
    let mut rules = ConfigStore::new().routing_rules();
    rules
        .device_rules
        .retain(|rule| rule.source_system_name != source.system_name);
    rules.device_rules.push(DeviceRouteRule {
        source_system_name: source.system_name.clone(),
        target_system_name: target.system_name.clone(),
    });
    ConfigStore::new()
        .save_routing_rules(&rules)
        .map_err(|error| AdapterError::Message(error.to_string()))
}

pub fn apply_stream_to_target(
    graph: &RuntimeGraph,
    stream: &Stream,
    target: &Device,
) -> Result<(), AdapterError> {
    pactl::move_stream_to_target(graph, &stream.id, &target.id)
}

fn active_streams_matching<'a>(
    graph: &'a RuntimeGraph,
    rule: &StreamRouteRule,
) -> Vec<&'a Stream> {
    graph
        .streams
        .iter()
        .filter(|stream| stream_matches_rule(stream, rule))
        .collect()
}

fn stream_matches_rule(stream: &Stream, rule: &StreamRouteRule) -> bool {
    if stream.app_name != rule.app_name {
        return false;
    }

    match (&rule.media_name, &stream.media_name) {
        (Some(rule_media), Some(stream_media)) => rule_media == stream_media,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn find_device_by_system_name<'a>(
    graph: &'a RuntimeGraph,
    system_name: &str,
) -> Option<&'a Device> {
    graph
        .devices
        .iter()
        .find(|device| device.system_name == system_name)
}
