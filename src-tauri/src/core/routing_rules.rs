use crate::config::store::ConfigStore;
use crate::core::models::{
    Device, DeviceRouteRule, RuntimeGraph, Stream, StreamRouteRule,
};
use crate::core::stream_identity::{rule_identity_key, stream_identity_key};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl;

pub fn apply_persisted_routing_rules(
    graph: &mut RuntimeGraph,
    ctx: &crate::core::rule_engine::ApplyRulesContext<'_>,
) -> Result<(), AdapterError> {
    crate::core::rule_engine::apply_routing_rules_with_explanations(graph, ctx)
}

pub fn save_stream_route_rule(stream: &Stream, target: &Device) -> Result<(), AdapterError> {
    let mut rules = ConfigStore::new().routing_rules();
    let identity = stream_identity_key(stream);
    rules.stream_rules.retain(|rule| rule_identity_key(rule) != identity);
    rules.stream_rules.push(StreamRouteRule {
        app_name: Some(stream.app_name.clone()),
        executable: stream.executable.clone(),
        media_name: stream.media_name.clone(),
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

pub fn find_device_by_system_name<'a>(
    graph: &'a RuntimeGraph,
    system_name: &str,
) -> Option<&'a Device> {
    graph
        .devices
        .iter()
        .find(|device| device.system_name == system_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::StreamDirection;
    use crate::core::rule_engine::stream_matches_persisted_rule;

    fn sample_stream(app_name: &str, executable: Option<&str>, media_name: Option<&str>) -> Stream {
        Stream {
            id: "stream-1".into(),
            app_name: app_name.into(),
            executable: executable.map(str::to_string),
            window_class: None,
            system_name: None,
            direction: StreamDirection::Playback,
            current_target: None,
            media_name: media_name.map(str::to_string),
            is_system: false,
            route_explanation: None,
        }
    }

    #[test]
    fn app_only_rule_matches_legacy_shape() {
        let stream = sample_stream("Firefox", Some("firefox"), None);
        let rule = StreamRouteRule {
            app_name: Some("Firefox".into()),
            executable: None,
            media_name: None,
            target_system_name: "browser".into(),
        };

        assert!(stream_matches_persisted_rule(&stream, &rule).is_some());
    }

    #[test]
    fn executable_rule_does_not_false_positive() {
        let stream = sample_stream("Discord", Some("discord"), None);
        let rule = StreamRouteRule {
            app_name: None,
            executable: Some("spotify".into()),
            media_name: None,
            target_system_name: "music".into(),
        };

        assert!(stream_matches_persisted_rule(&stream, &rule).is_none());
    }
}
