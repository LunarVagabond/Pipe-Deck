use crate::core::models::{
    DeviceDirection, DeviceKind, Link, MixSource, RuntimeGraph, StreamDirection,
};
use crate::core::rules::ApplyRulesContext;
use crate::core::stream_identity::{stream_identity_key, StreamIdentityKey};
use crate::backend::linux::pactl;
use crate::backend::linux::pw_link;
use crate::backend::linux::split_sink::effective_fan_out_source;
use crate::backend::linux::stream_match::{resolve_capture_target_device_id, resolve_playback_target_device_id};
use std::collections::{HashMap, HashSet};

pub(super) fn sync_live_routing_graph(graph: &mut RuntimeGraph) {
    gc_feed_sinks(graph);
    apply_native_playback_targets(graph);
    apply_native_capture_targets(graph);
    apply_pw_link_device_routes(graph);
    apply_virtual_mic_mix_routes(graph);
    normalize_stream_routing_links(graph);

    for stream in &mut graph.streams {
        stream.route_explanation = None;
    }
}

/// Native (`pw-link`-derived) replacement for the old `pactl`
/// sink-index-based `apply_pactl_playback_targets` (#432, Gap 1) — this is
/// the call site that actually matters for target resolution: it runs after
/// `merge_virtual_devices`/`merge_processing_nodes` (`graph_sync.rs`), so
/// `graph.devices`/`graph.processing_nodes` already include every virtual
/// device and processing node a stream could be routed to, unlike
/// `pw_dump::normalize`'s own link-derived `current_target` (which only ever
/// sees the plain hardware/software devices `pw-dump` itself enumerates,
/// since virtual devices/processing nodes are merged in later). Resolves
/// each playback stream's target by asking what its own output ports are
/// actually linked to right now (`pw_link::list_all_monitor_routes_for_source`,
/// which already has native+CLI fallback built in) and feeding that name
/// through the *same* [`resolve_playback_target_device_id`] the old pactl
/// path used — so every existing special case (processing nodes, a Mixer's
/// per-input feed sink) still applies unchanged.
fn apply_native_playback_targets(graph: &mut RuntimeGraph) {
    let mut updates: Vec<(String, String)> = Vec::new();

    for stream in &graph.streams {
        if stream.direction != StreamDirection::Playback {
            continue;
        }
        let Some(system_name) = &stream.system_name else {
            continue;
        };
        let Some(target_name) = pw_link::list_all_monitor_routes_for_source(system_name).into_iter().next() else {
            continue;
        };
        let Some(target_id) = resolve_playback_target_device_id(graph, &target_name) else {
            continue;
        };
        updates.push((stream.id.clone(), target_id));
    }

    for (stream_id, target_id) in updates {
        if let Some(stream) = graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
            stream.current_target = Some(target_id);
        }
    }
}

/// Capture-direction counterpart to [`apply_native_playback_targets`] —
/// same reasoning, using `pw_link::list_capture_sources_for_stream` (an
/// empty-prefix port lookup, since a stream's own ports follow no Pipe Deck
/// naming convention) to find what currently feeds a capture stream's input
/// ports, then [`resolve_capture_target_device_id`] to translate that into a
/// device id.
fn apply_native_capture_targets(graph: &mut RuntimeGraph) {
    let mut updates: Vec<(String, String)> = Vec::new();

    for stream in &graph.streams {
        if stream.direction != StreamDirection::Capture {
            continue;
        }
        let Some(system_name) = &stream.system_name else {
            continue;
        };
        let Some(source_name) = pw_link::list_capture_sources_for_stream(system_name).into_iter().next() else {
            continue;
        };
        let Some(target_id) = resolve_capture_target_device_id(graph, &source_name) else {
            continue;
        };
        updates.push((stream.id.clone(), target_id));
    }

    for (stream_id, target_id) in updates {
        if let Some(stream) = graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
            stream.current_target = Some(target_id);
        }
    }
}

/// Keep user-cleared routes off the graph even when PipeWire still has an active link.
pub(in crate::backend) fn apply_user_cleared_routes(
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

pub(super) fn apply_graph_routing(graph: &mut RuntimeGraph, ctx: &ApplyRulesContext<'_>) {
    sync_live_routing_graph(graph);
    let _ = crate::core::routing_rules::apply_persisted_routing_rules(graph, ctx);
    apply_native_playback_targets(graph);
    normalize_stream_routing_links(graph);
}

fn gc_feed_sinks(graph: &RuntimeGraph) {
    // A per-pair mix-source feed sink (`pipe-deck-feed-{owner}-{source}`) can
    // now be owned by either a virtual-mic `Device` (the original mechanism)
    // or a Mixer processing node (PD-032, which generalizes it) — both need
    // to be in this set, or `pactl::gc_feed_sinks`'s `is_per_pair_mix_feed_sink`
    // guard won't recognize the Mixer's feed sinks as legitimately owned and
    // will tear them straight back down on the very next refresh (caught by
    // `mixer_node_round_trips_on_a_real_pipewire_session` — the exact
    // "existing call site doesn't know about the new owner" failure mode
    // PD-032 exists to guard against, not hypothetical).
    let known_virtual_inputs = graph
        .devices
        .iter()
        .filter(|device| {
            device.direction == DeviceDirection::Input
                && device.kind == DeviceKind::Virtual
                && device.system_name.starts_with("pipe-deck-")
        })
        .map(|device| device.system_name.clone());
    let known_mixer_nodes = graph
        .processing_nodes
        .iter()
        .filter(|node| matches!(node.kind, crate::core::models::ProcessingNodeKind::Mixer { .. }))
        .map(|node| node.system_name.clone());

    let known_feed_owners: HashSet<String> = known_virtual_inputs.chain(known_mixer_nodes).collect();

    let _ = pactl::gc_feed_sinks(&known_feed_owners);
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

    // A stream's `current_target` resolving to a processing node (a stream
    // currently parked on a Mixer's per-input feed sink, see
    // `stream_match::resolve_playback_target_device_id`'s Mixer special
    // case) must never generate a generic `route-stream-*` link here — that
    // connection is already fully represented by the node's own `inputs`/
    // `outputs` port arrays (`collectEdges.ts` draws it from there). Drawing
    // it a second time from `current_target` too is not just redundant: if
    // the *specific* stream instance that was originally wired into the
    // port goes away and a new one reappears with the same app identity
    // (e.g. a browser tab reloading), the new stream's sink-input often
    // lands back on the same still-live feed sink — so `current_target`
    // picks it up correctly — but the processing node's own persisted port
    // still names the old, now-gone stream id and never re-resolves to the
    // new one. The result was a second, disagreeing edge with no
    // corresponding port/handle for it to attach to, rendered floating at
    // the node's default anchor instead of a real connection point.
    let processing_node_ids: HashSet<&str> =
        graph.processing_nodes.iter().map(|node| node.id.as_str()).collect();

    for stream in &graph.streams {
        let Some(target_id) = &stream.current_target else {
            continue;
        };
        if processing_node_ids.contains(target_id.as_str()) {
            continue;
        }

        match stream.direction {
            StreamDirection::Playback => {
                graph.links.push(Link {
                    id: format!("route-stream-{}", stream.id),
                    source_id: stream.id.clone(),
                    target_id: target_id.clone(),
                    is_monitor: false,
                });
            }
            StreamDirection::Capture => {
                graph.links.push(Link {
                    id: format!("route-capture-{}", stream.id),
                    source_id: target_id.clone(),
                    target_id: stream.id.clone(),
                    is_monitor: false,
                });
            }
        }
    }
}

fn apply_pw_link_device_routes(graph: &mut RuntimeGraph) {
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

    // Every virtual output can fan out to multiple real targets (there is no
    // PipeWire-level distinction between "multi output" and a plain output
    // sink), so always discover the full set of live fan-out targets per
    // device rather than relying on a single (source, target) pair — a sink
    // genuinely linked to two speakers has two rows in `pw-link -l` for the
    // same source, and collapsing them loses one.
    for device in &mut graph.devices {
        if device.direction != DeviceDirection::Output || device.kind != DeviceKind::Virtual {
            continue;
        }
        let system_name = device.system_name.clone();
        // A device currently hosting live effects routes its real audio out
        // through `effect_output.*`, not its own raw monitor (see
        // `split_sink::effective_fan_out_source`) — querying the raw name
        // here for such a device finds nothing (by design, since nothing
        // should be linked there once effects are live) and would silently
        // wipe `current_target`/`current_targets` back to empty on every
        // refresh, even though the device is genuinely still routed.
        let link_source = effective_fan_out_source(&system_name);
        let fan_out_names = pw_link::list_all_monitor_routes_for_source(&link_source);
        let targets: Vec<(String, String)> = fan_out_names
            .into_iter()
            .filter_map(|name| name_to_id.get(&name).cloned().map(|id| (name, id)))
            .collect();

        device.current_targets = targets.iter().map(|(_, id)| id.clone()).collect();
        device.current_target = targets.first().map(|(_, id)| id.clone());

        for (target_name, target_id) in &targets {
            graph.links.push(Link {
                id: format!("pwlink-{system_name}-{target_name}"),
                source_id: device.id.clone(),
                target_id: target_id.clone(),
                is_monitor: true,
            });
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
            let muted = pactl::sink_mute_state(feed_sink_name).ok().flatten().unwrap_or(false);

            mix_sources.push(MixSource {
                device_id: source_id.clone(),
                volume_percent,
                muted,
            });

            graph.links.push(Link {
                id: format!("pwlink-mix-{source_name}-{}", device.system_name),
                source_id: source_id.clone(),
                target_id: device.id.clone(),
                is_monitor: false,
            });
        }

        device.mix_sources = mix_sources;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Device, Stream};
    use crate::backend::linux::stream_match::resolve_playback_target_device_id;

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
                sample_rate: None,
                channels: None,
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
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
            sample_rate: None,
            channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                    sample_rate: None,
                    channels: None,
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
                media_name: None,
                is_system: false,
                volume_percent: None,
                muted: None,
                route_explanation: None,
                sample_rate: None,
                channels: None,
            }],
            links: vec![Link {
                id: "link-stale".into(),
                source_id: "firefox".into(),
                target_id: "hdmi".into(),
                is_monitor: false,
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

    /// Regression for a real bug found in manual live-PipeWire testing: a
    /// stream parked on a Mixer's per-input feed sink resolves
    /// `current_target` to the Mixer's processing-node id
    /// (`stream_match::resolve_playback_target_device_id`'s Mixer special
    /// case) — that must never also produce a generic `route-stream-*` link
    /// here, since the connection is already fully represented by the
    /// node's own `inputs` port array. Drawing it twice caused a floating,
    /// no-handle edge whenever the exact stream instance the port's
    /// bookkeeping named went away and a same-app-identity one reappeared
    /// (e.g. a browser tab reload) still parked on the same feed sink: the
    /// live-mirrored `current_target` picked the new stream up correctly,
    /// but the processing node's own port still named the old, gone stream
    /// id, so the two edges disagreed.
    #[test]
    fn normalize_stream_routing_links_skips_a_processing_node_target() {
        use crate::core::models::{ProcessingNode, ProcessingNodeKind, ProcessingNodePort};

        let mut graph = RuntimeGraph {
            streams: vec![Stream {
                id: "node-97".into(),
                app_name: "Firefox".into(),
                executable: Some("firefox".into()),
                window_class: None,
                system_name: Some("Firefox".into()),
                direction: StreamDirection::Playback,
                current_target: Some("processing-mixer-mixer".into()),
                media_name: None,
                is_system: false,
                volume_percent: None,
                muted: None,
                route_explanation: None,
                sample_rate: None,
                channels: None,
            }],
            processing_nodes: vec![ProcessingNode {
                id: "processing-mixer-mixer".into(),
                label: "Mixer".into(),
                kind: ProcessingNodeKind::Mixer { input_gains_percent: vec![100] },
                system_name: "pipe-deck-proc-mixer-mixer".into(),
                bypassed: false,
                live: true,
                inputs: vec![ProcessingNodePort { index: 0, connected_id: Some("node-97".into()), feed_key: None }],
                outputs: Vec::new(),
            }],
            data_source: "pipewire".into(),
            notice: None,
            ..Default::default()
        };

        normalize_stream_routing_links(&mut graph);

        assert!(
            graph.links.is_empty(),
            "a stream parked on a processing node's feed sink must not also get a route-stream-* link: {:?}",
            graph.links
        );
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. This is the actual
    //! regression case my earlier fixture-only assumption for #432, Gap 1
    //! missed: `pw_dump::normalize`'s own link-derived `current_target`
    //! never sees a `pipe-deck-*` virtual device at all (it's filtered out
    //! of `pw-dump`'s own node parsing and merged in later, see
    //! `pw_dump.rs`'s own file-level note), so a stream routed onto a
    //! virtual device can only ever get a `current_target` from *this*
    //! module's `apply_native_playback_targets`/`apply_native_capture_targets`
    //! — the actual call site #432 needed to replace, not `normalize`
    //! itself. Builds a graph the way `merge_virtual_devices` would (a real
    //! `pw-dump`-derived graph plus one manually-injected virtual `Device`,
    //! avoiding `CoreEngine`'s own `ConfigStore` persistence side effects)
    //! and confirms `sync_live_routing_graph` resolves a real routed
    //! stream's target to it with zero `pactl` calls in the path.
    use super::*;
    use crate::backend::AudioBackend;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use crate::core::models::DeviceKind;
    use std::thread;
    use std::time::Duration;

    struct LoopedPlaybackStream {
        child: std::process::Child,
    }

    impl LoopedPlaybackStream {
        fn spawn(target_system_name: &str) -> Self {
            let clip = "/usr/share/sounds/speech-dispatcher/test.wav";
            assert!(std::path::Path::new(clip).is_file(), "expected a system test wav to exist at {clip}");
            let child = crate::sysproc::command("sh")
                .args(["-c", &format!("while true; do pw-cat --playback --target '{target_system_name}' '{clip}'; done")])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("failed to spawn looped pw-cat");
            Self { child }
        }
    }

    impl Drop for LoopedPlaybackStream {
        fn drop(&mut self) {
            let _ = crate::sysproc::command("pkill").args(["-P", &self.child.id().to_string()]).status();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    #[ignore]
    fn resolves_a_playback_streams_target_to_a_virtual_device_natively() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend
            .create_virtual_output("Pipe Deck Native Target Resolution Test", false)
            .expect("create virtual output should succeed");

        let _stream = LoopedPlaybackStream::spawn(&device.system_name);

        let virtual_device_id = format!("virtual-{}", device.system_name.strip_prefix("pipe-deck-").unwrap());

        let found = (0..20).find_map(|_| {
            let mut graph = backend.fetch_graph().expect("fetch_graph should succeed");
            // Mimics `core::engine::virtual_ops::merge_virtual_devices` just
            // enough for this test: a real virtual device is never present
            // in `fetch_graph`'s own output (see this module's own doc
            // comment), so target resolution against it can only be
            // exercised by injecting it the same way the engine would
            // before calling `sync_live_routing_graph`.
            graph.devices.push(crate::core::models::Device {
                id: virtual_device_id.clone(),
                system_name: device.system_name.clone(),
                label: device.label.clone(),
                kind: DeviceKind::Virtual,
                direction: DeviceDirection::Output,
                sink_mode: None,
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
                sample_rate: None,
                channels: None,
            });

            sync_live_routing_graph(&mut graph);

            let stream = graph.streams.iter().find(|stream| stream.system_name.as_deref() == Some("pw-cat")).cloned();
            if stream.as_ref().and_then(|s| s.current_target.as_ref()).is_some() {
                return stream;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        let stream = found.expect("expected sync_live_routing_graph to resolve pw-cat's current_target");
        assert_eq!(stream.current_target.as_deref(), Some(virtual_device_id.as_str()));

        let _ = backend.remove_virtual_device(&device.system_name);
    }

    struct LoopedRecordStream {
        child: std::process::Child,
    }

    impl LoopedRecordStream {
        fn spawn() -> Self {
            let child = crate::sysproc::command("sh")
                .args(["-c", "while true; do pw-cat --record --format=s16 --rate=48000 --channels=2 /dev/null; done"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("failed to spawn looped pw-cat --record");
            Self { child }
        }
    }

    impl Drop for LoopedRecordStream {
        fn drop(&mut self) {
            let _ = crate::sysproc::command("pkill").args(["-P", &self.child.id().to_string()]).status();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Capture-direction counterpart to
    /// `resolves_a_playback_streams_target_to_a_virtual_device_natively`.
    #[test]
    #[ignore]
    fn resolves_a_capture_streams_target_to_a_virtual_device_natively() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend = LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let source = backend
            .create_virtual_output("Pipe Deck Native Capture Target Resolution Test", false)
            .expect("create virtual output should succeed");

        let _stream = LoopedRecordStream::spawn();
        let routed = (0..50).any(|_| {
            if pw_link::route_capture_stream(&source.system_name, "pw-cat").is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(routed, "expected the capture route to succeed once pw-cat registers as a live node");

        let virtual_device_id = format!("virtual-{}", source.system_name.strip_prefix("pipe-deck-").unwrap());

        let found = (0..20).find_map(|_| {
            // Re-assert the route on every attempt, not just once up front —
            // a session policy manager (wireplumber) auto-connects a
            // just-created capture stream with no explicit target to the
            // system default source the moment it appears, racing this
            // test's own explicit route (confirmed live: a manual `pw-dump`
            // link dump showed both the default mic and this test's virtual
            // device linked into `pw-cat` simultaneously). Re-issuing
            // `route_capture_stream` (a disconnect-then-link, #428) on every
            // poll converges past that race.
            let _ = pw_link::route_capture_stream(&source.system_name, "pw-cat");

            let mut graph = backend.fetch_graph().expect("fetch_graph should succeed");
            graph.devices.push(crate::core::models::Device {
                id: virtual_device_id.clone(),
                system_name: source.system_name.clone(),
                label: source.label.clone(),
                kind: DeviceKind::Virtual,
                direction: DeviceDirection::Output,
                sink_mode: None,
                volume_percent: None,
                muted: None,
                current_target: None,
                current_targets: Vec::new(),
                mix_sources: Vec::new(),
                sample_rate: None,
                channels: None,
            });

            sync_live_routing_graph(&mut graph);

            let stream = graph.streams.iter().find(|stream| stream.system_name.as_deref() == Some("pw-cat")).cloned();
            if stream.as_ref().and_then(|s| s.current_target.as_ref()).is_some() {
                return stream;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        let stream = found.expect("expected sync_live_routing_graph to resolve pw-cat's current_target");
        assert_eq!(stream.current_target.as_deref(), Some(virtual_device_id.as_str()));

        let _ = backend.remove_virtual_device(&source.system_name);
    }
}
