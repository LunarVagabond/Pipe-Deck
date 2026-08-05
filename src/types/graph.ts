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

export type SkipReason = "lower_priority" | "manual_override" | "missing_metadata";

export interface SkippedCandidate {
  rule_key: string;
  reason: string;
  rule_id?: string;
  kind: SkipReason;
}

export interface RouteExplanation {
  source: RouteSource;
  matched_rule_id?: string;
  matched_rule_key?: string;
  match_reasons: string[];
  skipped_candidates: SkippedCandidate[];
  action_status: ActionStatus;
  target_system_name?: string;
  target_system_names?: string[];
  fallback_applied: boolean;
}

export type SinkMode = "single" | "multi";

/** A contributor to a virtual-mic mix; volume/mute only affect its share of
 * that one mix (via a per-pair feed sink), not the source's own device
 * volume — muting never touches the underlying link. */
export interface MixSource {
  device_id: string;
  volume_percent: number;
  muted: boolean;
}

export interface Device {
  id: string;
  system_name: string;
  label: string;
  kind: DeviceKind;
  direction: DeviceDirection;
  sink_mode?: SinkMode;
  volume_percent?: number;
  muted?: boolean;
  current_target?: string;
  current_targets?: string[];
  mix_sources?: MixSource[];
  sample_rate?: number;
  channels?: number;
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
  volume_percent?: number;
  muted?: boolean;
  route_explanation?: RouteExplanation;
  sample_rate?: number;
  channels?: number;
}

export interface Link {
  id: string;
  source_id: string;
  target_id: string;
  is_monitor: boolean;
}

export interface RecentStreamIdentity {
  app_name: string;
  executable?: string;
  window_class?: string;
  system_name?: string;
  media_name?: string;
  direction: StreamDirection;
  is_system?: boolean;
  last_seen_secs: number;
  is_live?: boolean;
}

/** One addressable input/output port on a `ProcessingNode`. `index` is the
 * stable address a kind's own per-port config (e.g. Mixer's per-input gain)
 * is keyed by — never re-derived from array position. */
export interface ProcessingNodePort {
  index: number;
  connected_id?: string;
  /** Mixer input only: the original stream id that named this port's feed
   * sink at connect time, kept stable even after `connected_id` is
   * reconciled onto a same-app replacement stream instance (issue #304). */
  feed_key?: string;
}

/** issue #293's non-DSP effect kinds — addable to the graph, wired like any
 * other node, pure pass-through (never backed by a PipeWire object),
 * rendered with a visible "Not implemented yet" label. Originally eleven;
 * `reverb_delay`, `limiter`, `hpf`, and `stereo_widener` graduated to real
 * `delay`/`limiter`/`hpf`/`widener` processing nodes (issues
 * #313/#311/#312/#314), the same way `eq5band` graduated out of this list
 * previously. */
export type StubEffectKind =
  | "compressor"
  | "noise_gate"
  | "denoise"
  | "de_esser"
  | "auto_gain_leveler"
  | "pitch_shift"
  | "loudness_normalizer"
  | "saturation";

export type ProcessingNodeKind =
  | {
      kind: "mixer";
      /** Per-input gain, indexed by `ProcessingNodePort.index` — see PD-032:
       * this lives once, on the node, never re-synthesized per edge. */
      input_gains_percent: number[];
    }
  | {
      kind: "fan_out";
      /** Plain output volume on the node's own backing sink — not a
       * shaping gain (Fan-Out has no DSP), same meaning as a device's
       * volume_percent. */
      volume_percent: number;
      muted: boolean;
    }
  | {
      kind: "group";
      /** Created in one atomic gesture from a multi-select of existing
       * output devices (issue #80) rather than wired connection-by-
       * connection like Fan-Out — functionally identical shape/mechanism
       * otherwise (PD-035), including this same plain output volume. */
      volume_percent: number;
      muted: boolean;
    }
  | {
      kind: "eq5band";
      eq_sub: number;
      eq_bass: number;
      eq_mid: number;
      eq_treble: number;
      eq_air: number;
      output_gain: number;
    }
  | {
      kind: "delay";
      delay_ms: number;
      feedback_percent: number;
      feedforward_percent: number;
    }
  | {
      kind: "limiter";
      ceiling_db: number;
      floor_db: number;
      symmetric: boolean;
    }
  | {
      kind: "hpf";
      freq_hz: number;
      resonance_x10: number;
    }
  | {
      kind: "reverb";
      mix_percent: number;
    }
  | {
      kind: "widener";
      width_percent: number;
    }
  | {
      kind: "pan";
      balance_percent: number;
    }
  | { kind: "stub"; stub_kind: StubEffectKind };

/** A first-class routable processing node — Mixer, Fan-out, or a single
 * effect DSP stage (PD-032). Distinct from `Device`/`Stream`: has explicit,
 * independently-addressable input/output ports rather than at most one
 * implicit stream of audio in and out. */
export interface ProcessingNode {
  id: string;
  label: string;
  kind: ProcessingNodeKind;
  system_name: string;
  bypassed: boolean;
  /** Whether a real PipeWire object currently backs this node. Always
   * `false` for `stub` kinds, which are never backed by one. */
  live: boolean;
  inputs?: ProcessingNodePort[];
  outputs?: ProcessingNodePort[];
}

export interface RuntimeGraph {
  devices: Device[];
  streams: Stream[];
  links: Link[];
  data_source?: string;
  notice?: string;
  recent_stream_identities?: RecentStreamIdentity[];
  processing_nodes?: ProcessingNode[];
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
  restore_on_startup?: boolean;
  background_restore?: boolean;
  auto_apply_rules?: boolean;
  sidebar_collapsed?: boolean;
  theme_mode?: string;
  dark_scheme?: string;
  light_scheme?: string;
  notice_duration_ms?: number;
  soundboard_boards?: SoundboardBoard[];
  /** "minimize" | "quit"; absent/null means the user hasn't been asked yet. */
  close_behavior?: string;
  update_channel?: string;
}

export interface SoundboardBoard {
  id: string;
  name: string;
  folder: string;
  /** Board-wide playback destinations — every clip in this tab plays through these. */
  target_system_name: string | null;
  target_volume_percent: number;
  monitor_system_name: string | null;
  monitor_volume_percent: number;
}

export interface SoundboardClip {
  id: string;
  file_name: string;
  label: string;
  path: string;
  /** Probed from the file's own header/metadata; null if it couldn't be read. */
  duration_seconds: number | null;
}

export interface StreamRouteRule {
  app_name?: string;
  executable?: string;
  media_name?: string;
  target_system_name?: string;
  target_system_names?: string[];
}

export interface RoutingRulesConfig {
  stream_rules: StreamRouteRule[];
}

export type RuleCondition =
  | { type: "app_name"; value: string }
  | { type: "executable"; value: string }
  | { type: "window_class"; value: string }
  | { type: "media_name"; value: string }
  | { type: "direction"; value: StreamDirection }
  | { type: "category"; value: string }
  | { type: "identity"; value: string }
  | { type: "regex"; field: string; pattern: string };

export interface RuleAction {
  target_system_name?: string;
  target_system_names?: string[];
}

export interface RuleSafeguards {
  fallback_policy?: "keep_current" | "safe_default";
}

/** One node along a path submitted for latency-ping measurement (issue
 * #223). `system_name` is only needed as a resolution fallback for
 * processing nodes; device/stream ids resolve for free on the Rust side. */
export interface LatencyPathNode {
  id: string;
  system_name?: string;
}

/** Theoretical/buffering latency contribution of one hop, from `pw-top`'s
 * QUANT/RATE. `latency_ms` (and `quantum`/`rate`) are absent when the node
 * has no current reading — distinct from a real zero-latency reading. */
export interface LatencyHop {
  id: string;
  node_id?: number;
  quantum?: number;
  rate?: number;
  latency_ms?: number;
}

export interface LatencyPingResult {
  hops: LatencyHop[];
  total_latency_ms?: number;
}

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  priority: number;
  conditions: RuleCondition[];
  action: RuleAction;
  safeguards: RuleSafeguards;
}

export interface SimulationResult {
  stream_id: string;
  stream_label: string;
  is_recent?: boolean;
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
  virtual_devices?: VirtualDeviceSpec[];
  plugins?: Record<string, PluginEntry>;
}

export interface PluginEntry {
  enabled: boolean;
  granted_capabilities: string[];
  config?: Record<string, unknown>;
}

export type PluginRuntimeStatus = "stopped" | "running" | "error";

export interface PluginUiPanel {
  id: string;
  title: string;
  summary: string;
}

export interface PluginStatus {
  id: string;
  name: string;
  version: string;
  description?: string;
  bundled: boolean;
  developer?: string;
  repo?: string;
  enabled: boolean;
  requested_capabilities: string[];
  granted_capabilities: string[];
  runtime_status: PluginRuntimeStatus;
  last_error?: string;
  disabled_reason?: string;
  ui_panels: PluginUiPanel[];
}

export interface CapabilityInfo {
  id: string;
  description: string;
  enforced: boolean;
}

export interface PluginDiscoveryIssue {
  path: string;
  message: string;
}

export interface RoutingIntent {
  stream_id: string;
  target_device_id?: string;
  target_device_ids?: string[];
}

export interface VolumeStateEntry {
  volume_percent: number;
  muted?: boolean;
}

export interface DynamicsStage {
  enabled: boolean;
  threshold_db: number;
  ratio_x10: number;
  attack_ms: number;
  release_ms: number;
}

export function emptyDynamicsStage(): DynamicsStage {
  return { enabled: false, threshold_db: 0, ratio_x10: 0, attack_ms: 0, release_ms: 0 };
}

/**
 * One addable/removable/reorderable unit in a device's effect chain. `id` is
 * a stable, client-generated identifier (used as the Vue `:key` for
 * reorder/remove) — generate one with `crypto.randomUUID()` when adding a
 * new stage. v1 ships exactly one kind, `eq5band`, bundling all 6 sliders
 * (5 bands + output gain trim) as one unit — see PD-025 in docs/architecture/Decisions.md.
 */
export interface Eq5BandStage {
  kind: "eq5band";
  id: string;
  eq_sub: number;
  eq_bass: number;
  eq_mid: number;
  eq_treble: number;
  eq_air: number;
  output_gain: number;
}

export type EffectStage = Eq5BandStage;

export interface EffectChainConfig {
  /** Ordered, addable/removable/reorderable. v1: 0 or 1 `eq5band` entries. */
  stages: EffectStage[];
  compressor: DynamicsStage;
  limiter: DynamicsStage;
  noise_gate: DynamicsStage;
  /** Keeps the chain configured but passes audio through unprocessed. */
  bypassed: boolean;
}

export function emptyEq5BandStage(id: string): Eq5BandStage {
  return {
    kind: "eq5band",
    id,
    eq_sub: 0,
    eq_bass: 0,
    eq_mid: 0,
    eq_treble: 0,
    eq_air: 0,
    output_gain: 0,
  };
}

export interface FxCapabilities {
  builtin_eq: boolean;
  builtin_gain: boolean;
  builtin_limiter: boolean;
  ladspa_noise_gate?: string;
}

export interface PreflightResult {
  ok: boolean;
  warnings: string[];
  blocking_reasons: string[];
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
  effect_state?: Record<string, EffectChainConfig>;
}

export interface RoutingDriftItem {
  stream_id: string;
  stream_label: string;
  live_target_id?: string;
  live_target_label?: string;
  desired_target_id?: string;
  desired_target_label?: string;
}

export interface RoutingDrift {
  profile_id: string;
  profile_name: string;
  has_drift: boolean;
  items: RoutingDriftItem[];
}

export interface ApplyResult {
  success: boolean;
  message?: string;
}

export interface VirtualDeviceResult {
  device_id: string;
  system_name: string;
  label: string;
  multi?: boolean;
}

export interface DaemonStatus {
  running: boolean;
  enabled: boolean;
  pid?: number;
  last_run?: string;
  last_error?: string;
  devices_restored?: number;
}

export interface MixSourceSpec {
  system_name: string;
  volume_percent: number;
  muted: boolean;
}

export interface VirtualDeviceSpec {
  id: string;
  slug: string;
  label: string;
  direction: DeviceDirection;
  created_at: string;
  multi?: boolean;
  mix_sources?: MixSourceSpec[];
}

export interface ProcessingNodePortSpec {
  source_system_name: string;
  /** Only meaningful for `mixer` — other kinds ignore this. */
  gain_percent: number;
  muted: boolean;
  /** Captured only for a stream peer, so a later refresh can re-resolve
   * this port onto a same-app replacement stream instance (issue #304). */
  source_stream_identity?: {
    app_name: string;
    executable?: string;
    media_name?: string;
  };
}

export type ProcessingNodeSpecKind =
  | { kind: "mixer" }
  | { kind: "fan_out"; volume_percent: number; muted: boolean }
  | { kind: "group"; volume_percent: number; muted: boolean }
  | {
      kind: "eq5band";
      eq_sub: number;
      eq_bass: number;
      eq_mid: number;
      eq_treble: number;
      eq_air: number;
      output_gain: number;
    }
  | {
      kind: "delay";
      delay_ms: number;
      feedback_percent: number;
      feedforward_percent: number;
    }
  | {
      kind: "limiter";
      ceiling_db: number;
      floor_db: number;
      symmetric: boolean;
    }
  | {
      kind: "hpf";
      freq_hz: number;
      resonance_x10: number;
    }
  | {
      kind: "reverb";
      mix_percent: number;
    }
  | {
      kind: "widener";
      width_percent: number;
    }
  | {
      kind: "pan";
      balance_percent: number;
    }
  | { kind: "stub"; stub_kind: StubEffectKind };

/** Persisted counterpart to `ProcessingNode` (PD-032) — config.yaml storage,
 * analogous to `VirtualDeviceSpec`. Wiring lives directly on the spec (like
 * `VirtualDeviceSpec.mix_sources` already does), not per-profile. */
export interface ProcessingNodeSpec {
  id: string;
  slug: string;
  label: string;
  created_at: string;
  kind: ProcessingNodeSpecKind;
  input_sources?: ProcessingNodePortSpec[];
  output_targets?: string[];
  bypassed?: boolean;
}

export type AppView =
  | "dashboard"
  | "profiles"
  | "rules"
  | "routing"
  | "mixer"
  | "sources"
  | "soundboard"
  | "effects"
  | "settings";
