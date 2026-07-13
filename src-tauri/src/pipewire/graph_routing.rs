use crate::core::models::{
    DeviceDirection, DeviceKind, Link, MixSource, RuntimeGraph, StreamDirection,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::{stream_identity_key, StreamIdentityKey};
use crate::pipewire::graph_enrich::{apply_pactl_capture_targets, apply_pactl_playback_targets};
use crate::pipewire::pactl;
use crate::pipewire::pw_link;
use std::collections::{HashMap, HashSet};

pub fn sync_live_routing_graph(graph: &mut RuntimeGraph) {
    gc_feed_sinks(graph);
    apply_pactl_playback_targets(graph);
    apply_pactl_capture_targets(graph);
    apply_pw_link_device_routes(graph);
    apply_virtual_mic_mix_routes(graph);
    normalize_stream_routing_links(graph);

    for stream in &mut graph.streams {
        stream.route_explanation = None;
    }
}

/// Keep user-cleared routes off the graph even when PipeWire still has an active link.
pub fn apply_user_cleared_routes(
    graph: &mut RuntimeGraph,
    cleared_streams: &HashSet<StreamIdentityKey>,
    cleared_devices: &HashSet<String>,
) {
    if cleared_streams.is_empty() && cleared_devices.is_empty() {
        return;
    }

    for stream in &mut graph.streams {
        if cleared_streams.contains(&stream_identity_key(stream)) {
            stream.current_target = None;
            stream.current_targets.clear();
        }
    }

    for device in &mut graph.devices {
        if cleared_devices.contains(&device.id) {
            device.current_target = None;
            device.current_targets.clear();
        }
    }

    graph.links.retain(|link| {
        if cleared_devices.contains(&link.source_id)
            && (link.id.starts_with("pwlink-") || link.id.starts_with("route-device-"))
        {
            return false;
        }
        true
    });

    normalize_stream_routing_links(graph);
}

pub fn apply_graph_routing(graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>) {
    sync_live_routing_graph(graph);
    let _ = crate::core::routing_rules::apply_persisted_routing_rules(graph, ctx);
    apply_pactl_playback_targets(graph);
    normalize_stream_routing_links(graph);
}

fn gc_feed_sinks(graph: &RuntimeGraph) {
    let known_virtual_inputs: HashSet<String> = graph
        .devices
        .iter()
        .filter(|device| {
            device.direction == DeviceDirection::Input
                && device.kind == DeviceKind::Virtual
                && device.system_name.starts_with("pipe-deck-")
        })
        .map(|device| device.system_name.clone())
        .collect();

    let _ = pactl::gc_feed_sinks(&known_virtual_inputs);
}

pub fn normalize_stream_routing_links(graph: &mut RuntimeGraph) {
    let playback_stream_ids: HashSet<String> = graph
        .streams
        .iter()
        .filter(|stream| stream.direction == StreamDirection::Playback)
        .map(|stream| stream.id.clone())
        .collect();

    let capture_stream_ids: HashSet<String> = graph
        .streams
        .iter()
        .filter(|stream| stream.direction == StreamDirection::Capture)
        .map(|stream| stream.id.clone())
        .collect();

    graph.links.retain(|link| {
        if link.id.starts_with("route-stream-") || link.id.starts_with("route-capture-") {
            return false;
        }
        if playback_stream_ids.contains(&link.source_id) {
            return false;
        }
        if capture_stream_ids.contains(&link.target_id) {
            return false;
        }
        true
    });

    for stream in &graph.streams {
        let Some(target_id) = &stream.current_target else {
            continue;
        };

        match stream.direction {
            StreamDirection::Playback => {
                graph.links.push(Link {
                    id: format!("route-stream-{}", stream.id),
                    source_id: stream.id.clone(),
                    target_id: target_id.clone(),
                });
            }
            StreamDirection::Capture => {
                graph.links.push(Link {
                    id: format!("route-capture-{}", stream.id),
                    source_id: target_id.clone(),
                    target_id: stream.id.clone(),
                });
            }
        }
    }
}

fn apply_pw_link_device_routes(graph: &mut RuntimeGraph) {
    let routes = pw_link::list_monitor_routes();
    let name_to_id: HashMap<String, String> = graph
        .devices
        .iter()
        .map(|device| (device.system_name.clone(), device.id.clone()))
        .collect();

    for device in &mut graph.devices {
        if device.direction == DeviceDirection::Output && device.kind == DeviceKind::Virtual {
            device.current_target = None;
            device.current_targets.clear();
        }
    }

    graph.links.retain(|link| !link.id.starts_with("pwlink-"));

    for (source_name, target_name) in routes {
        let Some(source_id) = name_to_id.get(&source_name) else {
            continue;
        };
        let Some(target_id) = name_to_id.get(&target_name) else {
            continue;
        };

        let source_is_virtual = graph.devices.iter().any(|device| {
            device.id == *source_id
                && device.kind == DeviceKind::Virtual
                && device.direction == DeviceDirection::Output
        });
        if !source_is_virtual {
            continue;
        }

        if let Some(device) = graph.devices.iter_mut().find(|device| device.id == *source_id) {
            if device.is_multi_sink() {
                if !device.current_targets.contains(target_id) {
                    device.current_targets.push(target_id.clone());
                }
                if device.current_target.is_none() {
                    device.current_target = Some(target_id.clone());
                }
            } else {
                device.current_target = Some(target_id.clone());
                device.current_targets = vec![target_id.clone()];
            }
        }

        graph.links.push(Link {
            id: format!("pwlink-{source_name}-{target_name}"),
            source_id: source_id.clone(),
            target_id: target_id.clone(),
        });
    }

    for device in &mut graph.devices {
        if !device.is_multi_sink() {
            continue;
        }
        let fan_out_names = pw_link::list_all_monitor_routes_for_source(&device.system_name);
        let target_ids: Vec<String> = fan_out_names
            .iter()
            .filter_map(|name| name_to_id.get(name).cloned())
            .collect();
        if !target_ids.is_empty() {
            device.current_targets = target_ids.clone();
            device.current_target = target_ids.first().cloned();
        }
    }
}

/// Each mix source is fed through its own per-pair feed sink
/// (`pipe-deck-feed-{mic}-{source}`, see `pactl::ensure_feed_sink_for_mix_pair`)
/// whose monitor is linked into the mic's input ports, so an independent
/// gain per source can be read back from the feed sink's own volume. This
/// walks: mic's input ports -> feed sink names -> feed sink's playback ports
/// -> the physical capture source actually feeding it.
fn apply_virtual_mic_mix_routes(graph: &mut RuntimeGraph) {
    let name_to_id: HashMap<String, String> = graph
        .devices
        .iter()
        .map(|device| (device.system_name.clone(), device.id.clone()))
        .collect();

    graph.links.retain(|link| !link.id.starts_with("pwlink-mix-"));

    for device in &mut graph.devices {
        if device.kind != DeviceKind::Virtual
            || device.direction == DeviceDirection::Duplex
            || !device.system_name.starts_with("pipe-deck-")
        {
            continue;
        }

        let feed_sink_names = pw_link::list_capture_sources_for_virtual_input(&device.system_name);
        let mut mix_sources = Vec::new();

        for feed_sink_name in &feed_sink_names {
            if !feed_sink_name.starts_with("pipe-deck-feed-") {
                continue;
            }

            let Some(source_name) = pw_link::list_capture_sources_for_sink(feed_sink_name).into_iter().next() else {
                continue;
            };
            let Some(source_id) = name_to_id.get(&source_name) else {
                continue;
            };

            let volume_percent = pactl::sink_volume_percent(feed_sink_name)
                .ok()
                .flatten()
                .unwrap_or(100);

            mix_sources.push(MixSource {
                device_id: source_id.clone(),
                volume_percent,
            });

            graph.links.push(Link {
                id: format!("pwlink-mix-{source_name}-{}", device.system_name),
                source_id: source_id.clone(),
                target_id: device.id.clone(),
            });
        }

        device.mix_sources = mix_sources;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Device, Stream};
    use crate::pipewire::stream_match::resolve_playback_target_device_id;

    #[test]
    fn feed_sink_maps_to_virtual_input_target() {
        let mut graph = RuntimeGraph {
            devices: vec![Device {
                id: "virtual-test".into(),
                system_name: "pipe-deck-test".into(),
                label: "test".into(),
                kind: DeviceKind::Virtual,
                direction: DeviceDirection::Input,
                sink_mode: None,
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
            }],
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        };

        let target = resolve_playback_target_device_id(&graph, "pipe-deck-feed-test");
        assert_eq!(target.as_deref(), Some("virtual-test"));

        graph.streams.push(Stream {
            id: "node-42".into(),
            app_name: "Firefox".into(),
            executable: Some("firefox".into()),
            window_class: None,
            system_name: Some("Firefox".into()),
            direction: StreamDirection::Playback,
            current_target: target.clone(),
            current_targets: target.clone().into_iter().collect(),
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
        });
        normalize_stream_routing_links(&mut graph);

        assert_eq!(graph.links.len(), 1);
        assert_eq!(graph.links[0].source_id, "node-42");
        assert_eq!(graph.links[0].target_id, "virtual-test");
    }

    #[test]
    fn normalize_stream_routing_links_removes_stale_pw_dump_edges() {
        let mut graph = RuntimeGraph {
            devices: vec![
                Device {
                    id: "hdmi".into(),
                    system_name: "alsa_output.hdmi".into(),
                    label: "HDMI".into(),
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
                    id: "headset".into(),
                    system_name: "alsa_output.headset".into(),
                    label: "Headset".into(),
                    kind: DeviceKind::Physical,
                    direction: DeviceDirection::Output,
                    sink_mode: None,
                    volume_percent: None,
                    muted: None,
                    current_target: None,
                    current_targets: Vec::new(),
                    mix_sources: Vec::new(),
                },
            ],
            streams: vec![Stream {
                id: "firefox".into(),
                app_name: "Firefox".into(),
                executable: Some("firefox".into()),
                window_class: None,
                system_name: Some("Firefox".into()),
                direction: StreamDirection::Playback,
                current_target: Some("headset".into()),
                current_targets: Vec::new(),
                media_name: None,
                is_system: false,
                volume_percent: None,
                muted: None,
                route_explanation: None,
            }],
            links: vec![Link {
                id: "link-stale".into(),
                source_id: "firefox".into(),
                target_id: "hdmi".into(),
            }],
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        };

        normalize_stream_routing_links(&mut graph);

        assert_eq!(graph.links.len(), 1);
        assert_eq!(graph.links[0].source_id, "firefox");
        assert_eq!(graph.links[0].target_id, "headset");
        assert!(graph.links[0].id.starts_with("route-stream-"));
    }
}
