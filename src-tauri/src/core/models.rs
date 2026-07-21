use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Physical,
    Virtual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceDirection {
    Input,
    Output,
    Duplex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StreamDirection {
    Playback,
    Capture,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SinkMode {
    Single,
    Multi,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    /// Stable PipeWire node name used for routing and config aliases.
    pub system_name: String,
    /// User-facing label (alias override or derived system name).
    pub label: String,
    pub kind: DeviceKind,
    pub direction: DeviceDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_mode: Option<SinkMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub current_targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mix_sources: Vec<MixSource>,
}

/// A single contributor to a virtual-mic mix, with a gain that only affects
/// its contribution to that specific mix (not the source device's own
/// volume). Backed by a per-pair feed sink; see `backend::linux::virtual_mic_mix`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixSource {
    pub device_id: String,
    #[serde(default = "default_mix_volume")]
    pub volume_percent: u8,
    /// Silences this source's contribution to the mix without touching the
    /// link itself — the feed sink and its port connections stay exactly as
    /// they are, only its own mute flag toggles (see `pactl::set_sink_mute_by_name`).
    #[serde(default)]
    pub muted: bool,
}

fn default_mix_volume() -> u8 {
    100
}

impl MixSource {
    pub fn unity(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            volume_percent: default_mix_volume(),
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stream {
    pub id: String,
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,
    pub direction: StreamDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    #[serde(default)]
    pub is_system: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_explanation: Option<RouteExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentStreamIdentity {
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    pub direction: StreamDirection,
    #[serde(default)]
    pub is_system: bool,
    pub last_seen_secs: u64,
    #[serde(default)]
    pub is_live: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RuntimeGraph {
    pub devices: Vec<Device>,
    pub streams: Vec<Stream>,
    pub links: Vec<Link>,
    #[serde(default = "default_data_source")]
    pub data_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_stream_identities: Vec<RecentStreamIdentity>,
}

fn default_data_source() -> String {
    "pipewire".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIndexEntry {
    pub id: String,
    pub name: String,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAliasEntry {
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub show_system_streams: bool,
    #[serde(default = "default_true")]
    pub restore_on_startup: bool,
    #[serde(default)]
    pub background_restore: bool,
    #[serde(default = "default_true")]
    pub auto_apply_rules: bool,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(default = "default_theme_mode")]
    pub theme_mode: String,
    #[serde(default = "default_dark_scheme")]
    pub dark_scheme: String,
    #[serde(default = "default_light_scheme")]
    pub light_scheme: String,
}

fn default_theme_mode() -> String {
    "dark".into()
}

fn default_dark_scheme() -> String {
    "midnight-deck".into()
}

fn default_light_scheme() -> String {
    "paper-deck".into()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_system_streams: false,
            restore_on_startup: true,
            background_restore: false,
            auto_apply_rules: true,
            sidebar_collapsed: false,
            theme_mode: default_theme_mode(),
            dark_scheme: default_dark_scheme(),
            light_scheme: default_light_scheme(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeBase {
    Light,
    Dark,
}

/// A fully-resolved color palette — every field present, no fallbacks left to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeColors {
    pub background: String,
    pub surface_1: String,
    pub surface_2: String,
    pub border: String,
    pub text: String,
    pub text_muted: String,
    pub accent_purple: String,
    pub accent_teal: String,
    pub accent_amber: String,
    pub status_success: String,
    pub status_warning: String,
    pub status_danger: String,
}

/// A partial palette as authored by a user — any key left unset falls back to the
/// built-in default for the scheme's declared `base`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeColorOverrides {
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub surface_1: Option<String>,
    #[serde(default)]
    pub surface_2: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub text_muted: Option<String>,
    #[serde(default)]
    pub accent_purple: Option<String>,
    #[serde(default)]
    pub accent_teal: Option<String>,
    #[serde(default)]
    pub accent_amber: Option<String>,
    #[serde(default)]
    pub status_success: Option<String>,
    #[serde(default)]
    pub status_warning: Option<String>,
    #[serde(default)]
    pub status_danger: Option<String>,
}

impl ThemeColorOverrides {
    /// Merges these overrides on top of `base`, keeping `base`'s value for anything unset.
    pub fn resolve(&self, base: &ThemeColors) -> ThemeColors {
        ThemeColors {
            background: self.background.clone().unwrap_or_else(|| base.background.clone()),
            surface_1: self.surface_1.clone().unwrap_or_else(|| base.surface_1.clone()),
            surface_2: self.surface_2.clone().unwrap_or_else(|| base.surface_2.clone()),
            border: self.border.clone().unwrap_or_else(|| base.border.clone()),
            text: self.text.clone().unwrap_or_else(|| base.text.clone()),
            text_muted: self.text_muted.clone().unwrap_or_else(|| base.text_muted.clone()),
            accent_purple: self.accent_purple.clone().unwrap_or_else(|| base.accent_purple.clone()),
            accent_teal: self.accent_teal.clone().unwrap_or_else(|| base.accent_teal.clone()),
            accent_amber: self.accent_amber.clone().unwrap_or_else(|| base.accent_amber.clone()),
            status_success: self.status_success.clone().unwrap_or_else(|| base.status_success.clone()),
            status_warning: self.status_warning.clone().unwrap_or_else(|| base.status_warning.clone()),
            status_danger: self.status_danger.clone().unwrap_or_else(|| base.status_danger.clone()),
        }
    }
}

/// On-disk shape of a user scheme file under `<config_dir>/themes/*.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomThemeFile {
    pub name: String,
    pub base: ThemeBase,
    #[serde(default)]
    pub colors: ThemeColorOverrides,
}

/// A scheme ready for the frontend to apply — built-in or custom, always fully resolved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedScheme {
    pub id: String,
    pub name: String,
    pub kind: ThemeBase,
    pub source: ThemeSchemeSource,
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSchemeSource {
    Builtin,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VirtualDeviceSpec {
    pub id: String,
    pub slug: String,
    pub label: String,
    pub direction: DeviceDirection,
    pub created_at: String,
    #[serde(default)]
    pub multi: bool,
    /// Physical capture device system names mixed into this virtual input (e.g. headset mic),
    /// each with its own gain (applied via a per-pair feed sink, not the source's own volume).
    #[serde(default, deserialize_with = "deserialize_mix_source_specs", skip_serializing_if = "Vec::is_empty")]
    pub mix_sources: Vec<MixSourceSpec>,
}

/// A persisted mix contributor, keyed by system name (not a runtime device id,
/// which isn't stable across PipeWire restarts/config reloads).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixSourceSpec {
    pub system_name: String,
    #[serde(default = "default_mix_volume")]
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
}

impl MixSourceSpec {
    pub fn unity(system_name: impl Into<String>) -> Self {
        Self {
            system_name: system_name.into(),
            volume_percent: default_mix_volume(),
            muted: false,
        }
    }
}

/// Accepts either the legacy `Vec<String>` shape (bare system names, unity
/// gain) or the current `Vec<MixSourceSpec>` shape, so existing saved configs
/// keep loading after this field grew a volume.
fn deserialize_mix_source_specs<'de, D>(deserializer: D) -> Result<Vec<MixSourceSpec>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Legacy(String),
        Sourced(MixSourceSpec),
    }

    let entries: Vec<Entry> = Vec::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Legacy(system_name) => MixSourceSpec::unity(system_name),
            Entry::Sourced(spec) => spec,
        })
        .collect())
}

fn default_config_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
    pub active_profile: Option<String>,
    pub profile_index: Vec<ProfileIndexEntry>,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub devices: std::collections::HashMap<String, DeviceAliasEntry>,
    #[serde(default)]
    pub routing_rules: RoutingRulesConfig,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub virtual_devices: Vec<VirtualDeviceSpec>,
    #[serde(default)]
    pub plugins: std::collections::HashMap<String, PluginEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteSource {
    ManualOverride,
    PersistedRule,
    AuthoredRule,
    NoRule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Applied,
    Blocked,
    SkippedManualOverride,
    TargetUnavailable,
    Simulated,
    NoAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedCandidate {
    pub rule_key: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteExplanation {
    pub source: RouteSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule_key: Option<String>,
    pub match_reasons: Vec<String>,
    pub skipped_candidates: Vec<SkippedCandidate>,
    pub action_status: ActionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_system_names: Vec<String>,
    #[serde(default)]
    pub fallback_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamRouteRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_system_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
    pub action: RuleAction,
    #[serde(default)]
    pub safeguards: RuleSafeguards,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleCondition {
    AppName { value: String },
    Executable { value: String },
    WindowClass { value: String },
    MediaName { value: String },
    Direction { value: StreamDirection },
    Category { value: String },
    /// Matches when app name, executable, or PipeWire node name equals `value` (case-insensitive).
    Identity { value: String },
    Regex { field: String, pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleAction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_system_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackPolicy {
    KeepCurrent,
    SafeDefault,
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self::KeepCurrent
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSafeguards {
    #[serde(default)]
    pub fallback_policy: FallbackPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SimulationResult {
    pub stream_id: String,
    pub stream_label: String,
    #[serde(default)]
    pub is_recent: bool,
    pub explanation: RouteExplanation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub would_target_device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRouteRule {
    pub source_system_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_system_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_system_names: Vec<String>,
    #[serde(default)]
    pub safeguards: RuleSafeguards,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingRulesConfig {
    #[serde(default)]
    pub stream_rules: Vec<StreamRouteRule>,
    #[serde(default)]
    pub device_rules: Vec<DeviceRouteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRouteIntent {
    pub source_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_device_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingIntent {
    pub stream_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_device_ids: Vec<String>,
}

impl RoutingIntent {
    pub fn target_ids(&self) -> Vec<String> {
        if !self.target_device_ids.is_empty() {
            return self.target_device_ids.clone();
        }
        self.target_device_id.clone().into_iter().collect()
    }
}

impl Device {
    pub fn is_multi_sink(&self) -> bool {
        self.sink_mode == Some(SinkMode::Multi)
    }

    pub fn resolved_targets(&self) -> Vec<String> {
        if !self.current_targets.is_empty() {
            return self.current_targets.clone();
        }
        self.current_target.clone().into_iter().collect()
    }
}

impl DeviceRouteIntent {
    pub fn target_ids(&self) -> Vec<String> {
        if !self.target_device_ids.is_empty() {
            return self.target_device_ids.clone();
        }
        self.target_device_id.clone().into_iter().collect()
    }
}

impl DeviceRouteRule {
    pub fn target_system_names_resolved(&self) -> Vec<String> {
        if !self.target_system_names.is_empty() {
            return self.target_system_names.clone();
        }
        self.target_system_name.clone().into_iter().collect()
    }
}

impl StreamRouteRule {
    pub fn target_system_names_resolved(&self) -> Vec<String> {
        if !self.target_system_names.is_empty() {
            return self.target_system_names.clone();
        }
        self.target_system_name.clone().into_iter().collect()
    }
}

impl RuleAction {
    pub fn target_system_names_resolved(&self) -> Vec<String> {
        if !self.target_system_names.is_empty() {
            return self.target_system_names.clone();
        }
        self.target_system_name.clone().into_iter().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeStateEntry {
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
}

/// A single dynamics processing block (compressor, limiter, or noise gate).
/// `ratio_x10` is fixed-point (20 == 2.0:1) so the struct can derive `Eq`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DynamicsStage {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub threshold_db: i32,
    #[serde(default)]
    pub ratio_x10: i32,
    #[serde(default)]
    pub attack_ms: i32,
    #[serde(default)]
    pub release_ms: i32,
}

/// Accepts either a legacy bare bool (`compressor: true`) or the current
/// `DynamicsStage` object, so existing saved configs keep loading after this
/// field grew real parameters.
fn deserialize_dynamics_stage<'de, D>(deserializer: D) -> Result<DynamicsStage, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        LegacyBool(bool),
        Stage(DynamicsStage),
    }

    match Entry::deserialize(deserializer)? {
        Entry::LegacyBool(enabled) => Ok(DynamicsStage {
            enabled,
            ..Default::default()
        }),
        Entry::Stage(stage) => Ok(stage),
    }
}

/// One addable/removable/reorderable unit in a device's effect chain.
/// `id` is a stable, client-generated identifier used as the Vue `:key` for
/// reorder/remove — not derived from the stage's kind or contents, so
/// reordering/removing survives value edits.
///
/// v1 ships exactly one variant. Compressor/limiter/noise gate stay as
/// standalone `EffectChainConfig` fields (see below), not stages — they're
/// unconditionally blocked by `fx_validate::preflight` until a real backing
/// plugin exists (#86/#18), so there's no functional stage-kind shape to
/// design for them yet; promoting them into `stages` is deferred to whenever
/// one is actually unblocked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum EffectStage {
    #[serde(rename = "eq5band")]
    Eq5Band {
        id: String,
        /// Sub bass (~60 Hz).
        #[serde(default)]
        eq_sub: i32,
        /// Bass (~150 Hz). Legacy configs may use `eq_low`.
        #[serde(default, alias = "eq_low")]
        eq_bass: i32,
        #[serde(default)]
        eq_mid: i32,
        /// Treble (~4 kHz).
        #[serde(default)]
        eq_treble: i32,
        /// Air / presence (~10 kHz). Legacy configs may use `eq_high`.
        #[serde(default, alias = "eq_high")]
        eq_air: i32,
        /// Master trim in dB (-12..+12).
        #[serde(default)]
        output_gain: i32,
    },
}

impl Default for EffectStage {
    fn default() -> Self {
        EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_sub: 0,
            eq_bass: 0,
            eq_mid: 0,
            eq_treble: 0,
            eq_air: 0,
            output_gain: 0,
        }
    }
}

impl EffectStage {
    pub fn kind(&self) -> &'static str {
        match self {
            EffectStage::Eq5Band { .. } => "eq5band",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            EffectStage::Eq5Band { id, .. } => id,
        }
    }
}

/// Flattened EQ params for a chain's `Eq5Band` stage (if any) — the shape
/// `pipewire::fx_validate`'s conf/live-param rendering actually needs,
/// independent of `stages`' ordering/id bookkeeping. "No stage" renders as
/// all-neutral values (`Default`), same as a present-but-all-zero stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EqStageParams {
    pub eq_sub: i32,
    pub eq_bass: i32,
    pub eq_mid: i32,
    pub eq_treble: i32,
    pub eq_air: i32,
    pub output_gain: i32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct EffectChainConfig {
    /// Ordered, addable/removable/reorderable. v1: 0 or 1 `Eq5Band` entries.
    #[serde(default)]
    pub stages: Vec<EffectStage>,
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub compressor: DynamicsStage,
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub limiter: DynamicsStage,
    /// Modeled now; stays UI-disabled until `fx_capability` confirms a real
    /// backing plugin is present on the host (see `pipewire::fx_capability`).
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub noise_gate: DynamicsStage,
    /// Keeps the chain configured (and, once live processing exists, keeps
    /// its filter graph loaded) but passes audio through unprocessed. This is
    /// a live param, not a topology change — toggling it never needs a
    /// PipeWire restart once the live path is wired, unlike enabling/disabling
    /// individual stages.
    #[serde(default)]
    pub bypassed: bool,
    /// Whether the user has explicitly confirmed live processing for this
    /// chain (a real `apply_effect_chain_structural` succeeded and hasn't
    /// been reverted since) — distinct from `is_active()`, which only means
    /// "something is configured." PD-017 §1 requires restore to never
    /// silently turn on live processing that wasn't explicitly confirmed;
    /// before native transport this was inferred from a restart-based
    /// conf.d file surviving on disk across restarts, but native transport's
    /// liveness is in-memory in the daemon process and doesn't survive that
    /// way, so this flag is the persisted signal instead. `#[serde(default)]`
    /// means every pre-existing profile deserializes as `false` — an
    /// accepted one-time transition: those chains stay persist-only until
    /// re-applied once after upgrading, rather than guessing at legacy
    /// conf-file state.
    #[serde(default)]
    pub live: bool,
}

/// Accepts either the current `{ stages: [...], ... }` shape or a pre-PD-025
/// config with flat `eq_sub`/`eq_bass`/.../`output_gain` fields at the top
/// level — synthesizing a single `Eq5Band` stage (deterministic id `"eq"`,
/// since v1 only ever has zero or one) from the legacy fields so existing
/// saved profiles keep loading with no manual migration step, the same
/// established pattern as `deserialize_mix_source_specs`/
/// `deserialize_dynamics_stage` in this file.
impl<'de> Deserialize<'de> for EffectChainConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OnDisk {
            #[serde(default)]
            stages: Vec<EffectStage>,
            #[serde(default)]
            eq_sub: i32,
            #[serde(default, alias = "eq_low")]
            eq_bass: i32,
            #[serde(default)]
            eq_mid: i32,
            #[serde(default)]
            eq_treble: i32,
            #[serde(default, alias = "eq_high")]
            eq_air: i32,
            #[serde(default)]
            output_gain: i32,
            #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
            compressor: DynamicsStage,
            #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
            limiter: DynamicsStage,
            #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
            noise_gate: DynamicsStage,
            #[serde(default)]
            bypassed: bool,
            #[serde(default)]
            live: bool,
        }

        let raw = OnDisk::deserialize(deserializer)?;
        let legacy_eq_active = raw.eq_sub != 0
            || raw.eq_bass != 0
            || raw.eq_mid != 0
            || raw.eq_treble != 0
            || raw.eq_air != 0
            || raw.output_gain != 0;

        let stages = if !raw.stages.is_empty() {
            raw.stages
        } else if legacy_eq_active {
            vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: raw.eq_sub,
                eq_bass: raw.eq_bass,
                eq_mid: raw.eq_mid,
                eq_treble: raw.eq_treble,
                eq_air: raw.eq_air,
                output_gain: raw.output_gain,
            }]
        } else {
            Vec::new()
        };

        Ok(EffectChainConfig {
            stages,
            compressor: raw.compressor,
            limiter: raw.limiter,
            noise_gate: raw.noise_gate,
            bypassed: raw.bypassed,
            live: raw.live,
        })
    }
}

impl EffectChainConfig {
    /// Whether this chain has any non-default settings worth persisting.
    /// Deliberately independent of `bypassed` — bypassing mutes the chain's
    /// effect on audio without discarding its configured settings, so a
    /// bypassed-but-configured chain still counts as active here.
    pub fn is_active(&self) -> bool {
        self.compressor.enabled || self.limiter.enabled || self.noise_gate.enabled || !self.stages.is_empty()
    }

    /// The chain's `Eq5Band` stage, flattened — "no stage" is treated the
    /// same as a present-but-neutral one by every caller.
    // `find_map` looks unnecessary with only one `EffectStage` variant today,
    // but this is deliberately shaped to keep working once a second variant
    // exists (see `EffectStage`'s doc comment) — `.map().next()` would need
    // rewriting again at that point instead of just adding a match arm.
    #[allow(clippy::unnecessary_find_map)]
    pub fn eq_stage(&self) -> EqStageParams {
        self.stages
            .iter()
            .find_map(|stage| match stage {
                EffectStage::Eq5Band {
                    eq_sub, eq_bass, eq_mid, eq_treble, eq_air, output_gain, ..
                } => Some(EqStageParams {
                    eq_sub: *eq_sub,
                    eq_bass: *eq_bass,
                    eq_mid: *eq_mid,
                    eq_treble: *eq_treble,
                    eq_air: *eq_air,
                    output_gain: *output_gain,
                }),
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
    pub routing_intents: Vec<RoutingIntent>,
    #[serde(default)]
    pub volume_state: std::collections::HashMap<String, VolumeStateEntry>,
    #[serde(default)]
    pub device_assumptions: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub effect_state: std::collections::HashMap<String, EffectChainConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExportManifest {
    pub version: u32,
    pub exported_at: String,
    pub profile_id: String,
    pub profile_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDriftItem {
    pub stream_id: String,
    pub stream_label: String,
    pub live_target_id: Option<String>,
    pub live_target_label: Option<String>,
    pub desired_target_id: Option<String>,
    pub desired_target_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDrift {
    pub profile_id: String,
    pub profile_name: String,
    pub has_drift: bool,
    pub items: Vec<RoutingDriftItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualDeviceResult {
    pub device_id: String,
    pub system_name: String,
    pub label: String,
    #[serde(default)]
    pub multi: bool,
}

/// Backend-neutral virtual device bookkeeping info returned by
/// `AudioBackend::list_virtual_devices` — deliberately smaller than
/// `backend::linux::virtual_devices::VirtualDeviceEntry` (no `module_id`,
/// a pactl-only implementation detail). Never serialized to the frontend.
#[derive(Debug, Clone)]
pub struct VirtualDeviceInfo {
    pub device_id: String,
    pub system_name: String,
    pub label: String,
    pub direction: DeviceDirection,
    pub multi: bool,
}

impl VirtualDeviceInfo {
    pub fn to_device(&self) -> Device {
        Device {
            id: self.device_id.clone(),
            system_name: self.system_name.clone(),
            label: self.label.clone(),
            kind: DeviceKind::Virtual,
            direction: self.direction.clone(),
            sink_mode: match self.direction {
                DeviceDirection::Output | DeviceDirection::Duplex => Some(if self.multi {
                    SinkMode::Multi
                } else {
                    SinkMode::Single
                }),
                DeviceDirection::Input => None,
            },
            volume_percent: Some(100),
            muted: Some(false),
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileState {
    Missing,
    Present,
    StaleConfigRef,
    OrphanModule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub created: Vec<String>,
    pub adopted: Vec<String>,
    pub removed_orphans: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub enabled: bool,
    pub pid: Option<u32>,
    pub last_run: Option<String>,
    pub last_error: Option<String>,
    pub devices_restored: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub entry: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub bundled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub granted_capabilities: Vec<String>,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Default for PluginEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            granted_capabilities: Vec::new(),
            config: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginRuntimeStatus {
    Stopped,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatus {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub bundled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub enabled: bool,
    pub requested_capabilities: Vec<String>,
    pub granted_capabilities: Vec<String>,
    pub runtime_status: PluginRuntimeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub ui_panels: Vec<PluginUiPanel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginUiPanel {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityInfo {
    pub id: String,
    pub description: String,
    pub enforced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDiscoveryIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectsApplyRequest {
    pub device_id: String,
    pub config: EffectChainConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingSuggestion {
    pub plugin_id: String,
    pub stream_id: String,
    pub target_system_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub received_at: String,
}
