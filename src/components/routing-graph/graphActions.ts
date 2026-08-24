export type ProcessingNodeType =
  | "fan_out"
  | "mixer"
  | "eq5band"
  | "delay"
  | "limiter"
  | "hpf"
  | "reverb"
  | "widener"
  | "pan"
  | "compressor";

const PROCESSING_NODE_TYPES: readonly ProcessingNodeType[] = [
  "fan_out",
  "mixer",
  "eq5band",
  "delay",
  "limiter",
  "hpf",
  "reverb",
  "widener",
  "pan",
  "compressor",
];

/** Distinguishes the "add node" context-menu types that create a `ProcessingNode` directly from those that open the new-device dialog instead. */
export function isProcessingNodeType(type: string): type is ProcessingNodeType {
  return (PROCESSING_NODE_TYPES as readonly string[]).includes(type);
}

const DEFAULT_LABELS: Record<ProcessingNodeType, string> = {
  fan_out: "Fan-Out",
  mixer: "Mixer",
  eq5band: "5-Band EQ",
  delay: "Delay",
  limiter: "Limiter",
  hpf: "High-Pass Filter",
  reverb: "Reverb",
  widener: "Stereo Widener",
  pan: "Balance/Pan",
  compressor: "Compressor",
};

export function defaultLabelForProcessingNodeType(
  type: ProcessingNodeType,
): string {
  return DEFAULT_LABELS[type];
}

const PROCESSING_NODE_SYSTEM_NAME_PREFIX = "pipe-deck-proc-";

/** `ProcessingNode`s and virtual devices share the context-menu delete action; this tells them apart by system_name. */
export function isProcessingNodeSystemName(systemName: string): boolean {
  return systemName.startsWith(PROCESSING_NODE_SYSTEM_NAME_PREFIX);
}
