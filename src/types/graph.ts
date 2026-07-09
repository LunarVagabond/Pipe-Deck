export type DeviceKind = "physical" | "virtual";
export type DeviceDirection = "input" | "output" | "duplex";
export type StreamDirection = "playback" | "capture";

export type RouteSource = "manual_override" | "persisted_rule" | "authored_rule" | "no_rule";
export type ActionStatus =
  | "applied"
  | "blocked"
  | "skipped_manual_override"
  | "target_unavailable"
  | "simulated"
  | "no_action";

export interface SkippedCandidate {
  rule_key: string;
  reason: string;
}

export interface RouteExplanation {
  source: RouteSource;
  matched_rule_id?: string;
  matched_rule_key?: string;
  match_reasons: string[];
  skipped_candidates: SkippedCandidate[];
  action_status: ActionStatus;
  target_system_name?: string;
}

export interface Device {
  id: string;
  system_name: string;
  label: string;
  kind: DeviceKind;
  direction: DeviceDirection;
  volume_percent?: number;
  muted?: boolean;
  current_target?: string;
}

export interface Stream {
  id: string;
  app_name: string;
  executable?: string;
  window_class?: string;
  system_name?: string;
  direction: StreamDirection;
  current_target?: string;
  media_name?: string;
  is_system?: boolean;
  route_explanation?: RouteExplanation;
}

export interface Link {
  id: string;
  source_id: string;
  target_id: string;
}

export interface RuntimeGraph {
  devices: Device[];
  streams: Stream[];
  links: Link[];
  data_source?: string;
  notice?: string;
}

export interface DeviceAliasEntry {
  alias: string;
}

export interface ProfileIndexEntry {
  id: string;
  name: string;
  file: string;
}

export interface Preferences {
  show_system_streams?: boolean;
}

export interface StreamRouteRule {
  app_name?: string;
  executable?: string;
  media_name?: string;
  target_system_name: string;
}

export interface DeviceRouteRule {
  source_system_name: string;
  target_system_name: string;
}

export interface RoutingRulesConfig {
  stream_rules: StreamRouteRule[];
  device_rules: DeviceRouteRule[];
}

export type RuleCondition =
  | { type: "app_name"; value: string }
  | { type: "executable"; value: string }
  | { type: "window_class"; value: string }
  | { type: "media_name"; value: string }
  | { type: "direction"; value: StreamDirection }
  | { type: "category"; value: string }
  | { type: "regex"; field: string; pattern: string };

export interface RuleAction {
  target_system_name: string;
}

export interface RuleSafeguards {
  fallback_policy?: "keep_current" | "safe_default";
}

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  conditions: RuleCondition[];
  action: RuleAction;
  safeguards?: RuleSafeguards;
}

export interface SimulationResult {
  stream_id: string;
  explanation: RouteExplanation;
  would_target_device_id?: string;
}

export interface AppConfig {
  version: number;
  active_profile?: string;
  profile_index: ProfileIndexEntry[];
  preferences?: Preferences;
  devices?: Record<string, DeviceAliasEntry>;
  routing_rules?: RoutingRulesConfig;
  rules?: Rule[];
}

export interface RoutingIntent {
  stream_id: string;
  target_device_id: string;
}

export interface VolumeStateEntry {
  volume_percent: number;
  muted?: boolean;
}

export interface Profile {
  version: number;
  id: string;
  name: string;
  created: string;
  updated: string;
  routing_intents: RoutingIntent[];
  volume_state?: Record<string, VolumeStateEntry>;
  device_assumptions?: Record<string, string>;
}

export interface ApplyResult {
  success: boolean;
  message?: string;
}

export interface VirtualDeviceResult {
  device_id: string;
  system_name: string;
  label: string;
}

export type AppView =
  | "dashboard"
  | "profiles"
  | "rules"
  | "routing"
  | "mixer"
  | "sources"
  | "effects"
  | "settings";
