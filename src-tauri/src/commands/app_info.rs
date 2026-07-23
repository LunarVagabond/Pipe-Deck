use crate::core::models::{ActionStatus, RuntimeGraph};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;

const BUILD_REVISION: &str = env!("PIPE_DECK_BUILD_REVISION");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    Deb,
    Rpm,
    AppImage,
    Native,
    Dev,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub build_revision: String,
    pub release_version: Option<String>,
    pub install_kind: InstallKind,
    pub install_label: String,
    pub pipewire_version: Option<String>,
}

fn detect_install_kind() -> InstallKind {
    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if std::env::var("APPIMAGE").is_ok() || path.contains(".mount_") {
            return InstallKind::AppImage;
        }
    }

    if dpkg_owns_package("pipe-deck") {
        return InstallKind::Deb;
    }

    if rpm_owns_package("pipe-deck") {
        return InstallKind::Rpm;
    }

    if cfg!(debug_assertions) {
        return InstallKind::Dev;
    }

    InstallKind::Native
}

fn dpkg_owns_package(name: &str) -> bool {
    Command::new("dpkg-query")
        .args(["-W", "-f=${Status}", name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn rpm_owns_package(name: &str) -> bool {
    Command::new("rpm")
        .args(["-q", name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn install_label(kind: &InstallKind) -> String {
    match kind {
        InstallKind::Deb => ".deb package".to_string(),
        InstallKind::Rpm => ".rpm package".to_string(),
        InstallKind::AppImage => "AppImage".to_string(),
        InstallKind::Dev => "development build".to_string(),
        InstallKind::Native => "native package".to_string(),
    }
}

fn release_version_from_revision(revision: &str) -> Option<String> {
    let trimmed = revision.trim();
    if trimmed.is_empty() {
        return None;
    }

    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    // Only the numeric `major.minor.patch` core needs to be all-digits — a release
    // tag may carry a `-slug` suffix (e.g. `v0.0.2-alpha`, per `make release`'s tag
    // format), which should still be recognized as a version rather than rejected
    // as if it were a commit hash.
    let core = version.split('-').next().unwrap_or(version);
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    if parts.iter().all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit())) {
        Some(version.to_string())
    } else {
        None
    }
}

/// Best-effort distro/flavor label from `/etc/os-release`'s `PRETTY_NAME`
/// (e.g. "Pop!_OS 22.04 LTS"), falling back to `NAME` if `PRETTY_NAME` is
/// absent, and "unknown" if neither field or the file itself is present.
fn detect_os_name() -> String {
    let Ok(contents) = std::fs::read_to_string("/etc/os-release") else {
        return "unknown".to_string();
    };

    let mut name = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return value.trim_matches('"').to_string();
        }
        if let Some(value) = line.strip_prefix("NAME=") {
            name = Some(value.trim_matches('"').to_string());
        }
    }

    name.unwrap_or_else(|| "unknown".to_string())
}

/// Desktop environment / compositor, e.g. "COSMIC", "GNOME", "KDE" — read
/// from `XDG_CURRENT_DESKTOP`, which desktop session managers set on login.
fn detect_desktop_environment() -> Option<String> {
    std::env::var("XDG_CURRENT_DESKTOP").ok().filter(|value| !value.is_empty())
}

/// Display server session type ("wayland" or "x11") from `XDG_SESSION_TYPE`.
fn detect_session_type() -> Option<String> {
    std::env::var("XDG_SESSION_TYPE").ok().filter(|value| !value.is_empty())
}

fn build_revision_for_display() -> String {
    let revision = BUILD_REVISION.trim();
    if revision.is_empty() {
        "unknown".to_string()
    } else {
        revision.to_string()
    }
}

#[tauri::command]
pub async fn get_app_info(state: tauri::State<'_, crate::AppState>) -> Result<AppInfo, String> {
    let install_kind = detect_install_kind();
    let build_revision = build_revision_for_display();
    let pipewire_version = state.engine.read().await.platform_audio_version();

    Ok(AppInfo {
        release_version: release_version_from_revision(&build_revision),
        install_label: install_label(&install_kind),
        install_kind,
        build_revision,
        pipewire_version,
    })
}

/// Assembles a single copyable Markdown blob for bug reports: build/version
/// and running-environment info (OS/distro, desktop environment, session
/// type) as a bullet list, and a compact routing-graph summary (the same
/// devices/streams/links data already shown in the app's own UI, not a raw
/// `pw-dump` dump — see `format_graph_summary`) in a fenced code block so it
/// pastes into a GitHub issue as readable monospace rather than a wall of
/// unformatted text. Pipe Deck doesn't write a log file (see
/// `docs/developers/Getting_Started.md`'s troubleshooting section), so there's
/// no log section to include.
#[allow(clippy::too_many_arguments)]
fn format_diagnostics_bundle(
    install_kind: &InstallKind,
    build_revision: &str,
    release_version: Option<&str>,
    pipewire_version: Option<&str>,
    os_name: &str,
    desktop_environment: Option<&str>,
    session_type: Option<&str>,
    graph: &RuntimeGraph,
) -> String {
    let mut bundle = String::new();
    bundle.push_str("## Pipe Deck diagnostics\n\n");
    bundle.push_str(&format!("- **Version:** {}\n", release_version.unwrap_or(build_revision)));
    bundle.push_str(&format!("- **Build:** {build_revision}\n"));
    bundle.push_str(&format!("- **Install type:** {}\n", install_label(install_kind)));
    bundle.push_str(&format!(
        "- **PipeWire version:** {}\n",
        pipewire_version.unwrap_or("unknown")
    ));
    bundle.push_str(&format!("- **OS:** {os_name}\n"));
    bundle.push_str(&format!(
        "- **Desktop:** {}\n",
        desktop_environment.unwrap_or("unknown")
    ));
    bundle.push_str(&format!(
        "- **Session type:** {}\n\n",
        session_type.unwrap_or("unknown")
    ));

    bundle.push_str("### Graph snapshot\n\n");
    bundle.push_str("```\n");
    bundle.push_str(&format_graph_summary(graph));
    bundle.push_str("```\n");

    bundle
}

/// A compact, human-readable routing summary — device/stream labels and
/// where they're currently routed, resolved from ids to labels via `graph`
/// itself. Deliberately *not* a raw `pw-dump` dump: this is exactly the
/// state the app's own routing graph already displays, so it adds no new
/// exposure, and stays small (tens of lines, not the ~10k a raw snapshot
/// runs to) — see issue discussion on #170 for why the raw dump was dropped.
fn resolve_label<'a>(label_by_id: &HashMap<&'a str, &'a str>, id: &'a str) -> &'a str {
    label_by_id.get(id).copied().unwrap_or(id)
}

fn format_graph_summary(graph: &RuntimeGraph) -> String {
    let label_by_id: HashMap<&str, &str> =
        graph.devices.iter().map(|device| (device.id.as_str(), device.label.as_str())).collect();

    let mut out = String::new();
    out.push_str(&format!("Devices ({}):\n", graph.devices.len()));
    for device in &graph.devices {
        let targets: Vec<&str> = if !device.current_targets.is_empty() {
            device.current_targets.iter().map(|id| resolve_label(&label_by_id, id)).collect()
        } else {
            device
                .current_target
                .as_deref()
                .map(|id| resolve_label(&label_by_id, id))
                .into_iter()
                .collect()
        };
        let routed_to = if targets.is_empty() { String::new() } else { format!(" -> {}", targets.join(", ")) };
        out.push_str(&format!(
            "  {} ({:?}, {:?}){routed_to}\n",
            device.label, device.kind, device.direction
        ));
    }

    out.push_str(&format!("\nStreams ({}):\n", graph.streams.len()));
    for stream in &graph.streams {
        let routed_to = stream
            .current_target
            .as_deref()
            .map(|id| format!(" -> {}", resolve_label(&label_by_id, id)))
            .unwrap_or_default();
        let warning = match stream.route_explanation.as_ref().map(|explanation| &explanation.action_status) {
            Some(status) if !matches!(status, ActionStatus::Applied) => format!("  [{status:?}]"),
            _ => String::new(),
        };
        out.push_str(&format!(
            "  {} ({:?}){routed_to}{warning}\n",
            stream.app_name, stream.direction
        ));
    }

    out
}

#[tauri::command]
pub async fn get_diagnostics_bundle(state: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    let install_kind = detect_install_kind();
    let build_revision = build_revision_for_display();
    let release_version = release_version_from_revision(&build_revision);
    let engine = state.engine.read().await;
    let pipewire_version = engine.platform_audio_version();
    let os_name = detect_os_name();
    let desktop_environment = detect_desktop_environment();
    let session_type = detect_session_type();

    Ok(format_diagnostics_bundle(
        &install_kind,
        &build_revision,
        release_version.as_deref(),
        pipewire_version.as_deref(),
        &os_name,
        desktop_environment.as_deref(),
        session_type.as_deref(),
        engine.runtime_graph(),
    ))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http(s) URLs are allowed".to_string());
    }

    Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map_err(|error| format!("Failed to open URL: {error}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_diagnostics_bundle, format_graph_summary, release_version_from_revision, InstallKind};
    use crate::core::models::{
        ActionStatus, Device, DeviceDirection, DeviceKind, RouteExplanation, RouteSource, RuntimeGraph, Stream,
        StreamDirection,
    };

    fn sample_device(id: &str, label: &str, current_target: Option<&str>) -> Device {
        Device {
            id: id.to_string(),
            system_name: format!("pipe-deck-{id}"),
            label: label.to_string(),
            kind: DeviceKind::Virtual,
            direction: DeviceDirection::Output,
            sink_mode: None,
            volume_percent: None,
            muted: None,
            current_target: current_target.map(str::to_string),
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        }
    }

    fn sample_stream(app_name: &str, current_target: Option<&str>, action_status: Option<ActionStatus>) -> Stream {
        Stream {
            id: format!("stream-{app_name}"),
            app_name: app_name.to_string(),
            executable: None,
            window_class: None,
            system_name: None,
            direction: StreamDirection::Playback,
            current_target: current_target.map(str::to_string),
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: action_status.map(|action_status| RouteExplanation {
                source: RouteSource::NoRule,
                matched_rule_id: None,
                matched_rule_key: None,
                match_reasons: Vec::new(),
                skipped_candidates: Vec::new(),
                action_status,
                target_system_name: None,
                target_system_names: Vec::new(),
                fallback_applied: false,
            }),
        }
    }

    #[test]
    fn install_kind_serializes_snake_case() {
        let json = serde_json::to_string(&InstallKind::AppImage).expect("serialize");
        assert_eq!(json, "\"app_image\"");
    }

    #[test]
    fn release_version_parses_semver_tags() {
        assert_eq!(
            release_version_from_revision("v0.1.0"),
            Some("0.1.0".to_string())
        );
        assert_eq!(release_version_from_revision("1.2.3"), Some("1.2.3".to_string()));
    }

    #[test]
    fn release_version_rejects_commit_hashes() {
        assert_eq!(release_version_from_revision("910d0"), None);
        assert_eq!(release_version_from_revision("cc38c6e"), None);
    }

    #[test]
    fn release_version_parses_tags_with_slug_suffix() {
        assert_eq!(
            release_version_from_revision("v0.0.2-alpha"),
            Some("0.0.2-alpha".to_string())
        );
    }

    #[test]
    fn diagnostics_bundle_includes_version_and_graph_summary() {
        let graph = RuntimeGraph {
            devices: vec![sample_device("d1", "Speakers", None)],
            streams: vec![sample_stream("Discord", Some("d1"), None)],
            links: Vec::new(),
            data_source: "pipewire".to_string(),
            notice: None,
            recent_stream_identities: Vec::new(),
        };
        let bundle = format_diagnostics_bundle(
            &InstallKind::Deb,
            "v0.1.0",
            Some("0.1.0"),
            Some("1.2.3"),
            "Pop!_OS 22.04 LTS",
            Some("COSMIC"),
            Some("wayland"),
            &graph,
        );

        assert!(bundle.contains("**Version:** 0.1.0"));
        assert!(bundle.contains("**Build:** v0.1.0"));
        assert!(bundle.contains("**Install type:** .deb package"));
        assert!(bundle.contains("**PipeWire version:** 1.2.3"));
        assert!(bundle.contains("**OS:** Pop!_OS 22.04 LTS"));
        assert!(bundle.contains("**Desktop:** COSMIC"));
        assert!(bundle.contains("**Session type:** wayland"));
        assert!(bundle.contains("Speakers"));
        assert!(bundle.contains("Discord"));
        assert!(bundle.contains("-> Speakers"));
    }

    #[test]
    fn diagnostics_bundle_falls_back_when_fields_are_missing() {
        let graph = RuntimeGraph {
            devices: Vec::new(),
            streams: Vec::new(),
            links: Vec::new(),
            data_source: "pipewire".to_string(),
            notice: None,
            recent_stream_identities: Vec::new(),
        };
        let bundle = format_diagnostics_bundle(&InstallKind::Dev, "unknown", None, None, "unknown", None, None, &graph);

        assert!(bundle.contains("**Version:** unknown"));
        assert!(bundle.contains("**PipeWire version:** unknown"));
        assert!(bundle.contains("**OS:** unknown"));
        assert!(bundle.contains("**Desktop:** unknown"));
        assert!(bundle.contains("**Session type:** unknown"));
        assert!(bundle.contains("Devices (0):"));
        assert!(bundle.contains("Streams (0):"));
    }

    #[test]
    fn graph_summary_stays_compact_and_resolves_target_labels() {
        let graph = RuntimeGraph {
            devices: vec![sample_device("d1", "Headphones", None)],
            streams: vec![sample_stream("Spotify", Some("d1"), None)],
            links: Vec::new(),
            data_source: "pipewire".to_string(),
            notice: None,
            recent_stream_identities: Vec::new(),
        };
        let summary = format_graph_summary(&graph);

        // Small and readable, not a multi-thousand-line raw dump.
        assert!(summary.lines().count() < 10);
        assert!(summary.contains("Spotify (Playback) -> Headphones"));
    }

    #[test]
    fn graph_summary_flags_a_non_applied_route_but_not_an_applied_one() {
        let graph = RuntimeGraph {
            devices: vec![sample_device("d1", "Speakers", None)],
            streams: vec![
                sample_stream("Blocked App", Some("d1"), Some(ActionStatus::Blocked)),
                sample_stream("Fine App", Some("d1"), Some(ActionStatus::Applied)),
            ],
            links: Vec::new(),
            data_source: "pipewire".to_string(),
            notice: None,
            recent_stream_identities: Vec::new(),
        };
        let summary = format_graph_summary(&graph);

        assert!(summary.contains("Blocked App (Playback) -> Speakers  [Blocked]"));
        assert!(!summary.contains("Fine App (Playback) -> Speakers  ["));
    }
}
