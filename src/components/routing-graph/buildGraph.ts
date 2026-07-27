import type { Device, ProcessingNode, RuntimeGraph, Stream } from "../../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  isMultiSink,
  streamAccent,
  streamDisplayLabel,
  streamSubtitle,
} from "../../utils/routingLayout";
import { actionStatusLabel, routeWarningLevel } from "../../utils/routeExplanation";
import { computeDeviceConnections, handlesForDevice, handlesForProcessingNode, handlesForStream } from "./nodePorts";
import type { DeviceConnections, RoutingGraphHandle } from "./nodePorts";
import { collectRoutingEdges } from "./collectEdges";
import { deviceNodeId, processingNodeNodeId, streamNodeId } from "./nodeIds";
import type { GraphGroup } from "./groups";

export type { RoutingGraphHandle };

export type RoutingNodeKind = "stream" | "captureStream" | "virtualSink" | "output" | "input" | "processingNode";

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
  /** Set when this stream's routing was blocked, skipped, or couldn't find
   * its target — surfaced as a colored badge on the node itself since a
   * blocked route otherwise leaves no edge and no other on-graph trace. */
  routeWarning?: "blocked" | "unavailable";
  routeWarningTitle?: string;
  /** Set only for `nodeKind: "processingNode"` — which Mixer/Fan-out/EQ/stub
   * kind this is (PD-032), so the node body can render kind-specific
   * controls (per-input gain rows, a "Not implemented yet" badge, ...). */
  processingNodeKind?: ProcessingNode["kind"];
  /** Keeps the node wired exactly as-is but passes audio through
   * unprocessed — set only for `nodeKind: "processingNode"`. Only Eq5Band
   * currently enforces this backend-side. */
  processingNodeBypassed?: boolean;
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
// Playback streams originate the left-to-right chain (applications →
// processing → routing → outputs). Input/capture form a separate, shorter
// chain (a mic or filtered virtual mic feeding a capture stream) that used to
// sit past "output", which put input devices — the thing users instinctively
// look for on the left — furthest right of anything, and got dragged back
// constantly (issue #202). Input and captureStream now sit as a paired block
// right after processingNode, ahead of the playback routing block, so inputs
// read left-ish while capture streams still stay immediately next to the
// input device that feeds them (short, forward-reading connection preserved
// — same tight spacing the stream→processingNode pair already used).
const LANE_X: Record<RoutingNodeKind, number> = {
  stream: 40,
  processingNode: 190,
  input: 340,
  captureStream: 490,
  virtualSink: 790,
  output: 1090,
};

function loadLayout(): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    return raw ? (JSON.parse(raw) as Record<string, { x: number; y: number }>) : {};
  } catch {
    return {};
  }
}

const LANE_ROW_HEIGHT = 110;
const LANE_Y_OFFSET = 40;
// Matches the <Background> dot gap in RoutingGraph.vue — snapping to anything
// coarser (e.g. LANE_ROW_HEIGHT) makes a one-dot nudge do nothing until the
// drag crosses a much bigger threshold, which reads as "snapping doesn't
// match the grid" even though a snap is technically happening.
const GRID_SIZE = 20;

// Snaps a manually-dragged node to the same grid the dot background renders
// (issue #202) instead of persisting the arbitrary drop pixel.
function snapToGrid(x: number, y: number): { x: number; y: number } {
  return {
    x: Math.round(x / GRID_SIZE) * GRID_SIZE,
    y: Math.round(y / GRID_SIZE) * GRID_SIZE,
  };
}

export function saveNodePosition(nodeId: string, x: number, y: number) {
  const layout = loadLayout();
  layout[nodeId] = snapToGrid(x, y);
  localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
}

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

export { deviceNodeId, parseGraphNodeId, processingNodeNodeId, streamNodeId } from "./nodeIds";

function streamNodeKind(stream: Stream): RoutingGraphNodeData {
  const playback = stream.direction === "playback";
  const warning = routeWarningLevel(stream.route_explanation);
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
    routeWarning: warning ?? undefined,
    routeWarningTitle: warning ? actionStatusLabel(stream.route_explanation?.action_status) : undefined,
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

const PROCESSING_NODE_SUBTITLE: Record<ProcessingNode["kind"]["kind"], string> = {
  mixer: "Mixer",
  fan_out: "Fan-Out",
  eq5band: "5-Band EQ",
  stub: "Not implemented yet",
};

function processingNodeNodeKind(node: ProcessingNode): RoutingGraphNodeData {
  return {
    label: node.label,
    subtitle: PROCESSING_NODE_SUBTITLE[node.kind.kind],
    nodeKind: "processingNode",
    entityId: node.id,
    handles: handlesForProcessingNode(node),
    nodeClass: `processing-node processing-node--${node.kind.kind}`,
    systemName: node.system_name,
    // Rename isn't wired up for processing nodes yet — `set_device_alias`
    // writes into a generic system_name-keyed alias map that
    // `processing_node_from_spec` doesn't read from, so a rename would
    // silently appear to succeed without changing anything. Left
    // non-editable until that's built rather than shipping a broken affordance.
    editable: false,
    deletable: true,
    // Processing nodes are never volume/mute-shaped devices — no
    // `channelType`, so `RoutingGraphNode.vue` renders neither the pinned
    // volume row nor the effects-attachment body for them.
    supportsEffects: false,
    processingNodeKind: node.kind,
    processingNodeBypassed: node.bypassed,
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
  for (const node of graph.processing_nodes ?? []) liveNodeIds.add(processingNodeNodeId(node.id));
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
    processingNode: new Set(),
    virtualSink: new Set(),
    output: new Set(),
    input: new Set(),
    captureStream: new Set(),
  };
  // Seed occupied slots from every already-saved position (dragged or previously
  // auto-placed) so a brand new node can't be handed a slot that collides with one.
  for (const position of Object.values(layout)) {
    if (position.x === LANE_X.stream) occupiedSlots.stream.add(slotIndexForY(position.y));
    else if (position.x === LANE_X.processingNode) occupiedSlots.processingNode.add(slotIndexForY(position.y));
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

  const sortedProcessingNodes = [...(graph.processing_nodes ?? [])].sort((a, b) => a.id.localeCompare(b.id));
  for (const node of sortedProcessingNodes) {
    const data = processingNodeNodeKind(node);
    const id = processingNodeNodeId(node.id);
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
