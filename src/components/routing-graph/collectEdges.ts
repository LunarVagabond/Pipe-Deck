import type { RuntimeGraph } from "../../types/graph";
import {
  columnRank,
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

/** Column rank of a stream or device, for detecting backward-flowing connections. */
function entityColumnRank(graph: RuntimeGraph, entityId: string): number {
  if (graph.streams.some((stream) => stream.id === entityId)) {
    return columnRank("applications");
  }
  const device = graph.devices.find((entry) => entry.id === entityId);
  const column = device ? deviceColumn(device) : null;
  return column ? columnRank(column) : columnRank("applications");
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
  const { sourceHandle, targetHandle } = handlesForLink(
    sourceIsStream,
    targetIsStream,
    sourceId,
    targetId,
  );

  // Nodes always render their input ports on the left and output ports on the
  // right, so a connection whose source sits in a column to the right of its
  // target's (e.g. a mic in the rightmost "inputs" column feeding a capture
  // stream or a mix target further left) has to bend backward. The default
  // bezier edge handles that by bowing out vertically, which can read as an
  // edge leaving the bottom of one node and entering the top of another.
  // Route those backward connections as an orthogonal smoothstep instead.
  const isBackward = entityColumnRank(graph, sourceId) > entityColumnRank(graph, targetId);

  return {
    id: linkId,
    source,
    target,
    sourceHandle,
    targetHandle,
    animated: true,
    updatable: true,
    interactionWidth: 22,
    type: isBackward ? "smoothstep" : undefined,
    class: `routing-edge ${edgeClassForPort()}`,
    style: { stroke: edgeColorForPorts(), strokeWidth: "2.5" },
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
