export type PortType = "audio-in" | "audio-out";

export interface PortMeta {
  label: string;
  shortLabel: string;
  color: string;
}

export const PORT_META: Record<PortType, PortMeta> = {
  "audio-in": {
    label: "Input (receives audio)",
    shortLabel: "Input",
    color: "#7c5cff",
  },
  "audio-out": {
    label: "Output (sends audio)",
    shortLabel: "Output",
    color: "#26c3a3",
  },
};

/** Legend entries shown above the routing graph. */
export const LEGEND_ENTRIES = [
  { key: "audio-out", label: "Output (sends)", color: PORT_META["audio-out"].color },
  { key: "audio-in", label: "Input (receives)", color: PORT_META["audio-in"].color },
] as const;

const VALID_TARGETS: Record<PortType, PortType[]> = {
  "audio-out": ["audio-in"],
  "audio-in": [],
};

export function portColor(port: PortType): string {
  return PORT_META[port].color;
}

export function canConnectPorts(
  sourcePort: string | null | undefined,
  targetPort: string | null | undefined,
): boolean {
  if (!sourcePort || !targetPort) {
    return false;
  }
  const allowed = VALID_TARGETS[sourcePort as PortType];
  return allowed?.includes(targetPort as PortType) ?? false;
}

export function edgeColorForPorts(_sourcePort: PortType): string {
  return PORT_META["audio-out"].color;
}

export function edgeClassForPort(_sourcePort: PortType): string {
  return "routing-edge--audio";
}
