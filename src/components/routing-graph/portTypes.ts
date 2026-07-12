export type PortType = "audio-in" | "audio-out";

export interface PortMeta {
  label: string;
  shortLabel: string;
  color: string;
}

/**
 * Both directions share one color (Blueprint-style): a valid connection always
 * links two dots of the same color. Direction is conveyed by which side of the
 * node a handle sits on, not by hue.
 */
const AUDIO_COLOR = "#3fd0c9";

export const PORT_META: Record<PortType, PortMeta> = {
  "audio-in": {
    label: "Input (receives audio)",
    shortLabel: "Input",
    color: AUDIO_COLOR,
  },
  "audio-out": {
    label: "Output (sends audio)",
    shortLabel: "Output",
    color: AUDIO_COLOR,
  },
};

/** Legend entries shown above the routing graph. */
export const LEGEND_ENTRIES = [
  { key: "audio", label: "Audio connection", color: AUDIO_COLOR },
] as const;

const VALID_TARGETS: Record<PortType, PortType[]> = {
  "audio-out": ["audio-in"],
  "audio-in": [],
};

export function portColor(_port: PortType): string {
  return AUDIO_COLOR;
}

/** Handle ids are `${PortType}` (streams) or `${PortType}:${connectedId|"empty"}` (devices). */
function parseHandleBase(handleId: string): PortType | null {
  const base = handleId.split(":")[0];
  return base === "audio-in" || base === "audio-out" ? base : null;
}

/**
 * A device-side handle only accepts a fresh connection while it's the trailing
 * empty slot; an already-occupied device handle represents one specific existing
 * connection and isn't a valid drop target for a new one. Stream handles (no
 * `:` suffix) are always reassignable, matching today's single-target behavior.
 */
export function isHandleFillable(handleId: string): boolean {
  if (!handleId.includes(":")) {
    return true;
  }
  return handleId.endsWith(":empty");
}

/**
 * `requireEmptySlot` must be false when re-validating an already-persisted edge:
 * Vue Flow calls `isValidConnection` not only while a user is dragging a fresh
 * wire, but also to re-check every existing edge each time the declarative
 * `edges` array resyncs (e.g. on any unrelated graph update). An established
 * edge's handles are its normal, permanently-occupied ones, so enforcing
 * "target must be the empty slot" there would reject every real connection on
 * every resync — which is exactly what caused edges to vanish until refresh.
 */
export function canConnectPorts(
  sourcePort: string | null | undefined,
  targetPort: string | null | undefined,
  requireEmptySlot = true,
): boolean {
  if (!sourcePort || !targetPort) {
    return false;
  }
  const sourceBase = parseHandleBase(sourcePort);
  const targetBase = parseHandleBase(targetPort);
  if (!sourceBase || !targetBase) {
    return false;
  }
  if (!VALID_TARGETS[sourceBase]?.includes(targetBase)) {
    return false;
  }
  if (!requireEmptySlot) {
    return true;
  }
  return isHandleFillable(sourcePort) && isHandleFillable(targetPort);
}

export function edgeColorForPorts(): string {
  return AUDIO_COLOR;
}

export function edgeClassForPort(): string {
  return "routing-edge--audio";
}
