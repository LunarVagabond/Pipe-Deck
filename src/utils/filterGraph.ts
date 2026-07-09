import type { RuntimeGraph } from "../types/graph";

export function filterRuntimeGraph(
  graph: RuntimeGraph,
  showSystemStreams: boolean,
): RuntimeGraph {
  if (showSystemStreams) {
    return graph;
  }

  const hiddenStreamIds = new Set(
    graph.streams.filter((stream) => stream.is_system).map((stream) => stream.id),
  );

  if (hiddenStreamIds.size === 0) {
    return graph;
  }

  return {
    ...graph,
    streams: graph.streams.filter((stream) => !stream.is_system),
    links: graph.links.filter(
      (link) =>
        !hiddenStreamIds.has(link.source_id) &&
        !hiddenStreamIds.has(link.target_id),
    ),
  };
}
