use serde::Serialize;
use std::process::Command;

const BUILD_REVISION: &str = env!("PIPE_DECK_BUILD_REVISION");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    Flatpak,
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
    pub background_restore_supported: bool,
    pub install_label: String,
    pub pipewire_version: Option<String>,
}

fn detect_install_kind() -> InstallKind {
    if std::env::var("FLATPAK_ID").is_ok() {
        return InstallKind::Flatpak;
    }

    if let Ok(exe) = std::env::current_exe() {
        let path = exe.to_string_lossy();
        if path.starts_with("/app/") {
            return InstallKind::Flatpak;
        }
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
        InstallKind::Flatpak => "Flatpak".to_string(),
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
    let background_restore_supported = !matches!(install_kind, InstallKind::Flatpak);
    let build_revision = build_revision_for_display();
    let pipewire_version = state.engine.read().await.platform_audio_version();

    Ok(AppInfo {
        release_version: release_version_from_revision(&build_revision),
        install_label: install_label(&install_kind),
        background_restore_supported,
        install_kind,
        build_revision,
        pipewire_version,
    })
}

/// Assembles a single copyable text blob for bug reports: build/version info,
/// a fresh raw pw-dump snapshot (via the existing backend fetch path — see
/// `AudioBackend::diagnostics_snapshot`), and the environment fields the bug
/// report template already asks for. Pipe Deck doesn't write a log file (see
/// `docs/project/Getting_Started.md`'s troubleshooting section), so there's
/// no log section to include.
fn format_diagnostics_bundle(
    install_kind: &InstallKind,
    build_revision: &str,
    release_version: Option<&str>,
    pipewire_version: Option<&str>,
    pw_dump_snapshot: Option<&str>,
) -> String {
    let mut bundle = String::new();
    bundle.push_str("Pipe Deck diagnostics\n");
    bundle.push_str("=====================\n\n");
    bundle.push_str(&format!("Version: {}\n", release_version.unwrap_or(build_revision)));
    bundle.push_str(&format!("Build: {build_revision}\n"));
    bundle.push_str(&format!("Install type: {}\n", install_label(install_kind)));
    bundle.push_str(&format!(
        "PipeWire version: {}\n",
        pipewire_version.unwrap_or("unknown")
    ));
    bundle.push('\n');

    bundle.push_str("pw-dump snapshot\n");
    bundle.push_str("-----------------\n");
    bundle.push_str(pw_dump_snapshot.unwrap_or("(not available)\n"));

    bundle
}

#[tauri::command]
pub async fn get_diagnostics_bundle(state: tauri::State<'_, crate::AppState>) -> Result<String, String> {
    let install_kind = detect_install_kind();
    let build_revision = build_revision_for_display();
    let release_version = release_version_from_revision(&build_revision);
    let engine = state.engine.read().await;
    let pipewire_version = engine.platform_audio_version();
    let pw_dump_snapshot = engine.diagnostics_snapshot();

    Ok(format_diagnostics_bundle(
        &install_kind,
        &build_revision,
        release_version.as_deref(),
        pipewire_version.as_deref(),
        pw_dump_snapshot.as_deref(),
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
    use super::{format_diagnostics_bundle, release_version_from_revision, InstallKind};

    #[test]
    fn install_kind_serializes_snake_case() {
        let json = serde_json::to_string(&InstallKind::Flatpak).expect("serialize");
        assert_eq!(json, "\"flatpak\"");
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
    fn diagnostics_bundle_includes_version_and_snapshot() {
        let bundle = format_diagnostics_bundle(
            &InstallKind::Deb,
            "v0.1.0",
            Some("0.1.0"),
            Some("1.2.3"),
            Some("{\"id\": 1}"),
        );

        assert!(bundle.contains("Version: 0.1.0"));
        assert!(bundle.contains("Build: v0.1.0"));
        assert!(bundle.contains("Install type: .deb package"));
        assert!(bundle.contains("PipeWire version: 1.2.3"));
        assert!(bundle.contains("{\"id\": 1}"));
    }

    #[test]
    fn diagnostics_bundle_falls_back_when_fields_are_missing() {
        let bundle = format_diagnostics_bundle(&InstallKind::Dev, "unknown", None, None, None);

        assert!(bundle.contains("Version: unknown"));
        assert!(bundle.contains("PipeWire version: unknown"));
        assert!(bundle.contains("(not available)"));
    }
}
