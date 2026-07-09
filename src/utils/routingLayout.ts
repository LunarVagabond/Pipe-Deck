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
  if (device.direction === "output" || device.direction === "duplex") {
    return device.kind === "virtual" ? "routing" : "outputs";
  }
  if (device.direction === "input") return "inputs";
  return null;
}

export function deviceSubtitle(device: Device): string {
  if (device.direction === "output" || device.direction === "duplex") {
    return device.kind === "virtual" ? "Virtual Sink" : "Hardware Output";
  }
  return device.kind === "virtual" ? "Virtual Source" : "Hardware Input";
}

export function linkColor(sourceId: string, _targetId: string): string {
  return accentForId(sourceId);
}
