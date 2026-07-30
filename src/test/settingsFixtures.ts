import type { CapabilityInfo, DaemonStatus, PluginDiscoveryIssue, PluginStatus } from "../types/graph";
import type { AppInfo, UpdateCheckResult } from "../types/app";
import type { ResolvedScheme } from "../types/theme";

export function makePluginStatus(overrides: Partial<PluginStatus> = {}): PluginStatus {
  return {
    id: "plugin-1",
    name: "Sample Plugin",
    version: "1.0.0",
    bundled: false,
    developer: "Example Dev",
    repo: "https://github.com/example/plugin",
    enabled: true,
    requested_capabilities: [],
    granted_capabilities: [],
    runtime_status: "running",
    ui_panels: [],
    ...overrides,
  };
}

export function makeCapabilityInfo(overrides: Partial<CapabilityInfo> = {}): CapabilityInfo {
  return {
    id: "network",
    description: "Access the network",
    enforced: true,
    ...overrides,
  };
}

export function makeDiscoveryIssue(overrides: Partial<PluginDiscoveryIssue> = {}): PluginDiscoveryIssue {
  return {
    path: "/home/user/.config/pipe-deck/plugins/broken",
    message: "Failed to parse manifest",
    ...overrides,
  };
}

export function makeDaemonStatus(overrides: Partial<DaemonStatus> = {}): DaemonStatus {
  return {
    running: true,
    enabled: true,
    devices_restored: 2,
    ...overrides,
  };
}

export function makeAppInfo(overrides: Partial<AppInfo> = {}): AppInfo {
  return {
    buildRevision: "abc1234",
    installKind: "dev",
    installLabel: "dev build",
    ...overrides,
  };
}

export function makeUpdateResult(overrides: Partial<UpdateCheckResult> = {}): UpdateCheckResult {
  return {
    status: "current",
    currentVersion: "0.1.0",
    ...overrides,
  };
}

export function makeResolvedScheme(overrides: Partial<ResolvedScheme> = {}): ResolvedScheme {
  const colors = {
    background: "#000000",
    surface_1: "#111111",
    surface_2: "#222222",
    border: "#333333",
    text: "#ffffff",
    text_muted: "#999999",
    accent_purple: "#8844ff",
    accent_teal: "#44ffcc",
    accent_amber: "#ffaa44",
    status_success: "#44ff44",
    status_warning: "#ffcc44",
    status_danger: "#ff4444",
  };
  return {
    id: "midnight-deck",
    name: "Midnight Deck",
    kind: "dark",
    source: "builtin",
    colors,
    ...overrides,
  };
}

export function makeConfigPaths() {
  return {
    configDir: "/home/user/.config/pipe-deck",
    profilesDir: "/home/user/.config/pipe-deck/profiles",
    pluginsDir: "/home/user/.config/pipe-deck/plugins",
  };
}
