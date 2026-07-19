use crate::config::store::ConfigStore;
use crate::core::models::{
    Device, DeviceRouteRule, RuntimeGraph, Stream, StreamRouteRule,
};
use crate::core::stream_identity::{rule_identity_key, stream_identity_key};
use crate::backend::BackendError;
use crate::backend::linux::split_sink;

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

pub fn clear_device_route_rule(source: &Device) -> Result<(), BackendError> {
    let mut rules = ConfigStore::new().routing_rules();
    rules
        .device_rules
        .retain(|rule| rule.source_system_name != source.system_name);
    ConfigStore::new()
        .save_routing_rules(&rules)
        .map_err(|error| BackendError::Message(error.to_string()))
}

pub fn save_stream_route_rule(stream: &Stream, target: &Device) -> Result<(), BackendError> {
    let mut rules = ConfigStore::new().routing_rules();
    let identity = stream_identity_key(stream);
    rules.stream_rules.retain(|rule| rule_identity_key(rule) != identity);
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

pub fn save_device_route_rule(source: &Device, targets: &[Device]) -> Result<(), BackendError> {
    if targets.is_empty() {
        return Ok(());
    }
    let mut rules = ConfigStore::new().routing_rules();
    let existing_safeguards = rules
        .device_rules
        .iter()
        .find(|rule| rule.source_system_name == source.system_name)
        .map(|rule| rule.safeguards.clone())
        .unwrap_or_default();
    rules
        .device_rules
        .retain(|rule| rule.source_system_name != source.system_name);
    rules.device_rules.push(DeviceRouteRule {
        source_system_name: source.system_name.clone(),
        target_system_name: targets.first().map(|device| device.system_name.clone()),
        target_system_names: targets.iter().map(|device| device.system_name.clone()).collect(),
        safeguards: existing_safeguards,
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

pub fn apply_sink_to_targets(
    graph: &RuntimeGraph,
    sink_device_id: &str,
    target_device_ids: &[String],
) -> Result<(), BackendError> {
    split_sink::apply_sink_targets(graph, sink_device_id, target_device_ids)
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
    use crate::core::models::{DeviceDirection, DeviceKind, SinkMode, StreamDirection};

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

    #[test]
    fn save_device_route_rule_stores_multiple_targets() {
        let _guard = crate::config::store::lock_config_dir_env();
        let dir = std::env::temp_dir().join(format!("pipe-deck-rules-{}", std::process::id()));
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &dir);
        let _ = std::fs::remove_dir_all(&dir);
        let store = ConfigStore::new();
        store.ensure_layout().unwrap();
        let source = Device {
            id: "multi-bus".into(),
            system_name: "pipe-deck-multi-bus".into(),
            label: "Multi Bus".into(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Output,
            sink_mode: Some(SinkMode::Multi),
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        };
        let targets = vec![
            Device {
                id: "headphones".into(),
                system_name: "alsa-headphones".into(),
                label: "Headphones".into(),
                kind: DeviceKind::Physical,
                direction: DeviceDirection::Output,
                sink_mode: None,
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
            },
            Device {
                id: "speakers".into(),
                system_name: "alsa-speakers".into(),
                label: "Speakers".into(),
                kind: DeviceKind::Physical,
                direction: DeviceDirection::Output,
                sink_mode: None,
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
            },
        ];
        save_device_route_rule(&source, &targets).unwrap();
        let rules = ConfigStore::new().routing_rules();
        assert_eq!(rules.device_rules.len(), 1);
        assert_eq!(
            rules.device_rules[0].target_system_names,
            vec!["alsa-headphones".to_string(), "alsa-speakers".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("PIPE_DECK_CONFIG_DIR");
    }
}
