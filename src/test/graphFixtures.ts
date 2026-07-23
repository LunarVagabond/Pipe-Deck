import type { Device, Link, RuntimeGraph, Stream } from "../types/graph";

export function makeDevice(overrides: Partial<Device> = {}): Device {
  const kind = overrides.kind ?? "physical";
  const direction = overrides.direction ?? "output";
  // Fixtures predate #287's Bus/terminal-Output split; default every virtual
  // output/duplex to "bus", matching the real migration default for existing
  // devices, so specs written against "today's virtual output" behavior
  // keep passing without every call site needing to opt in explicitly.
  const virtual_role =
    kind === "virtual" && (direction === "output" || direction === "duplex") ? "bus" : undefined;
  return {
    id: "dev-1",
    system_name: "physical-out-1",
    label: "Speakers",
    kind: "physical",
    direction: "output",
    volume_percent: 80,
    muted: false,
    virtual_role,
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

export function makeGraph(devices: Device[] = [], streams: Stream[] = [], links: Link[] = []): RuntimeGraph {
  return { devices, streams, links };
}
