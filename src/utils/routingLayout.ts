import type { Device, Stream } from "../types/graph";

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

export type RuleTargetKind = "output" | "input";

export function isSelectableOutputTarget(device: Device): boolean {
  if (device.system_name.startsWith("pipe-deck-feed-")) {
    return false;
  }
  return device.direction === "output" || device.direction === "duplex";
}

export function isSelectableInputTarget(device: Device): boolean {
  if (device.system_name.startsWith("pipe-deck-feed-")) {
    return false;
  }
  return device.direction === "input" || device.direction === "duplex";
}

export function devicesForTargetKind(
  devices: Device[],
  kind: RuleTargetKind,
): Device[] {
  const predicate =
    kind === "output" ? isSelectableOutputTarget : isSelectableInputTarget;
  return devices.filter(predicate).sort((left, right) => {
    const leftVirtual = left.kind === "virtual" ? 0 : 1;
    const rightVirtual = right.kind === "virtual" ? 0 : 1;
    if (leftVirtual !== rightVirtual) {
      return leftVirtual - rightVirtual;
    }
    return left.label.localeCompare(right.label);
  });
}

export function inferRuleTargetKind(
  device: Device | undefined,
): RuleTargetKind {
  if (!device) {
    return "output";
  }
  return isSelectableInputTarget(device) && device.direction === "input"
    ? "input"
    : "output";
}

export function ruleTargetKindLabel(kind: RuleTargetKind): string {
  return kind === "output" ? "Output" : "Input";
}

export function targetLabel(device: Device): string {
  if (device.kind === "virtual" && device.direction === "input") {
    return `${device.label} (virtual mic)`;
  }
  return device.label;
}

export function isMultiSink(device: Device): boolean {
  return device.sink_mode === "multi";
}

export function deviceTargetIds(device: Device): string[] {
  if (device.current_targets?.length) {
    return device.current_targets;
  }
  return device.current_target ? [device.current_target] : [];
}

export function deviceSubtitle(device: Device): string {
  if (device.system_name.startsWith("pipe-deck-split-")) {
    return "Split fan-out sink";
  }
  if (device.direction === "output" || device.direction === "duplex") {
    if (device.kind === "virtual" && isMultiSink(device)) {
      return "Multi Output Sink";
    }
    return device.kind === "virtual" ? "Virtual Sink" : "Hardware Output";
  }
  return device.kind === "virtual" ? "Virtual Source" : "Hardware Input";
}

export function streamSubtitle(stream: {
  app_name: string;
  executable?: string;
  media_name?: string;
  direction: string;
  is_system?: boolean;
}): string {
  if (stream.is_system) {
    return "System stream";
  }
  if (stream.media_name && stream.media_name !== stream.app_name) {
    const suffix = stream.executable ? ` · ${stream.executable}` : "";
    return `${stream.media_name}${suffix}`;
  }
  if (stream.executable && stream.executable !== stream.app_name) {
    return stream.executable;
  }
  return stream.direction === "capture" ? "Capture stream" : "Playback stream";
}

export function linkColor(sourceId: string, _targetId: string): string {
  return accentForId(sourceId);
}

export function sinksForStream(devices: Device[], stream: Stream): Device[] {
  return devices.filter((device) => {
    if (device.system_name.startsWith("pipe-deck-feed-")) return false;
    if (stream.direction === "playback") {
      if (device.kind === "virtual" && device.direction === "output") {
        return true;
      }
      return (
        device.direction === "output" ||
        device.direction === "duplex" ||
        (device.kind === "virtual" && device.direction === "input")
      );
    }
    return device.direction === "input" || device.direction === "duplex";
  });
}

export function targetsForVirtualSink(devices: Device[], device: Device): Device[] {
  return devices.filter((candidate) => {
    if (candidate.id === device.id) return false;
    if (candidate.kind === "physical" && candidate.direction === "output") {
      return true;
    }
    return candidate.kind === "virtual" && candidate.direction === "input";
  });
}

export function isVirtualMicDevice(device: Device): boolean {
  return device.kind === "virtual" && device.direction === "input";
}

export function virtualMicFeedSinks(
  devices: Device[],
  virtualMic: Device,
): Device[] {
  return devices.filter(
    (device) =>
      device.kind === "virtual" &&
      device.direction === "output" &&
      device.current_target === virtualMic.id,
  );
}
