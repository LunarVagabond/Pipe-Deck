import type { Device, RuntimeGraph, Stream } from "../../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  isMultiSink,
  streamAccent,
  streamSubtitle,
} from "../../utils/routingLayout";
import { handlesForDevice, handlesForStream } from "./nodePorts";
import type { RoutingGraphHandle } from "./nodePorts";
import { collectRoutingEdges } from "./collectEdges";
import { deviceNodeId, streamNodeId } from "./nodeIds";

export type { RoutingGraphHandle };

export type RoutingNodeKind = "stream" | "virtualSink" | "output" | "input";

export interface RoutingGraphNodeData {
  label: string;
  subtitle: string;
  nodeKind: RoutingNodeKind;
  entityId: string;
  accent?: string;
  handles: RoutingGraphHandle[];
  nodeClass: string;
  systemName?: string;
  editable?: boolean;
  deletable?: boolean;
}

export interface BuiltRoutingGraph {
  nodes: Array<{
    id: string;
    type: string;
    position: { x: number; y: number };
    data: RoutingGraphNodeData;
  }>;
  edges: Array<{
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
  }>;
}

const LAYOUT_KEY = "pipe-deck-routing-layout";
const LANE_X: Record<RoutingNodeKind, number> = {
  stream: 40,
  virtualSink: 340,
  output: 640,
  input: 940,
};

function loadLayout(): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    return raw ? (JSON.parse(raw) as Record<string, { x: number; y: number }>) : {};
  } catch {
    return {};
  }
}

export function saveNodePosition(nodeId: string, x: number, y: number) {
  const layout = loadLayout();
  layout[nodeId] = { x, y };
  localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
}

function positionFor(
  nodeId: string,
  kind: RoutingNodeKind,
  laneCounts: Record<RoutingNodeKind, number>,
): { x: number; y: number } {
  const saved = loadLayout()[nodeId];
  if (saved) return saved;
  const laneIndex = laneCounts[kind];
  laneCounts[kind] += 1;
  return {
    x: LANE_X[kind],
    y: 40 + laneIndex * 110,
  };
}

export { deviceNodeId, parseGraphNodeId, streamNodeId } from "./nodeIds";

function streamNodeKind(stream: Stream): RoutingGraphNodeData {
  const playback = stream.direction === "playback";
  return {
    label: stream.app_name,
    subtitle: streamSubtitle(stream),
    nodeKind: "stream",
    entityId: stream.id,
    accent: streamAccent(stream.id),
    handles: handlesForStream(stream),
    nodeClass: playback ? "playback" : "capture",
  };
}

function isManagedVirtualDevice(device: Device): boolean {
  return device.kind === "virtual" && device.system_name.startsWith("pipe-deck-");
}

function deviceNodeKind(device: Device): RoutingGraphNodeData | null {
  const column = deviceColumn(device);
  if (!column) return null;

  const managed = isManagedVirtualDevice(device);

  if (column === "routing") {
    const subtitle = isMultiSink(device)
      ? `${deviceSubtitle(device)} · drag to branch`
      : deviceSubtitle(device);
    return {
      label: device.label,
      subtitle,
      nodeKind: "virtualSink",
      entityId: device.id,
      handles: handlesForDevice(device),
      nodeClass: "virtual-sink",
      systemName: device.system_name,
      editable: true,
      deletable: managed,
    };
  }

  if (column === "outputs") {
    return {
      label: device.label,
      subtitle: deviceSubtitle(device),
      nodeKind: "output",
      entityId: device.id,
      handles: handlesForDevice(device),
      nodeClass: "output",
      systemName: device.system_name,
      editable: true,
      deletable: managed,
    };
  }

  const isVirtualInput = device.kind === "virtual" && device.direction === "input";
  return {
    label: device.label,
    subtitle: deviceSubtitle(device),
    nodeKind: "input",
    entityId: device.id,
    handles: handlesForDevice(device),
    nodeClass: isVirtualInput ? "virtual-input" : "input",
    systemName: device.system_name,
    editable: true,
    deletable: managed,
  };
}

export function buildRoutingGraph(graph: RuntimeGraph): BuiltRoutingGraph {
  const laneCounts: Record<RoutingNodeKind, number> = {
    stream: 0,
    virtualSink: 0,
    output: 0,
    input: 0,
  };

  const nodes: BuiltRoutingGraph["nodes"] = [];

  for (const stream of graph.streams) {
    const data = streamNodeKind(stream);
    const id = streamNodeId(stream.id);
    nodes.push({
      id,
      type: "routingNode",
      position: positionFor(id, "stream", laneCounts),
      data,
    });
  }

  for (const device of graph.devices) {
    const data = deviceNodeKind(device);
    if (!data) continue;
    const id = deviceNodeId(device.id);
    nodes.push({
      id,
      type: "routingNode",
      position: positionFor(id, data.nodeKind, laneCounts),
      data,
    });
  }

  const edges = collectRoutingEdges(graph);

  return { nodes, edges };
}
