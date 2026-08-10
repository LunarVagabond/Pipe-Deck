use crate::backend::linux::split_sink;
use crate::backend::BackendError;
use crate::config::store::ConfigStore;
use crate::core::models::{Device, RuntimeGraph, Stream, StreamRouteRule};
use crate::core::stream_identity::{rule_identity_key, stream_identity_key};

pub fn apply_persisted_routing_rules(
    graph: &mut RuntimeGraph,
    ctx: &crate::core::rules::ApplyRulesContext<'_>,
) -> Result<(), BackendError> {
    crate::core::rules::apply_routing_rules_with_explanations(graph, ctx)
}

pub fn clear_stream_route_rule(stream: &Stream) -> Result<(), BackendError> {
    let mut rules = ConfigStore::new().routing_rules();
    let identity = stream_identity_key(stream);
    rules
        .stream_rules
        .retain(|rule| rule_identity_key(rule) != identity);
    ConfigStore::new()
        .save_routing_rules(&rules)
        .map_err(|error| BackendError::Message(error.to_string()))
}

pub fn save_stream_route_rule(stream: &Stream, target: &Device) -> Result<(), BackendError> {
    let mut rules = ConfigStore::new().routing_rules();
    let identity = stream_identity_key(stream);
    rules
        .stream_rules
        .retain(|rule| rule_identity_key(rule) != identity);
    rules.stream_rules.push(StreamRouteRule {
        app_name: Some(stream.app_name.clone()),
        executable: stream.executable.clone(),
        media_name: stream.media_name.clone(),
        target_system_name: Some(target.system_name.clone()),
        target_system_names: Vec::new(),
    });
    ConfigStore::new()
        .save_routing_rules(&rules)
        .map_err(|error| BackendError::Message(error.to_string()))
}

pub fn apply_stream_to_target(
    graph: &RuntimeGraph,
    stream: &Stream,
    target: &Device,
) -> Result<(), BackendError> {
    split_sink::apply_stream_to_sink(graph, &stream.id, &target.id)
}

pub fn apply_stream_to_sink_id(
    graph: &RuntimeGraph,
    stream: &Stream,
    target_device_id: &str,
) -> Result<(), BackendError> {
    split_sink::apply_stream_to_sink(graph, &stream.id, target_device_id)
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
            volume_percent: None,
            muted: None,
            route_explanation: None,
            sample_rate: None,
            channels: None,
        }
    }

    #[test]
    fn persisted_rule_matches_executable_only() {
        let stream = sample_stream("Discord Canary", Some("discord"), None);
        let rule = StreamRouteRule {
            app_name: None,
            executable: Some("discord".into()),
            media_name: None,
            target_system_name: Some("chat".into()),
            target_system_names: Vec::new(),
        };

        assert!(crate::core::rules::stream_matches_persisted_rule(&stream, &rule).is_some());
    }
}
