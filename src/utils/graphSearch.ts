import type { Device, RuntimeGraph, Stream } from "../types/graph";

export function deviceMatchesSearch(device: Device, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  return [device.label, device.system_name].some((value) =>
    value.toLowerCase().includes(normalizedQuery),
  );
}

export function streamMatchesSearch(stream: Stream, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  return [
    stream.app_name,
    stream.executable,
    stream.media_name,
    stream.system_name,
  ].some((value) => value?.toLowerCase().includes(normalizedQuery));
}

export function applyGraphSearch(
  graph: RuntimeGraph,
  query: string,
): RuntimeGraph {
  if (!query.trim()) return graph;

  const devices = graph.devices.filter((device) =>
    deviceMatchesSearch(device, query),
  );
  const streams = graph.streams.filter((stream) =>
    streamMatchesSearch(stream, query),
  );
  const keptIds = new Set([
    ...devices.map((device) => device.id),
    ...streams.map((stream) => stream.id),
  ]);

  return {
    ...graph,
    devices,
    streams,
    links: graph.links.filter(
      (link) => keptIds.has(link.source_id) && keptIds.has(link.target_id),
    ),
  };
}
