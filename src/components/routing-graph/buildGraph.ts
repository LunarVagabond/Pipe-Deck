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

// Bumped to v2 (was "pipe-deck-routing-layout") because `LANE_X` changed
// three times in one dev cycle (#202, #25, then the row/column-spacing
// overlap fix) without a matching bump — a node with a stable id (a
// hardware device, whose position gets saved once and honored verbatim
// forever, same as a manual drag) could still be sitting at whatever x an
// earlier revision auto-placed it at, while an ephemeral-id node (a stream,
// which never accumulates a saved position and always re-places fresh) now
// renders under the current lanes. The two can end up in the wrong visual
// order relative to each other even though neither is actually "wrong" on
// its own — e.g. a stream rendering right of an output device that was
// auto-placed under an older, narrower `LANE_X`. Bumping the key discards
// every pre-existing saved position in one shot so everything re-auto-places
// under the current lanes; future manual drags persist under the new key
// exactly as before.
const LAYOUT_KEY = "pipe-deck-routing-layout-v2";
// Three-zone layout (issue #25): audio sources on the left — input hardware
// (previously defaulted to the far right, issue #202) shares its column with
// playback streams like a browser or game, since both are "where the audio
// comes from" rather than something Pipe Deck generates. Capture streams
// stay paired immediately past that shared source column (a mic or filtered
// virtual mic feeding a capture stream is a short, forward-reading
// connection — see #202) rather than being pulled into the center zone on
// their own. Processing nodes and virtual sinks make up the center
// "internal processing" zone; outputs remain the rightmost lane.
//
// Gaps: a "paired" pair (source column/captureStream, stream/processingNode,
// processingNode/virtualSink) sits `PAIR_GAP` apart, a zone boundary
// (captureStream/stream, virtualSink/output) sits `ZONE_GAP` apart. Both
// must clear the widest real rendered card, not just look reasonable on
// paper — a plain device card renders ~210-245px wide, but an Eq5Band/Delay
// processing node renders ~260px wide. `PAIR_GAP` (300) and `ZONE_GAP` (420)
// both still clear 260px with real margin, just tighter than an earlier pass
// at this (320/500) that erred on the spacious side.
const PAIR_GAP = 300;
const ZONE_GAP = 420;
const LANE_X: Record<RoutingNodeKind, number> = {
  input: 40,
  stream: 40,
  captureStream: 40 + PAIR_GAP,
  processingNode: 40 + PAIR_GAP + ZONE_GAP,
  virtualSink: 40 + PAIR_GAP + ZONE_GAP + PAIR_GAP,
  output: 40 + PAIR_GAP + ZONE_GAP + PAIR_GAP + ZONE_GAP,
};

function loadLayout(): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    return raw ? (JSON.parse(raw) as Record<string, { x: number; y: number }>) : {};
  } catch {
    return {};
  }
}

// A plain device/stream card renders ~85-90px tall, but an Eq5Band node
// (6 band sliders + its actions row) renders ~225px tall — well over double
// the old 110px row height, so two processing nodes auto-placed into
// adjacent slots of the same lane visually overlapped by design, not just
// when dragged close together. 280 clears the tallest real card (Eq5Band)
// with margin.
const LANE_ROW_HEIGHT = 280;
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
// Keyed by the lane's actual x coordinate, not by `RoutingNodeKind` — two
// kinds can (and, since `input`/`stream` share a column, do) render at the
// same x. Keying by kind name instead of position would track two
// independent slot pools for what's visually one column, letting an
// auto-placed input node and an auto-placed stream node both land on slot 0
// and render exactly on top of each other.
function nextFreeSlot(x: number, occupiedSlots: Map<number, Set<number>>): number {
  let slots = occupiedSlots.get(x);
  if (!slots) {
    slots = new Set();
    occupiedSlots.set(x, slots);
  }
  let slot = 0;
  while (slots.has(slot)) {
    slot += 1;
  }
  slots.add(slot);
  return slot;
}

function positionFor(
  nodeId: string,
  kind: RoutingNodeKind,
  layout: Record<string, { x: number; y: number }>,
  occupiedSlots: Map<number, Set<number>>,
): { x: number; y: number } {
  const saved = layout[nodeId];
  if (saved) return saved;
  const x = LANE_X[kind];
  const slot = nextFreeSlot(x, occupiedSlots);
  const position = { x, y: LANE_Y_OFFSET + slot * LANE_ROW_HEIGHT };
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
  delay: "Delay",
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

  // Seed occupied slots from every already-saved position (dragged or previously
  // auto-placed) so a brand new node can't be handed a slot that collides with
  // one — keyed by x rather than kind, so kinds sharing a column (input/stream)
  // share one slot pool instead of two independently-numbered ones.
  const occupiedSlots = new Map<number, Set<number>>();
  for (const position of Object.values(layout)) {
    let slots = occupiedSlots.get(position.x);
    if (!slots) {
      slots = new Set();
      occupiedSlots.set(position.x, slots);
    }
    slots.add(slotIndexForY(position.y));
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
