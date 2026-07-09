export type DeviceKind = "physical" | "virtual";
export type DeviceDirection = "input" | "output" | "duplex";
export type StreamDirection = "playback" | "capture";

export interface Device {
  id: string;
  system_name: string;
  label: string;
  kind: DeviceKind;
  direction: DeviceDirection;
  volume_percent?: number;
  muted?: boolean;
}

export interface Stream {
  id: string;
  app_name: string;
  direction: StreamDirection;
  current_target?: string;
  is_system?: boolean;
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

export interface AppConfig {
  version: number;
  active_profile?: string;
  profile_index: ProfileIndexEntry[];
  preferences?: Preferences;
  devices?: Record<string, DeviceAliasEntry>;
}
