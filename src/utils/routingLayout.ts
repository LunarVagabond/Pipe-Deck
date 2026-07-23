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

/**
 * Left-to-right position of each column in the routing graph. Used to tell
 * "forward" connections (application → routing → outputs, always left to
 * right) apart from "backward" ones (inputs feeding a capture stream or a
 * mix target, which sit in the rightmost column and so always connect back
 * toward the left). The default bezier edge assumes source-on-left/target-
 * on-right and loops wildly for backward connections, so callers use this to
 * pick a saner edge routing for them instead.
 */
const COLUMN_RANK: Record<NodeColumn, number> = {
  applications: 0,
  routing: 1,
  outputs: 2,
  inputs: 3,
};

export function columnRank(column: NodeColumn): number {
  return COLUMN_RANK[column];
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

function titleCaseFromBinary(executable: string): string {
  return executable
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** The one place stream node labels get decided.
 *
 * PipeWire's `application.name` isn't always the app itself — some apps
 * (e.g. Discord over WebRTC) set it to an internal engine name like "WEBRTC
 * VoiceEngine" rather than their own name, while others (Firefox) report it
 * correctly. `application.process.binary` (the actual executable) is more
 * reliable when present, so it takes precedence; `app_name` is the fallback
 * for streams with no executable reported. */
export function streamDisplayLabel(stream: { app_name: string; executable?: string }): string {
  if (stream.executable) {
    return titleCaseFromBinary(stream.executable);
  }
  return stream.app_name;
}

export function targetLabel(device: Device): string {
  if (device.kind === "virtual" && device.direction === "input") {
    return `${device.label} (virtual mic)`;
  }
  return device.label;
}

// Every virtual Bus supports fanning out to multiple targets today — there
// is no PipeWire-level difference between a "multi output" and a plain output
// sink, both are the same null-sink under the hood. `sink_mode` is kept on
// the model only for backward-compat deserialization of older persisted
// devices/profiles; it no longer drives any behavioral distinction here. A
// terminal Output (virtual) (#287) is excluded — it never fans out.
export function isMultiSink(device: Device): boolean {
  return (
    device.kind === "virtual" &&
    device.virtual_role === "bus" &&
    (device.direction === "output" || device.direction === "duplex")
  );
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
    if (device.kind === "virtual") {
      return device.virtual_role === "output" ? "Virtual Output" : "Bus";
    }
    return "Hardware Output";
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
  const label = streamDisplayLabel(stream);
  if (stream.media_name && stream.media_name !== label) {
    return stream.media_name;
  }
  // Surfaces the raw `application.name` (e.g. "WEBRTC VoiceEngine") whenever
  // the label above came from the executable instead, so that identifying
  // detail isn't lost even though it's no longer the primary label.
  if (stream.app_name !== label) {
    return stream.app_name;
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
    if (candidate.kind === "virtual" && candidate.direction === "input") {
      return true;
    }
    return candidate.kind === "virtual" && candidate.direction === "output";
  });
}

