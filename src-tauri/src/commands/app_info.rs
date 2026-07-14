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
    use super::{release_version_from_revision, InstallKind};

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
}
