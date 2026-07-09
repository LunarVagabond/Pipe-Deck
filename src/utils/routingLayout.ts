import type { Device } from "../types/graph";

export type NodeColumn = "applications" | "routing" | "outputs" | "inputs";

export interface MatrixNode {
  id: string;
  label: string;
  column: NodeColumn;
  subtitle?: string;
  accent?: string;
}

const ACCENT_PALETTE = [
  "#7c5cff",
  "#26c3a3",
  "#ffb020",
  "#4f8cff",
  "#f472b6",
  "#34d399",
];

export function accentForId(id: string): string {
  let hash = 0;
  for (const char of id) {
    hash = char.charCodeAt(0) + ((hash << 5) - hash);
  }
  return ACCENT_PALETTE[Math.abs(hash) % ACCENT_PALETTE.length];
}

export function streamAccent(streamId: string): string {
  return accentForId(streamId);
}

export function deviceColumn(device: Device): NodeColumn | null {
  if (device.system_name.startsWith("pipe-deck-feed-")) {
    return null;
  }
  if (device.direction === "output" || device.direction === "duplex") {
    return device.kind === "virtual" ? "routing" : "outputs";
  }
  if (device.direction === "input") return "inputs";
  return null;
}

export function targetLabel(device: Device): string {
  if (device.kind === "virtual" && device.direction === "input") {
    return `${device.label} (virtual mic)`;
  }
  return device.label;
}

export function deviceSubtitle(device: Device): string {
  if (device.direction === "output" || device.direction === "duplex") {
    return device.kind === "virtual" ? "Virtual Sink" : "Hardware Output";
  }
  return device.kind === "virtual" ? "Virtual Source" : "Hardware Input";
}

export function streamSubtitle(stream: {
  app_name: string;
  media_name?: string;
  direction: string;
  is_system?: boolean;
}): string {
  if (stream.is_system) {
    return "System stream";
  }
  if (stream.media_name && stream.media_name !== stream.app_name) {
    return stream.media_name;
  }
  return stream.direction === "capture" ? "Capture stream" : "Playback stream";
}

export function linkColor(sourceId: string, _targetId: string): string {
  return accentForId(sourceId);
}
