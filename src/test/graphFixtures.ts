import type { Device, Link, ProcessingNode, RuntimeGraph, Stream } from "../types/graph";

export function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    id: "dev-1",
    system_name: "physical-out-1",
    label: "Speakers",
    kind: "physical",
    direction: "output",
    volume_percent: 80,
    muted: false,
    ...overrides,
  };
}

export function makeStream(overrides: Partial<Stream> = {}): Stream {
  return {
    id: "stream-1",
    app_name: "Test App",
    direction: "playback",
    volume_percent: 60,
    muted: false,
    ...overrides,
  };
}

export function makeProcessingNode(overrides: Partial<ProcessingNode> = {}): ProcessingNode {
  return {
    id: "proc-1",
    label: "Fan-out",
    kind: { kind: "fan_out", volume_percent: 100, muted: false },
    system_name: "pipe-deck-proc-fan_out-1",
    bypassed: false,
    live: false,
    inputs: [],
    outputs: [],
    ...overrides,
  };
}

export function makeGraph(
  devices: Device[] = [],
  streams: Stream[] = [],
  links: Link[] = [],
  processingNodes: ProcessingNode[] = [],
): RuntimeGraph {
  return { devices, streams, links, processing_nodes: processingNodes };
}
