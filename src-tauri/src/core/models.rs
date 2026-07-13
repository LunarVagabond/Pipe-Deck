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
/// volume). Backed by a per-pair feed sink; see `pipewire::virtual_mic_mix`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MixSource {
    pub device_id: String,
    #[serde(default = "default_mix_volume")]
    pub volume_percent: u8,
}

fn default_mix_volume() -> u8 {
    100
}

impl MixSource {
    pub fn unity(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            volume_percent: default_mix_volume(),
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub current_targets: Vec<String>,
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
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_system_streams: false,
            restore_on_startup: true,
            background_restore: false,
            auto_apply_rules: true,
        }
    }
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
}

impl MixSourceSpec {
    pub fn unity(system_name: impl Into<String>) -> Self {
        Self {
            system_name: system_name.into(),
            volume_percent: default_mix_volume(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
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

impl Stream {
    pub fn resolved_targets(&self) -> Vec<String> {
        if !self.current_targets.is_empty() {
            return self.current_targets.clone();
        }
        self.current_target.clone().into_iter().collect()
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EffectChainConfig {
    /// Sub bass (~60 Hz).
    #[serde(default)]
    pub eq_sub: i32,
    /// Bass (~150 Hz). Legacy configs may use `eq_low`.
    #[serde(default, alias = "eq_low")]
    pub eq_bass: i32,
    #[serde(default)]
    pub eq_mid: i32,
    /// Treble (~4 kHz).
    #[serde(default)]
    pub eq_treble: i32,
    /// Air / presence (~10 kHz). Legacy configs may use `eq_high`.
    #[serde(default, alias = "eq_high")]
    pub eq_air: i32,
    /// Master trim in dB (-12..+12).
    #[serde(default)]
    pub output_gain: i32,
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub compressor: DynamicsStage,
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub limiter: DynamicsStage,
    /// Modeled now; stays UI-disabled until `fx_capability` confirms a real
    /// backing plugin is present on the host (see `pipewire::fx_capability`).
    #[serde(default, deserialize_with = "deserialize_dynamics_stage")]
    pub noise_gate: DynamicsStage,
}

impl EffectChainConfig {
    pub fn is_active(&self) -> bool {
        self.compressor.enabled
            || self.limiter.enabled
            || self.noise_gate.enabled
            || self.eq_sub != 0
            || self.eq_bass != 0
            || self.eq_mid != 0
            || self.eq_treble != 0
            || self.eq_air != 0
            || self.output_gain != 0
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
    pub enabled: bool,
    pub requested_capabilities: Vec<String>,
    pub granted_capabilities: Vec<String>,
    pub runtime_status: PluginRuntimeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub ui_panels: Vec<PluginUiPanel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginUiPanel {
    pub id: String,
    pub title: String,
    pub summary: String,
}
