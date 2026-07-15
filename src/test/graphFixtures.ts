import type { Device, Link, RuntimeGraph, Stream } from "../types/graph";

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

export function makeGraph(devices: Device[] = [], streams: Stream[] = [], links: Link[] = []): RuntimeGraph {
  return { devices, streams, links };
}
