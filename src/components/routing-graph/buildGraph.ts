import type {
  Device,
  ProcessingNode,
  RuntimeGraph,
  Stream,
} from "../../types/graph";
import {
  deviceColumn,
  deviceSubtitle,
  isBluetoothDevice,
  isMultiSink,
  streamAccent,
  streamDisplayLabel,
  streamSubtitle,
} from "../../utils/routingLayout";
import {
  actionStatusLabel,
  routeWarningLevel,
} from "../../utils/routeExplanation";
import { formatMismatch } from "../../utils/formatMismatch";
import { streamIdentityKey } from "../../utils/streamIdentity";
import {
  computeDeviceConnections,
  deviceHandleSides,
  handlesForDevice,
  handlesForProcessingNode,
  handlesForStream,
} from "./nodePorts";
import type { DeviceConnections, RoutingGraphHandle } from "./nodePorts";
import { collectRoutingEdges } from "./collectEdges";
import { deviceNodeId, processingNodeNodeId, streamNodeId } from "./nodeIds";
import type { GraphGroup } from "./groups";

export type { RoutingGraphHandle };

export type RoutingNodeKind =
  | "stream"
  | "captureStream"
  | "virtualSink"
  | "output"
  | "input"
  | "processingNode";

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
  /** Set when this stream's negotiated sample-rate/channel-count differs
   * from its current target device's — informational only (issue #156):
   * PipeWire already resamples/remixes transparently at the link layer, so
   * this isn't an error state, just awareness of a conversion happening. */
  formatMismatch?: boolean;
  formatMismatchTitle?: string;
  /** Set only for stream/captureStream kinds — lets a consumer (the
   * fitView-on-new-node logic in RoutingGraph.vue) recognize "the same
   * stream, new PipeWire node id" (e.g. Firefox recreating its audio node
   * on tab pause/resume) instead of treating it as a genuinely new node. */
  streamIdentityKey?: string;
  /** Set only for `nodeKind: "processingNode"` — which Mixer/Fan-out/EQ/stub
   * kind this is (PD-032), so the node body can render kind-specific
   * controls (per-input gain rows, a "Not implemented yet" badge, ...). */
  processingNodeKind?: ProcessingNode["kind"];
  /** Keeps the node wired exactly as-is but passes audio through
   * unprocessed — set only for `nodeKind: "processingNode"`. Only Eq5Band
   * currently enforces this backend-side. */
  processingNodeBypassed?: boolean;
  /** Overrides the `NodeTypeIcon` kind without touching `nodeClass` (issue
   * #226) — `nodeClass` also drives the node's border-color CSS class
   * (`.output`/`.input`/...), which a Bluetooth device should keep, so the
   * icon-only "bluetooth" distinction is carried separately instead of
   * replacing `nodeClass` outright. */
  iconOverride?: string;
  /** Set only for a `group`-kind processing node (issue #80, PD-035
   * revision) — the real device each occupied output port currently points
   * at, resolved to id+label here (the one place with full graph access) so
   * `RoutingGraphNodeGroup.vue` can render an inline member list without
   * needing the whole graph threaded down to it. `portIndex` is what
   * `disconnect_processing_node_port` needs to remove that one member. */
  groupMembers?: { id: string; label: string; portIndex: number }[];
  /** Set only for a `group`-kind processing node — every terminal output
   * device (hardware or plain virtual, same eligibility as the initial
   * "Group Selected Outputs" gesture) not already a member, for the "+ add
   * member" picker in `RoutingGraphNodeGroup.vue`. */
  groupAvailableDevices?: { id: string; label: string }[];
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
//
// Bumped again to v3 (issue #342) — the kind-based lane scheme (`LANE_X`)
// was replaced outright by a connectivity-based three-column one
// (`columnXFor`), so every existing saved position needs to re-auto-place
// under the new columns for the same reason as the v1→v2 bump above.
const LAYOUT_KEY = "pipe-deck-routing-layout-v3";

// Bridges a stream's saved layout position across a PipeWire node-id change
// for what's conceptually the same stream (e.g. Firefox recreating its
// audio node when a tab's playback pauses/resumes) — keyed by identity
// (app_name/executable/media_name), not node id, since the id itself is
// exactly what's churning. Session-only (module-level, resets on reload);
// this only needs to survive within a session's polling ticks, not across
// restarts like the position layout itself.
let previousStreamIdentityByNodeId = new Map<string, string>();
// Three columns by connectivity shape, not by node kind (issue #342 —
// superseding the original kind-based three-zone layout from issues
// #25/#202, which put input hardware in the source column but capture
// streams in their own lane between the source column and the center zone;
// a virtual mic-mix device — genuinely a pass-through, fed by a physical mic
// and feeding a capture stream — got stuck in the leftmost/source column
// purely because its `direction` was `input`, producing long crossing edges
// whenever a Discord/Slack call routed through one):
//   - Output only (no input) → left: playback streams, physical input
//     hardware.
//   - Input and output → center: virtual input devices (mic-mix buses),
//     every processing-node kind (Mixer/Fan-out/EQ/etc, PD-032).
//   - Input only (no output) → right: capture streams, physical output
//     hardware, virtual sink/output devices (structurally always
//     receive-only since #293 retired forward-routing on plain virtual
//     output devices — see `deviceHandleSides` in `nodePorts.ts`, the same
//     hasIn/hasOut logic used here so a node's column can never disagree
//     with whether it actually draws an input/output dot).
//
// `ZONE_GAP` must clear the widest real rendered card, not just look
// reasonable on paper — a plain device card renders ~210-245px wide, but an
// Eq5Band/Delay processing node renders ~260px wide; 420 clears that with
// real margin.
const ZONE_GAP = 420;
const COL_SOURCE = 40;
const COL_PASSTHROUGH = COL_SOURCE + ZONE_GAP;
const COL_TERMINAL = COL_PASSTHROUGH + ZONE_GAP;

function columnXFor(hasIn: boolean, hasOut: boolean): number {
  if (hasOut && !hasIn) return COL_SOURCE;
  if (hasIn && !hasOut) return COL_TERMINAL;
  return COL_PASSTHROUGH; // both, or the (unreachable in practice) neither case
}

function loadLayout(): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    return raw
      ? (JSON.parse(raw) as Record<string, { x: number; y: number }>)
      : {};
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
//
// Applying that 280px worst-case height to *every* row regardless of what's
// actually in the graph wastes 900px+ of vertical space on an all-plain-card
// graph (the common case — most demo scenarios and plenty of real setups
// have zero processing nodes), forcing fitView's zoom down toward its floor
// and making labels illegible (#390). Rather than a fully dynamic per-node
// row height — a much larger change to this slot-index-based layout, with
// real regression risk to drag-position persistence and slot migration —
// this uses the tall spacing only when the graph actually contains a
// processing node anywhere, and the original tighter spacing otherwise.
const LANE_ROW_HEIGHT_TALL = 280;
const LANE_ROW_HEIGHT_PLAIN = 110;
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
function nextFreeSlot(
  x: number,
  occupiedSlots: Map<number, Set<number>>,
): number {
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
  x: number,
  layout: Record<string, { x: number; y: number }>,
  occupiedSlots: Map<number, Set<number>>,
  rowHeight: number,
): { x: number; y: number } {
  const saved = layout[nodeId];
  if (saved) return saved;
  const slot = nextFreeSlot(x, occupiedSlots);
  const position = { x, y: LANE_Y_OFFSET + slot * rowHeight };
  layout[nodeId] = position;
  return position;
}

export {
  deviceNodeId,
  parseGraphNodeId,
  processingNodeNodeId,
  streamNodeId,
} from "./nodeIds";

function streamNodeKind(
  stream: Stream,
  devices: Device[],
): RoutingGraphNodeData {
  const playback = stream.direction === "playback";
  const warning = routeWarningLevel(stream.route_explanation);
  const targetDevice = devices.find(
    (device) => device.id === stream.current_target,
  );
  const format = targetDevice
    ? formatMismatch(stream, targetDevice)
    : { mismatch: false };
  return {
    label: streamDisplayLabel(stream),
    subtitle: streamSubtitle(stream),
    nodeKind: playback ? "stream" : "captureStream",
    entityId: stream.id,
    accent: streamAccent(stream.id),
    handles: handlesForStream(stream),
    nodeClass: playback ? "playback" : "capture",
    channelType:
      stream.volume_percent !== undefined && !stream.is_system
        ? "stream"
        : undefined,
    volumePercent: stream.volume_percent,
    muted: stream.muted,
    // Streams are always audio sources — always effects-capable.
    supportsEffects: true,
    routeWarning: warning ?? undefined,
    routeWarningTitle: warning
      ? actionStatusLabel(stream.route_explanation?.action_status)
      : undefined,
    formatMismatch: format.mismatch || undefined,
    formatMismatchTitle: format.title,
    streamIdentityKey: streamIdentityKey(stream),
  };
}

function isManagedVirtualDevice(device: Device): boolean {
  return (
    device.kind === "virtual" && device.system_name.startsWith("pipe-deck-")
  );
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
    channelType:
      device.volume_percent !== undefined ? ("device" as const) : undefined,
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
      iconOverride: isBluetoothDevice(device) ? "bluetooth" : undefined,
      ...shared,
    };
  }

  const isVirtualInput =
    device.kind === "virtual" && device.direction === "input";
  return {
    label: device.label,
    subtitle: deviceSubtitle(device),
    nodeKind: "input",
    entityId: device.id,
    nodeClass: isVirtualInput ? "virtual-input" : "input",
    iconOverride: isBluetoothDevice(device) ? "bluetooth" : undefined,
    ...shared,
  };
}

const PROCESSING_NODE_SUBTITLE: Record<ProcessingNode["kind"]["kind"], string> =
  {
    mixer: "Mixer",
    fan_out: "Fan-Out",
    group: "Group",
    eq5band: "5-Band EQ",
    delay: "Delay",
    limiter: "Limiter",
    hpf: "High-Pass Filter",
    reverb: "Reverb",
    widener: "Stereo Widener",
    pan: "Balance/Pan",
    compressor: "Compressor",
    stub: "Not implemented yet",
  };

function processingNodeNodeKind(
  node: ProcessingNode,
  graph: RuntimeGraph,
): RoutingGraphNodeData {
  const memberIds = new Set(
    (node.outputs ?? []).map((port) => port.connected_id).filter(Boolean),
  );
  const groupMembers =
    node.kind.kind === "group"
      ? (node.outputs ?? [])
          .filter((port): port is typeof port & { connected_id: string } =>
            Boolean(port.connected_id),
          )
          .map((port) => ({
            id: port.connected_id,
            label:
              graph.devices.find((device) => device.id === port.connected_id)
                ?.label ?? port.connected_id,
            portIndex: port.index,
          }))
      : undefined;
  const groupAvailableDevices =
    node.kind.kind === "group"
      ? graph.devices
          .filter(
            (device) =>
              deviceColumn(device) === "outputs" && !memberIds.has(device.id),
          )
          .map((device) => ({ id: device.id, label: device.label }))
      : undefined;

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
    groupMembers,
    groupAvailableDevices,
  };
}

function slotIndexForY(y: number, rowHeight: number): number {
  return Math.round((y - LANE_Y_OFFSET) / rowHeight);
}

export function buildRoutingGraph(
  graph: RuntimeGraph,
  groups: GraphGroup[] = [],
): BuiltRoutingGraph {
  const layout = loadLayout();
  const rowHeight =
    (graph.processing_nodes ?? []).length > 0
      ? LANE_ROW_HEIGHT_TALL
      : LANE_ROW_HEIGHT_PLAIN;

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
  for (const node of graph.processing_nodes ?? [])
    liveNodeIds.add(processingNodeNodeId(node.id));
  for (const group of groups) liveNodeIds.add(group.id);

  // Count current streams per identity so a departing node's saved position
  // is only migrated onto a replacement when the match is unambiguous (e.g.
  // two simultaneous same-app streams with no distinguishing media_name
  // would make it a guess which one "continues" which old node id — skip
  // migration entirely rather than risk stealing the wrong position).
  const currentIdentityCounts = new Map<string, number>();
  const currentIdentityToNodeId = new Map<string, string>();
  const currentNodeIdToIdentity = new Map<string, string>();
  for (const stream of graph.streams) {
    const identity = streamIdentityKey(stream);
    const id = streamNodeId(stream.id);
    currentIdentityCounts.set(
      identity,
      (currentIdentityCounts.get(identity) ?? 0) + 1,
    );
    currentIdentityToNodeId.set(identity, id);
    currentNodeIdToIdentity.set(id, identity);
  }

  // Migrate a departing stream's saved position onto its replacement's new
  // node id when there's an unambiguous identity match, before the prune
  // step below deletes the departing id's entry outright — otherwise a
  // stream whose PipeWire node was recreated (e.g. Firefox recreating its
  // audio node when a tab's playback pauses/resumes) loses its spot and
  // gets auto-placed into a fresh slot, which reads as the node jumping.
  let layoutChanged = false;
  for (const [staleId, identity] of previousStreamIdentityByNodeId) {
    if (liveNodeIds.has(staleId)) continue;
    if ((currentIdentityCounts.get(identity) ?? 0) !== 1) continue;
    const newId = currentIdentityToNodeId.get(identity);
    if (!newId || newId === staleId || layout[newId] || !layout[staleId])
      continue;
    layout[newId] = layout[staleId];
    layoutChanged = true;
  }
  previousStreamIdentityByNodeId = currentNodeIdToIdentity;

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
    slots.add(slotIndexForY(position.y, rowHeight));
  }

  function trackedPositionFor(id: string, x: number): { x: number; y: number } {
    const before = layout[id];
    const position = positionFor(id, x, layout, occupiedSlots, rowHeight);
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
  const sortedStreams = [...graph.streams].sort((a, b) =>
    a.id.localeCompare(b.id),
  );
  const sortedDevices = [...graph.devices].sort((a, b) =>
    a.id.localeCompare(b.id),
  );

  for (const stream of sortedStreams) {
    const data = streamNodeKind(stream, graph.devices);
    const id = streamNodeId(stream.id);
    const x = columnXFor(
      stream.direction === "capture",
      stream.direction === "playback",
    );
    nodes.push({
      id,
      type: "routingNode",
      ...withGroup(id, trackedPositionFor(id, x)),
      data,
    });
  }

  const deviceConnections = computeDeviceConnections(graph);

  for (const device of sortedDevices) {
    const data = deviceNodeKind(
      device,
      deviceConnections.get(device.id) ?? { in: [], out: [] },
    );
    if (!data) continue;
    const id = deviceNodeId(device.id);
    const { hasIn, hasOut } = deviceHandleSides(device);
    nodes.push({
      id,
      type: "routingNode",
      ...withGroup(id, trackedPositionFor(id, columnXFor(hasIn, hasOut))),
      data,
    });
  }

  const sortedProcessingNodes = [...(graph.processing_nodes ?? [])].sort(
    (a, b) => a.id.localeCompare(b.id),
  );
  for (const node of sortedProcessingNodes) {
    const data = processingNodeNodeKind(node, graph);
    const id = processingNodeNodeId(node.id);
    nodes.push({
      id,
      type: "routingNode",
      ...withGroup(id, trackedPositionFor(id, COL_PASSTHROUGH)),
      data,
    });
  }

  if (layoutChanged) {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  }

  const edges = collectRoutingEdges(graph);

  return { nodes, edges };
}
