import type { Device, RuntimeGraph, Stream } from "../../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  isMultiSink,
  streamAccent,
  streamDisplayLabel,
  streamSubtitle,
} from "../../utils/routingLayout";
import { computeDeviceConnections, handlesForDevice, handlesForStream } from "./nodePorts";
import type { DeviceConnections, RoutingGraphHandle } from "./nodePorts";
import { collectRoutingEdges } from "./collectEdges";
import { deviceNodeId, streamNodeId } from "./nodeIds";
import type { GraphGroup } from "./groups";

export type { RoutingGraphHandle };

export type RoutingNodeKind = "stream" | "captureStream" | "virtualSink" | "output" | "input";

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
  channelType?: "device" | "stream";
  volumePercent?: number;
  muted?: boolean;
  /** Whether this node can carry the effects list (issue #105's redesign) —
   * true for virtual devices and streams (audio sources), false for physical
   * hardware. Hardware still gets a plain volume slider via `channelType`,
   * it just isn't framed as an effect and can't have more added to it. */
  supportsEffects?: boolean;
}

export interface RoutingGraphGroupData {
  label: string;
  groupId: string;
  color?: string;
}

export interface BuiltRoutingGraphNode {
  id: string;
  type: string;
  position: { x: number; y: number };
  parentNode?: string;
  dragHandle?: string;
  style?: Record<string, string>;
  selectable?: boolean;
  data: RoutingGraphNodeData | RoutingGraphGroupData;
}

export interface BuiltRoutingGraph {
  nodes: BuiltRoutingGraphNode[];
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
// Playback streams originate the left-to-right chain (applications → routing
// → outputs), so they sit in the leftmost lane. Capture streams are the
// opposite: they're fed BY an input-lane device (a mic, or a filtered virtual
// mic), so placing them in the same leftmost lane as playback streams forced
// that connection to run backward across the entire graph — every other lane
// sat between a capture stream's node and its actual audio source. Giving
// capture streams their own lane past "input" keeps that connection short and
// forward-reading instead, without moving or removing any input-lane device.
const LANE_X: Record<RoutingNodeKind, number> = {
  stream: 40,
  virtualSink: 340,
  output: 640,
  input: 940,
  captureStream: 1240,
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

const LANE_ROW_HEIGHT = 110;
const LANE_Y_OFFSET = 40;

/**
 * Auto-placed nodes (never manually dragged) used to be assigned a lane slot
 * purely from "how many same-lane nodes have we seen so far in this pass" —
 * since that count depends on backend array ordering and which nodes currently
 * exist, an undragged node could jump to a different slot (and collide with a
 * dragged node's saved position) on almost any unrelated graph update. Instead,
 * find the first slot not already occupied by a saved position in this lane,
 * and persist it immediately so the node keeps that slot on every future build.
 */
function nextFreeSlot(kind: RoutingNodeKind, occupiedSlots: Record<RoutingNodeKind, Set<number>>): number {
  let slot = 0;
  while (occupiedSlots[kind].has(slot)) {
    slot += 1;
  }
  occupiedSlots[kind].add(slot);
  return slot;
}

function positionFor(
  nodeId: string,
  kind: RoutingNodeKind,
  layout: Record<string, { x: number; y: number }>,
  occupiedSlots: Record<RoutingNodeKind, Set<number>>,
): { x: number; y: number } {
  const saved = layout[nodeId];
  if (saved) return saved;
  const slot = nextFreeSlot(kind, occupiedSlots);
  const position = { x: LANE_X[kind], y: LANE_Y_OFFSET + slot * LANE_ROW_HEIGHT };
  layout[nodeId] = position;
  return position;
}

export { deviceNodeId, parseGraphNodeId, streamNodeId } from "./nodeIds";

function streamNodeKind(stream: Stream): RoutingGraphNodeData {
  const playback = stream.direction === "playback";
  return {
    label: streamDisplayLabel(stream),
    subtitle: streamSubtitle(stream),
    nodeKind: playback ? "stream" : "captureStream",
    entityId: stream.id,
    accent: streamAccent(stream.id),
    handles: handlesForStream(stream),
    nodeClass: playback ? "playback" : "capture",
    channelType: stream.volume_percent !== undefined && !stream.is_system ? "stream" : undefined,
    volumePercent: stream.volume_percent,
    muted: stream.muted,
    // Streams are always audio sources — always effects-capable.
    supportsEffects: true,
  };
}

function isManagedVirtualDevice(device: Device): boolean {
  return device.kind === "virtual" && device.system_name.startsWith("pipe-deck-");
}

function deviceNodeKind(
  device: Device,
  connections: DeviceConnections,
): RoutingGraphNodeData | null {
  const column = deviceColumn(device);
  if (!column) return null;

  const managed = isManagedVirtualDevice(device);
  const shared = {
    handles: handlesForDevice(device, connections),
    systemName: device.system_name,
    editable: true,
    deletable: managed,
    channelType: device.volume_percent !== undefined ? ("device" as const) : undefined,
    volumePercent: device.volume_percent,
    muted: device.muted,
    // Hardware (physical) devices keep a plain volume slider only — no
    // effects list. Virtual devices (mixer/mic/virtual outputs) are
    // effects-capable, same as streams.
    supportsEffects: device.kind !== "physical",
  };

  if (column === "routing") {
    const subtitle = isMultiSink(device)
      ? `${deviceSubtitle(device)} · drag to branch`
      : deviceSubtitle(device);
    return {
      label: device.label,
      subtitle,
      nodeKind: "virtualSink",
      entityId: device.id,
      nodeClass: "virtual-sink",
      ...shared,
    };
  }

  if (column === "outputs") {
    return {
      label: device.label,
      subtitle: deviceSubtitle(device),
      nodeKind: "output",
      entityId: device.id,
      nodeClass: "output",
      ...shared,
    };
  }

  const isVirtualInput = device.kind === "virtual" && device.direction === "input";
  return {
    label: device.label,
    subtitle: deviceSubtitle(device),
    nodeKind: "input",
    entityId: device.id,
    nodeClass: isVirtualInput ? "virtual-input" : "input",
    ...shared,
  };
}

function slotIndexForY(y: number): number {
  return Math.round((y - LANE_Y_OFFSET) / LANE_ROW_HEIGHT);
}

export function buildRoutingGraph(graph: RuntimeGraph, groups: GraphGroup[] = []): BuiltRoutingGraph {
  const layout = loadLayout();

  // Saved positions are keyed by node id and never removed when a node disappears
  // (a stream closes, a device is unplugged). Left unpruned, those stale entries
  // keep "occupying" slots forever, so a brand new node in a busy lane gets pushed
  // past them into an ever-growing y offset — landing far from the live cluster
  // instead of the nearest free gap. Drop anything that isn't part of the current
  // graph before slots are computed.
  const liveNodeIds = new Set<string>();
  for (const stream of graph.streams) liveNodeIds.add(streamNodeId(stream.id));
  for (const device of graph.devices) {
    if (deviceColumn(device)) liveNodeIds.add(deviceNodeId(device.id));
  }
  for (const group of groups) liveNodeIds.add(group.id);

  let layoutChanged = false;
  for (const id of Object.keys(layout)) {
    if (!liveNodeIds.has(id)) {
      delete layout[id];
      layoutChanged = true;
    }
  }

  const occupiedSlots: Record<RoutingNodeKind, Set<number>> = {
    stream: new Set(),
    virtualSink: new Set(),
    output: new Set(),
    input: new Set(),
    captureStream: new Set(),
  };
  // Seed occupied slots from every already-saved position (dragged or previously
  // auto-placed) so a brand new node can't be handed a slot that collides with one.
  for (const position of Object.values(layout)) {
    if (position.x === LANE_X.stream) occupiedSlots.stream.add(slotIndexForY(position.y));
    else if (position.x === LANE_X.virtualSink) occupiedSlots.virtualSink.add(slotIndexForY(position.y));
    else if (position.x === LANE_X.output) occupiedSlots.output.add(slotIndexForY(position.y));
    else if (position.x === LANE_X.input) occupiedSlots.input.add(slotIndexForY(position.y));
    else if (position.x === LANE_X.captureStream) occupiedSlots.captureStream.add(slotIndexForY(position.y));
  }

  function trackedPositionFor(id: string, kind: RoutingNodeKind): { x: number; y: number } {
    const before = layout[id];
    const position = positionFor(id, kind, layout, occupiedSlots);
    if (!before) layoutChanged = true;
    return position;
  }

  const groupByMemberId = new Map<string, GraphGroup>();
  for (const group of groups) {
    for (const memberId of group.memberIds) {
      groupByMemberId.set(memberId, group);
    }
  }

  function withGroup(
    id: string,
    absolutePosition: { x: number; y: number },
  ): { position: { x: number; y: number }; parentNode?: string } {
    const group = groupByMemberId.get(id);
    if (!group) return { position: absolutePosition };
    return {
      parentNode: group.id,
      position: {
        x: absolutePosition.x - group.position.x,
        y: absolutePosition.y - group.position.y,
      },
    };
  }

  // Group container nodes must precede their members so vue-flow can resolve parentNode on first render.
  const nodes: BuiltRoutingGraph["nodes"] = groups.map((group) => ({
    id: group.id,
    type: "groupNode",
    position: group.position,
    selectable: true,
    dragHandle: ".group-drag-handle",
    style: { width: `${group.size.width}px`, height: `${group.size.height}px` },
    data: { label: group.label, groupId: group.id, color: group.color },
  }));

  // Stable, id-based order: which nodes claim a free auto-layout slot should
  // depend only on the set of node ids present, not on backend array ordering
  // (which can vary between polls and would otherwise reshuffle un-dragged nodes).
  const sortedStreams = [...graph.streams].sort((a, b) => a.id.localeCompare(b.id));
  const sortedDevices = [...graph.devices].sort((a, b) => a.id.localeCompare(b.id));

  for (const stream of sortedStreams) {
    const data = streamNodeKind(stream);
    const id = streamNodeId(stream.id);
    nodes.push({
      id,
      type: "routingNode",
      ...withGroup(id, trackedPositionFor(id, data.nodeKind)),
      data,
    });
  }

  const deviceConnections = computeDeviceConnections(graph);

  for (const device of sortedDevices) {
    const data = deviceNodeKind(device, deviceConnections.get(device.id) ?? { in: [], out: [] });
    if (!data) continue;
    const id = deviceNodeId(device.id);
    nodes.push({
      id,
      type: "routingNode",
      ...withGroup(id, trackedPositionFor(id, data.nodeKind)),
      data,
    });
  }

  if (layoutChanged) {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  }

  const edges = collectRoutingEdges(graph);

  return { nodes, edges };
}
