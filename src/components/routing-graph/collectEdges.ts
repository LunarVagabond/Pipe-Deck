import type { RuntimeGraph } from "../../types/graph";
import {
  deviceColumn,
  deviceTargetIds,
  isMultiSink,
} from "../../utils/routingLayout";
import { graphEntityExists, handlesForLink } from "./nodePorts";
import { edgeClassForPort, edgeColorForPorts } from "./portTypes";
import { deviceNodeId, streamNodeId } from "./nodeIds";

export interface BuiltGraphEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string;
  targetHandle?: string;
  animated?: boolean;
  style?: Record<string, string>;
  class?: string;
  updatable?: boolean | "source" | "target";
  interactionWidth?: number;
  type?: string;
}

function edgeKey(source: string, target: string): string {
  return `${source}|${target}`;
}

function makeEdge(
  graph: RuntimeGraph,
  linkId: string,
  sourceId: string,
  targetId: string,
): BuiltGraphEdge | null {
  if (!graphEntityExists(graph.streams, graph.devices, sourceId)) {
    return null;
  }
  if (!graphEntityExists(graph.streams, graph.devices, targetId)) {
    return null;
  }

  const sourceIsStream = graph.streams.some((stream) => stream.id === sourceId);
  const targetIsStream = graph.streams.some((stream) => stream.id === targetId);
  const source = sourceIsStream ? streamNodeId(sourceId) : deviceNodeId(sourceId);
  const target = targetIsStream ? streamNodeId(targetId) : deviceNodeId(targetId);
  const { sourceHandle, targetHandle } = handlesForLink();

  return {
    id: linkId,
    source,
    target,
    sourceHandle,
    targetHandle,
    animated: true,
    updatable: true,
    interactionWidth: 22,
    type: "routingEdge",
    class: `routing-edge ${edgeClassForPort(sourceHandle)}`,
    style: { stroke: edgeColorForPorts(sourceHandle), strokeWidth: "2.5" },
  };
}

/** Collect deduplicated routing edges from graph links and multi-sink fan-out. */
export function collectRoutingEdges(graph: RuntimeGraph): BuiltGraphEdge[] {
  const edges = new Map<string, BuiltGraphEdge>();
  const streamSourceSeen = new Set<string>();
  const captureStreamSeen = new Set<string>();

  function addEdge(linkId: string, sourceId: string, targetId: string) {
    const streamSource = graph.streams.find(
      (stream) =>
        stream.id === sourceId &&
        stream.direction === "playback" &&
        stream.current_target,
    );
    if (streamSource) {
      if (streamSourceSeen.has(sourceId)) {
        return;
      }
      if (streamSource.current_target !== targetId) {
        return;
      }
      streamSourceSeen.add(sourceId);
    }

    const captureStream = graph.streams.find(
      (stream) =>
        stream.id === targetId &&
        stream.direction === "capture" &&
        stream.current_target,
    );
    if (captureStream) {
      if (captureStreamSeen.has(targetId)) {
        return;
      }
      if (captureStream.current_target !== sourceId) {
        return;
      }
      captureStreamSeen.add(targetId);
    }

    const edge = makeEdge(graph, linkId, sourceId, targetId);
    if (!edge) {
      return;
    }

    const key = edgeKey(edge.source, edge.target);
    if (!edges.has(key)) {
      edges.set(key, edge);
    }
  }

  for (const link of graph.links) {
    addEdge(link.id, link.source_id, link.target_id);
  }

  for (const device of graph.devices) {
    if (!isMultiSink(device) || deviceColumn(device) !== "routing") {
      continue;
    }
    for (const targetId of deviceTargetIds(device)) {
      addEdge(`route-device-${device.id}-${targetId}`, device.id, targetId);
    }
  }

  return [...edges.values()];
}
