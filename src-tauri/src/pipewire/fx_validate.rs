use crate::core::models::EffectChainConfig;
use crate::pipewire::fx_capability::FxCapabilities;

/// Range bounds enforced before any value is ever serialized into a conf
/// file. Out-of-range values are rejected outright, never silently clamped —
/// a silently-clamped-but-still-written value would hide the fact validation
/// caught something.
const EQ_GAIN_RANGE_DB: (i32, i32) = (-12, 12);
const OUTPUT_GAIN_RANGE_DB: (i32, i32) = (-12, 12);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PreflightResult {
    pub ok: bool,
    pub warnings: Vec<String>,
    pub blocking_reasons: Vec<String>,
}

/// Validates a chain against the v1 safety contract: builtin-only stages,
/// no filesystem/plugin paths accepted from user input, every numeric
/// parameter range-checked, and any dynamics stage the host can't actually
/// back (per `FxCapabilities`) rejected rather than silently ignored. Pure
/// and side-effect-free — never writes anything, never touches PipeWire.
pub fn preflight(config: &EffectChainConfig, capabilities: &FxCapabilities) -> PreflightResult {
    let mut warnings = Vec::new();
    let mut blocking_reasons = Vec::new();

    for (label, value) in [
        ("Sub band", config.eq_sub),
        ("Bass band", config.eq_bass),
        ("Mid band", config.eq_mid),
        ("Treble band", config.eq_treble),
        ("Air band", config.eq_air),
    ] {
        check_range(label, value, EQ_GAIN_RANGE_DB, &mut blocking_reasons);
    }
    check_range("Output gain", config.output_gain, OUTPUT_GAIN_RANGE_DB, &mut blocking_reasons);

    let has_eq_or_gain =
        config.eq_sub != 0 || config.eq_bass != 0 || config.eq_mid != 0 || config.eq_treble != 0
            || config.eq_air != 0 || config.output_gain != 0;
    if has_eq_or_gain && !capabilities.builtin_eq {
        blocking_reasons.push(
            "EQ/gain requires PipeWire's builtin filter-chain module, which was not found on this system"
                .to_string(),
        );
    }

    if config.limiter.enabled {
        // No PipeWire version currently ships a builtin limiter (verified
        // against `man 7 libpipewire-module-filter-chain`); this stage has
        // nothing safe to back it with yet, so it's always blocked in v1.
        blocking_reasons.push(
            "Limiter has no supported backing plugin on this system yet — disabled until one is available"
                .to_string(),
        );
    }
    if config.compressor.enabled {
        blocking_reasons.push(
            "Compressor has no supported backing plugin on this system yet — disabled until one is available"
                .to_string(),
        );
    }
    if config.noise_gate.enabled && capabilities.ladspa_noise_gate.is_none() {
        blocking_reasons.push(
            "Noise gate requires a LADSPA noise-suppression plugin (e.g. librnnoise_ladspa) that was not found on this system"
                .to_string(),
        );
    }

    for stage in [&config.limiter, &config.compressor, &config.noise_gate] {
        if stage.enabled && !(-60..=0).contains(&stage.threshold_db) {
            blocking_reasons.push(format!(
                "Threshold {}dB is out of the supported -60..0 range",
                stage.threshold_db
            ));
        }
    }

    if blocking_reasons.is_empty() && has_eq_or_gain {
        warnings.push(
            "Applying will briefly restart PipeWire audio while the effect chain is loaded".to_string(),
        );
    }

    PreflightResult {
        ok: blocking_reasons.is_empty(),
        warnings,
        blocking_reasons,
    }
}

fn check_range(label: &str, value: i32, range: (i32, i32), blocking_reasons: &mut Vec<String>) {
    if value < range.0 || value > range.1 {
        blocking_reasons.push(format!(
            "{label} value {value}dB is out of the supported {}..{} range",
            range.0, range.1
        ));
    }
}

/// Renders the builtin-only `module-filter-chain` conf block for one
/// device's EQ + output gain, following the exact syntax PipeWire's own
/// shipped example (`/usr/share/pipewire/filter-chain/sink-eq6.conf`) uses.
/// Only called after `preflight` has returned `ok: true` — never accepts a
/// plugin path, LADSPA name, or anything else sourced from outside this
/// fixed builtin template.
///
/// The capture stage takes over `device_system_name` itself (replacing the
/// device's plain null-sink for as long as effects are active) so anything
/// already routed to it keeps finding a sink of the same name; the processed
/// signal leaves via a separate `effect_output.{device_system_name}` node
/// that must be explicitly linked onward (see `pipewire::filter_chain`).
pub fn render_conf(device_system_name: &str, config: &EffectChainConfig) -> String {
    let node_description = format!("Pipe Deck Effects - {device_system_name}");
    // Bypassed means "keep the chain loaded but pass audio through
    // unprocessed" — bake that in as neutral values here so the initial
    // Structural Apply already matches what `live_params` would push right
    // after, rather than briefly applying the real values first.
    let gain_mult = if config.bypassed { 1.0 } else { db_to_linear_mult(config.output_gain) };
    let eq_sub = if config.bypassed { 0 } else { config.eq_sub };
    let eq_bass = if config.bypassed { 0 } else { config.eq_bass };
    let eq_mid = if config.bypassed { 0 } else { config.eq_mid };
    let eq_treble = if config.bypassed { 0 } else { config.eq_treble };
    let eq_air = if config.bypassed { 0 } else { config.eq_air };

    format!(
        r#"# Managed by Pipe Deck — do not edit by hand, changes are overwritten on Apply.
context.modules = [
    {{ name = libpipewire-module-filter-chain
        flags = [ nofail ]
        args = {{
            node.description = "{node_description}"
            media.name       = "{node_description}"
            filter.graph = {{
                nodes = [
                    {{ type = builtin name = eq_sub    label = bq_lowshelf  control = {{ "Freq" = 60.0    "Q" = 1.0 "Gain" = {eq_sub} }} }}
                    {{ type = builtin name = eq_bass   label = bq_peaking   control = {{ "Freq" = 150.0   "Q" = 1.0 "Gain" = {eq_bass} }} }}
                    {{ type = builtin name = eq_mid    label = bq_peaking   control = {{ "Freq" = 1000.0  "Q" = 1.0 "Gain" = {eq_mid} }} }}
                    {{ type = builtin name = eq_treble label = bq_peaking   control = {{ "Freq" = 4000.0  "Q" = 1.0 "Gain" = {eq_treble} }} }}
                    {{ type = builtin name = eq_air    label = bq_highshelf control = {{ "Freq" = 10000.0 "Q" = 1.0 "Gain" = {eq_air} }} }}
                    {{ type = builtin name = out_gain  label = linear       control = {{ "Mult" = {gain_mult} }} }}
                ]
                links = [
                    {{ output = "eq_sub:Out"    input = "eq_bass:In" }}
                    {{ output = "eq_bass:Out"   input = "eq_mid:In" }}
                    {{ output = "eq_mid:Out"    input = "eq_treble:In" }}
                    {{ output = "eq_treble:Out" input = "eq_air:In" }}
                    {{ output = "eq_air:Out"    input = "out_gain:In" }}
                ]
            }}
            audio.channels = 2
            audio.position = [ FL FR ]
            capture.props = {{
                node.name   = "{device_system_name}"
                media.class = Audio/Sink
            }}
            playback.props = {{
                node.name    = "effect_output.{device_system_name}"
                node.passive = true
            }}
        }}
    }}
]
"#
    )
}

fn db_to_linear_mult(db: i32) -> f64 {
    10f64.powf(f64::from(db) / 20.0)
}

/// The exact `(control_name, value)` pairs to push through `pw_cli::set_params`
/// for a live slider update — node/control names must match `render_conf`'s
/// filter-graph node names exactly, since this is talking to the same live
/// filter-chain instance without ever re-reading the conf file.
///
/// When `bypassed`, this pushes neutral values (0 gain, unity mult) instead
/// of the configured ones — the chain stays loaded and every link stays
/// exactly as it is, only the audible effect goes away. That's the whole
/// mechanism behind "mute effects without touching the link".
pub fn live_params(config: &EffectChainConfig) -> Vec<(String, f64)> {
    if config.bypassed {
        return vec![
            ("eq_sub:Gain".to_string(), 0.0),
            ("eq_bass:Gain".to_string(), 0.0),
            ("eq_mid:Gain".to_string(), 0.0),
            ("eq_treble:Gain".to_string(), 0.0),
            ("eq_air:Gain".to_string(), 0.0),
            ("out_gain:Mult".to_string(), 1.0),
        ];
    }

    vec![
        ("eq_sub:Gain".to_string(), f64::from(config.eq_sub)),
        ("eq_bass:Gain".to_string(), f64::from(config.eq_bass)),
        ("eq_mid:Gain".to_string(), f64::from(config.eq_mid)),
        ("eq_treble:Gain".to_string(), f64::from(config.eq_treble)),
        ("eq_air:Gain".to_string(), f64::from(config.eq_air)),
        ("out_gain:Mult".to_string(), db_to_linear_mult(config.output_gain)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::DynamicsStage;

    fn capabilities(builtin_eq: bool) -> FxCapabilities {
        FxCapabilities {
            builtin_eq,
            builtin_gain: builtin_eq,
            builtin_limiter: false,
            ladspa_noise_gate: None,
        }
    }

    #[test]
    fn accepts_in_range_eq_when_builtin_present() {
        let config = EffectChainConfig {
            eq_bass: 3,
            ..Default::default()
        };
        let result = preflight(&config, &capabilities(true));
        assert!(result.ok);
        assert!(result.blocking_reasons.is_empty());
    }

    #[test]
    fn rejects_out_of_range_eq_gain() {
        let config = EffectChainConfig {
            eq_bass: 40,
            ..Default::default()
        };
        let result = preflight(&config, &capabilities(true));
        assert!(!result.ok);
        assert!(result.blocking_reasons.iter().any(|reason| reason.contains("Bass band")));
    }

    #[test]
    fn rejects_eq_when_builtin_filter_chain_module_missing() {
        let config = EffectChainConfig {
            eq_bass: 3,
            ..Default::default()
        };
        let result = preflight(&config, &capabilities(false));
        assert!(!result.ok);
        assert!(result.blocking_reasons.iter().any(|reason| reason.contains("builtin filter-chain")));
    }

    #[test]
    fn rejects_limiter_since_no_pipewire_version_ships_a_builtin_one() {
        let config = EffectChainConfig {
            limiter: DynamicsStage { enabled: true, threshold_db: -18, ..Default::default() },
            ..Default::default()
        };
        let result = preflight(&config, &capabilities(true));
        assert!(!result.ok);
        assert!(result.blocking_reasons.iter().any(|reason| reason.contains("Limiter")));
    }

    #[test]
    fn rejects_noise_gate_without_a_discovered_ladspa_plugin() {
        let config = EffectChainConfig {
            noise_gate: DynamicsStage { enabled: true, threshold_db: -30, ..Default::default() },
            ..Default::default()
        };
        let result = preflight(&config, &capabilities(true));
        assert!(!result.ok);
        assert!(result.blocking_reasons.iter().any(|reason| reason.contains("Noise gate")));
    }

    #[test]
    fn allows_noise_gate_when_a_ladspa_plugin_is_discovered() {
        let config = EffectChainConfig {
            noise_gate: DynamicsStage { enabled: true, threshold_db: -30, ..Default::default() },
            ..Default::default()
        };
        let mut caps = capabilities(true);
        caps.ladspa_noise_gate = Some("/usr/lib/ladspa/librnnoise_ladspa.so".to_string());
        let result = preflight(&config, &caps);
        assert!(result.ok);
    }

    #[test]
    fn rejects_out_of_range_dynamics_threshold() {
        let config = EffectChainConfig {
            noise_gate: DynamicsStage { enabled: true, threshold_db: -90, ..Default::default() },
            ..Default::default()
        };
        let mut caps = capabilities(true);
        caps.ladspa_noise_gate = Some("/usr/lib/ladspa/librnnoise_ladspa.so".to_string());
        let result = preflight(&config, &caps);
        assert!(!result.ok);
        assert!(result.blocking_reasons.iter().any(|reason| reason.contains("Threshold")));
    }

    #[test]
    fn render_conf_never_contains_ffmpeg_or_acompressor() {
        let config = EffectChainConfig {
            eq_bass: 6,
            output_gain: -3,
            ..Default::default()
        };
        let rendered = render_conf("pipe-deck-game", &config);
        assert!(!rendered.to_lowercase().contains("ffmpeg"));
        assert!(!rendered.to_lowercase().contains("acompressor"));
        assert!(rendered.contains("nofail"));
        assert!(rendered.contains("bq_peaking"));
    }

    #[test]
    fn render_conf_is_deterministic_for_idempotence_checks() {
        let config = EffectChainConfig {
            eq_sub: 2,
            ..Default::default()
        };
        assert_eq!(
            render_conf("pipe-deck-mic", &config),
            render_conf("pipe-deck-mic", &config)
        );
    }

    #[test]
    fn bypass_pushes_neutral_live_params_regardless_of_configured_values() {
        let config = EffectChainConfig {
            eq_bass: 6,
            eq_treble: -8,
            output_gain: 4,
            bypassed: true,
            ..Default::default()
        };
        for (name, value) in live_params(&config) {
            if name == "out_gain:Mult" {
                assert_eq!(value, 1.0, "bypassed output gain should be unity");
            } else {
                assert_eq!(value, 0.0, "bypassed {name} should be neutral");
            }
        }
    }

    #[test]
    fn bypass_bakes_neutral_values_into_the_initial_structural_apply_too() {
        let config = EffectChainConfig {
            eq_bass: 6,
            bypassed: true,
            ..Default::default()
        };
        let rendered = render_conf("pipe-deck-game", &config);
        assert!(rendered.contains("\"Gain\" = 0"));
        assert!(!rendered.contains("\"Gain\" = 6"));
    }

    #[test]
    fn live_params_control_names_match_render_conf_node_names() {
        let config = EffectChainConfig {
            eq_bass: 6,
            output_gain: 0,
            ..Default::default()
        };
        let rendered = render_conf("pipe-deck-game", &config);
        for (name, _value) in live_params(&config) {
            let node_name = name.split(':').next().unwrap();
            assert!(
                rendered.contains(&format!("name = {node_name} ")),
                "render_conf is missing a node for live param {name:?}"
            );
        }
    }
}
