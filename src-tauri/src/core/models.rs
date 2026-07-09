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
pub struct Device {
    pub id: String,
    /// Stable PipeWire node name used for routing and config aliases.
    pub system_name: String,
    /// User-facing label (alias override or derived system name).
    pub label: String,
    pub kind: DeviceKind,
    pub direction: DeviceDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stream {
    pub id: String,
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_name: Option<String>,
    pub direction: StreamDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
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
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_system_streams: false,
        }
    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamRouteRule {
    pub app_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    pub target_system_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceRouteRule {
    pub source_system_name: String,
    pub target_system_name: String,
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
    pub target_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingIntent {
    pub stream_id: String,
    pub target_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeStateEntry {
    pub volume_percent: u8,
    #[serde(default)]
    pub muted: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExportManifest {
    pub version: u32,
    pub exported_at: String,
    pub profile_id: String,
    pub profile_name: String,
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
}
