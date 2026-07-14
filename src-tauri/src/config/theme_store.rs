use crate::core::models::{
    CustomThemeFile, ResolvedScheme, ThemeBase, ThemeColors, ThemeSchemeSource,
};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("failed to read theme: {0}")]
    Read(String),
}

pub struct ThemeStore {
    config_dir: PathBuf,
}

fn midnight_deck() -> ThemeColors {
    ThemeColors {
        background: "#0b0f14".into(),
        surface_1: "#131820".into(),
        surface_2: "#1c2330".into(),
        border: "#2a3344".into(),
        text: "#e6e9ef".into(),
        text_muted: "#9aa4b2".into(),
        accent_purple: "#7c5cff".into(),
        accent_teal: "#26c3a3".into(),
        accent_amber: "#ffb020".into(),
    }
}

fn copper_dusk() -> ThemeColors {
    ThemeColors {
        background: "#12100e".into(),
        surface_1: "#1b1815".into(),
        surface_2: "#26211c".into(),
        border: "#3a322a".into(),
        text: "#f0e9e0".into(),
        text_muted: "#b0a394".into(),
        accent_purple: "#e0794b".into(),
        accent_teal: "#4bb0a0".into(),
        accent_amber: "#f2c14e".into(),
    }
}

fn paper_deck() -> ThemeColors {
    ThemeColors {
        background: "#f5f6f8".into(),
        surface_1: "#ffffff".into(),
        surface_2: "#eef0f4".into(),
        border: "#d3d8e0".into(),
        text: "#1a2230".into(),
        text_muted: "#5b6675".into(),
        accent_purple: "#6b47e6".into(),
        accent_teal: "#12a08a".into(),
        accent_amber: "#d98a00".into(),
    }
}

fn meadow_light() -> ThemeColors {
    ThemeColors {
        background: "#f4f7f2".into(),
        surface_1: "#ffffff".into(),
        surface_2: "#e8efe6".into(),
        border: "#cdd8c8".into(),
        text: "#1c2a1e".into(),
        text_muted: "#5d6b5a".into(),
        accent_purple: "#2f8f5b".into(),
        accent_teal: "#0e9488".into(),
        accent_amber: "#e08a1e".into(),
    }
}

impl ThemeStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    pub fn built_in_schemes() -> Vec<ResolvedScheme> {
        vec![
            ResolvedScheme {
                id: "midnight-deck".into(),
                name: "Midnight Deck".into(),
                kind: ThemeBase::Dark,
                source: ThemeSchemeSource::Builtin,
                colors: midnight_deck(),
            },
            ResolvedScheme {
                id: "copper-dusk".into(),
                name: "Copper Dusk".into(),
                kind: ThemeBase::Dark,
                source: ThemeSchemeSource::Builtin,
                colors: copper_dusk(),
            },
            ResolvedScheme {
                id: "paper-deck".into(),
                name: "Paper Deck".into(),
                kind: ThemeBase::Light,
                source: ThemeSchemeSource::Builtin,
                colors: paper_deck(),
            },
            ResolvedScheme {
                id: "meadow-light".into(),
                name: "Meadow Light".into(),
                kind: ThemeBase::Light,
                source: ThemeSchemeSource::Builtin,
                colors: meadow_light(),
            },
        ]
    }

    /// The built-in palette a custom scheme's unset keys fall back to.
    pub fn built_in_base(base: ThemeBase) -> ThemeColors {
        match base {
            ThemeBase::Dark => midnight_deck(),
            ThemeBase::Light => paper_deck(),
        }
    }

    /// Scans `<config_dir>/themes/*.yaml`, resolving each against its declared base.
    /// Malformed files are skipped (and logged) rather than failing the whole list.
    pub fn load_custom_schemes(&self) -> Vec<ResolvedScheme> {
        let themes_dir = self.themes_dir();
        let Ok(entries) = fs::read_dir(&themes_dir) else {
            return Vec::new();
        };

        let mut schemes = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };

            match self.load_custom_scheme_file(&path, stem) {
                Ok(scheme) => schemes.push(scheme),
                Err(error) => {
                    eprintln!("skipping invalid theme file {path:?}: {error}");
                }
            }
        }

        schemes.sort_by(|left, right| left.id.cmp(&right.id));
        schemes
    }

    fn load_custom_scheme_file(&self, path: &std::path::Path, stem: &str) -> Result<ResolvedScheme, ThemeError> {
        let contents = fs::read_to_string(path)
            .map_err(|error| ThemeError::Read(format!("{path:?}: {error}")))?;
        let file: CustomThemeFile = serde_yaml::from_str(&contents)
            .map_err(|error| ThemeError::Read(format!("{path:?}: {error}")))?;

        let base = Self::built_in_base(file.base);
        Ok(ResolvedScheme {
            id: format!("custom:{stem}"),
            name: file.name,
            kind: file.base,
            source: ThemeSchemeSource::Custom,
            colors: file.colors.resolve(&base),
        })
    }

    pub fn list_schemes(&self) -> Vec<ResolvedScheme> {
        let mut schemes = Self::built_in_schemes();
        schemes.extend(self.load_custom_schemes());
        schemes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn with_temp_themes_dir<F: FnOnce(&ThemeStore, &PathBuf)>(run: F) {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "pipe-deck-theme-test-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        let themes_dir = temp_dir.join("themes");
        fs::create_dir_all(&themes_dir).unwrap();
        let store = ThemeStore::new(temp_dir.clone());
        run(&store, &themes_dir);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn built_in_list_has_two_light_two_dark() {
        let schemes = ThemeStore::built_in_schemes();
        let dark = schemes.iter().filter(|s| s.kind == ThemeBase::Dark).count();
        let light = schemes.iter().filter(|s| s.kind == ThemeBase::Light).count();
        assert_eq!(dark, 2);
        assert_eq!(light, 2);
    }

    #[test]
    fn custom_partial_override_merges_over_base() {
        with_temp_themes_dir(|store, themes_dir| {
            fs::write(
                themes_dir.join("neon.yaml"),
                "name: Neon Night\nbase: dark\ncolors:\n  accent_purple: \"#ff2d95\"\n  accent_teal: \"#00e5ff\"\n",
            )
            .unwrap();

            let schemes = store.load_custom_schemes();
            assert_eq!(schemes.len(), 1);
            let scheme = &schemes[0];
            let base = ThemeStore::built_in_base(ThemeBase::Dark);
            assert_eq!(scheme.colors.accent_purple, "#ff2d95");
            assert_eq!(scheme.colors.accent_teal, "#00e5ff");
            assert_eq!(scheme.colors.background, base.background);
            assert_eq!(scheme.colors.text, base.text);
        });
    }

    #[test]
    fn custom_name_comes_from_yaml_field_not_filename() {
        with_temp_themes_dir(|store, themes_dir| {
            fs::write(
                themes_dir.join("some-file-stem.yaml"),
                "name: My Cool Theme\nbase: light\n",
            )
            .unwrap();

            let schemes = store.load_custom_schemes();
            assert_eq!(schemes[0].name, "My Cool Theme");
            assert_eq!(schemes[0].id, "custom:some-file-stem");
        });
    }

    #[test]
    fn invalid_theme_file_is_skipped_not_fatal() {
        with_temp_themes_dir(|store, themes_dir| {
            fs::write(themes_dir.join("broken.yaml"), "not: [valid, theme").unwrap();
            fs::write(themes_dir.join("good.yaml"), "name: Good\nbase: dark\n").unwrap();

            let schemes = store.load_custom_schemes();
            assert_eq!(schemes.len(), 1);
            assert_eq!(schemes[0].name, "Good");
        });
    }

    #[test]
    fn unknown_base_value_rejected() {
        with_temp_themes_dir(|store, themes_dir| {
            fs::write(
                themes_dir.join("bad-base.yaml"),
                "name: Bad Base\nbase: purple\n",
            )
            .unwrap();

            let schemes = store.load_custom_schemes();
            assert!(schemes.is_empty());
        });
    }

    #[test]
    fn list_schemes_combines_builtin_and_custom() {
        with_temp_themes_dir(|store, themes_dir| {
            fs::write(themes_dir.join("extra.yaml"), "name: Extra\nbase: light\n").unwrap();
            let schemes = store.list_schemes();
            assert_eq!(schemes.len(), 5);
        });
    }
}
