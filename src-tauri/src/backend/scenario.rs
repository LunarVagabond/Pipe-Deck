//! Loads a demo scenario file (`docs/specs/Demo_Scenario_Spec.md`) and
//! expands it into a full `RuntimeGraph` for `MockAudioBackend` (issue #368).
//!
//! A scenario file authors only the facts that can't be derived — devices,
//! streams, and the routes between them — and leaves everything mechanical
//! (`current_target`/`current_targets`, `links`, `is_monitor`) to
//! `expand_scenario`. Reproducing those fields by hand in every scenario
//! file would recreate exactly the drift-prone duplication issue #366
//! removed from the screenshot tooling.

use crate::core::models::{
    Device, DeviceDirection, DeviceKind, Link, MixSource, ProcessingNode, RuntimeGraph, SinkMode,
    Stream, StreamDirection,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct ScenarioFile {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub devices: Vec<ScenarioDevice>,
    #[serde(default)]
    pub streams: Vec<ScenarioStream>,
    #[serde(default)]
    pub routes: Vec<ScenarioRoute>,
    #[serde(default)]
    pub processing_nodes: Vec<ProcessingNode>,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioDevice {
    pub id: String,
    pub label: String,
    pub kind: DeviceKind,
    pub direction: DeviceDirection,
    #[serde(default)]
    pub sink_mode: Option<SinkMode>,
    #[serde(default)]
    pub mix_sources: Vec<ScenarioMixSource>,
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioMixSource {
    pub device_id: String,
    #[serde(default = "default_mix_volume")]
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioStream {
    pub id: String,
    pub app_name: String,
    #[serde(default)]
    pub executable: Option<String>,
    pub direction: StreamDirection,
    #[serde(default)]
    pub media_name: Option<String>,
    #[serde(default)]
    pub window_class: Option<String>,
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioRoute {
    pub from: String,
    pub to: String,
}

fn default_volume_percent() -> u8 {
    70
}

fn default_mix_volume() -> u8 {
    100
}

pub fn load_scenario_file(path: &Path) -> Result<RuntimeGraph, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read scenario file {}: {error}", path.display()))?;
    let scenario: ScenarioFile = serde_yaml::from_str(&raw)
        .map_err(|error| format!("failed to parse scenario file {}: {error}", path.display()))?;
    expand_scenario(scenario)
}

/// Validates and expands a parsed scenario into a full `RuntimeGraph`. See
/// `docs/specs/Demo_Scenario_Spec.md` for the schema and the fields this
/// derives rather than requires the author to write.
pub fn expand_scenario(scenario: ScenarioFile) -> Result<RuntimeGraph, String> {
    if scenario.version != 1 {
        return Err(format!(
            "unsupported scenario version: {}",
            scenario.version
        ));
    }

    let mut known_ids: HashSet<&str> = HashSet::new();
    for device in &scenario.devices {
        if !known_ids.insert(device.id.as_str()) {
            return Err(format!("duplicate id in scenario: {}", device.id));
        }
    }
    for stream in &scenario.streams {
        if !known_ids.insert(stream.id.as_str()) {
            return Err(format!("duplicate id in scenario: {}", stream.id));
        }
    }

    let device_ids: HashSet<&str> = scenario
        .devices
        .iter()
        .map(|device| device.id.as_str())
        .collect();
    for device in &scenario.devices {
        for mix_source in &device.mix_sources {
            if !device_ids.contains(mix_source.device_id.as_str()) {
                return Err(format!(
                    "scenario device '{}' has a mix_sources entry referencing unknown device '{}'",
                    device.id, mix_source.device_id
                ));
            }
        }
    }

    for route in &scenario.routes {
        if !known_ids.contains(route.from.as_str()) {
            return Err(format!("route references unknown id: {}", route.from));
        }
        if !known_ids.contains(route.to.as_str()) {
            return Err(format!("route references unknown id: {}", route.to));
        }
    }

    // Same rule the live backend uses (`graph_routing.rs`): a link is a
    // "monitor" fan-out when its source is a virtual *output* device, not a
    // direct app/stream routing target or an input-side mic-mix merge.
    let virtual_output_ids: HashSet<&str> = scenario
        .devices
        .iter()
        .filter(|device| {
            device.kind == DeviceKind::Virtual && device.direction == DeviceDirection::Output
        })
        .map(|device| device.id.as_str())
        .collect();

    let mut targets_by_id: HashMap<&str, Vec<String>> = HashMap::new();
    for route in &scenario.routes {
        targets_by_id
            .entry(route.from.as_str())
            .or_default()
            .push(route.to.clone());
    }

    let devices = scenario
        .devices
        .iter()
        .map(|device| {
            let targets = targets_by_id
                .get(device.id.as_str())
                .cloned()
                .unwrap_or_default();
            Device {
                id: device.id.clone(),
                system_name: device.id.clone(),
                label: device.label.clone(),
                kind: device.kind.clone(),
                direction: device.direction.clone(),
                sink_mode: device.sink_mode.clone(),
                volume_percent: Some(device.volume_percent),
                muted: Some(device.muted),
                current_target: targets.first().cloned(),
                current_targets: targets,
                mix_sources: device
                    .mix_sources
                    .iter()
                    .map(|mix_source| MixSource {
                        device_id: mix_source.device_id.clone(),
                        volume_percent: mix_source.volume_percent,
                        muted: mix_source.muted,
                    })
                    .collect(),
                sample_rate: None,
                channels: None,
            }
        })
        .collect();

    let streams = scenario
        .streams
        .iter()
        .map(|stream| Stream {
            id: stream.id.clone(),
            app_name: stream.app_name.clone(),
            executable: stream.executable.clone(),
            window_class: stream.window_class.clone(),
            system_name: Some(stream.id.clone()),
            direction: stream.direction.clone(),
            current_target: targets_by_id
                .get(stream.id.as_str())
                .and_then(|targets| targets.first().cloned()),
            media_name: stream.media_name.clone(),
            is_system: false,
            volume_percent: Some(stream.volume_percent),
            muted: Some(stream.muted),
            route_explanation: None,
            sample_rate: None,
            channels: None,
        })
        .collect();

    let links = scenario
        .routes
        .iter()
        .map(|route| Link {
            id: format!("link-{}-{}", route.from, route.to),
            source_id: route.from.clone(),
            target_id: route.to.clone(),
            is_monitor: virtual_output_ids.contains(route.from.as_str()),
        })
        .collect();

    Ok(RuntimeGraph {
        devices,
        streams,
        links,
        data_source: "mock".into(),
        notice: Some(format!(
            "Scenario: {}. Unset PIPE_DECK_MOCK_SCENARIO to use the default sample graph.",
            scenario.name
        )),
        recent_stream_identities: Vec::new(),
        processing_nodes: scenario.processing_nodes,
        default_output_system_name: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_scenario() -> ScenarioFile {
        ScenarioFile {
            version: 1,
            id: "test".into(),
            name: "Test scenario".into(),
            description: None,
            devices: vec![
                ScenarioDevice {
                    id: "sink-a".into(),
                    label: "Sink A".into(),
                    kind: DeviceKind::Virtual,
                    direction: DeviceDirection::Output,
                    sink_mode: Some(SinkMode::Single),
                    mix_sources: Vec::new(),
                    volume_percent: 70,
                    muted: false,
                },
                ScenarioDevice {
                    id: "sink-b".into(),
                    label: "Headphones".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    mix_sources: Vec::new(),
                    volume_percent: 70,
                    muted: false,
                },
            ],
            streams: vec![ScenarioStream {
                id: "stream-a".into(),
                app_name: "App A".into(),
                executable: Some("app-a".into()),
                direction: StreamDirection::Playback,
                media_name: None,
                window_class: None,
                volume_percent: 70,
                muted: false,
            }],
            routes: vec![
                ScenarioRoute {
                    from: "stream-a".into(),
                    to: "sink-a".into(),
                },
                ScenarioRoute {
                    from: "sink-a".into(),
                    to: "sink-b".into(),
                },
            ],
            processing_nodes: Vec::new(),
        }
    }

    #[test]
    fn expands_routes_into_links_and_current_targets() {
        let graph = expand_scenario(base_scenario()).unwrap();

        assert_eq!(graph.links.len(), 2);
        assert_eq!(graph.data_source, "mock");

        let sink_a = graph
            .devices
            .iter()
            .find(|device| device.id == "sink-a")
            .unwrap();
        assert_eq!(sink_a.current_target.as_deref(), Some("sink-b"));
        assert_eq!(sink_a.current_targets, vec!["sink-b".to_string()]);

        let stream_a = graph
            .streams
            .iter()
            .find(|stream| stream.id == "stream-a")
            .unwrap();
        assert_eq!(stream_a.current_target.as_deref(), Some("sink-a"));
    }

    #[test]
    fn marks_virtual_output_fan_out_links_as_monitor() {
        let graph = expand_scenario(base_scenario()).unwrap();

        let stream_to_sink = graph
            .links
            .iter()
            .find(|link| link.source_id == "stream-a")
            .unwrap();
        assert!(
            !stream_to_sink.is_monitor,
            "a stream's direct route is never a monitor fan-out"
        );

        let sink_to_output = graph
            .links
            .iter()
            .find(|link| link.source_id == "sink-a")
            .unwrap();
        assert!(
            sink_to_output.is_monitor,
            "a virtual output device's fan-out is a monitor link"
        );
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut scenario = base_scenario();
        scenario.version = 2;
        let error = expand_scenario(scenario).unwrap_err();
        assert!(error.contains("unsupported scenario version"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut scenario = base_scenario();
        scenario.streams[0].id = "sink-a".into();
        let error = expand_scenario(scenario).unwrap_err();
        assert!(error.contains("duplicate id"));
    }

    #[test]
    fn rejects_dangling_route_reference() {
        let mut scenario = base_scenario();
        scenario.routes.push(ScenarioRoute {
            from: "sink-b".into(),
            to: "does-not-exist".into(),
        });
        let error = expand_scenario(scenario).unwrap_err();
        assert!(error.contains("unknown id"));
    }

    #[test]
    fn rejects_dangling_mix_source_reference() {
        let mut scenario = base_scenario();
        scenario.devices[0].mix_sources.push(ScenarioMixSource {
            device_id: "does-not-exist".into(),
            volume_percent: 100,
            muted: false,
        });
        let error = expand_scenario(scenario).unwrap_err();
        assert!(error.contains("mix_sources"));
    }

    #[test]
    fn load_scenario_file_reports_missing_file() {
        let error =
            load_scenario_file(Path::new("/nonexistent/pipe-deck-scenario.yaml")).unwrap_err();
        assert!(error.contains("failed to read scenario file"));
    }
}
