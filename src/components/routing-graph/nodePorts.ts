import type { Device, ProcessingNode, RuntimeGraph, Stream } from "../../types/graph";
import { deviceColumn, deviceTargetIds, isMultiSink } from "../../utils/routingLayout";
import type { PortType } from "./portTypes";

export interface RoutingGraphHandle {
  id: string;
  type: "source" | "target";
  position: "left" | "right";
  portType: PortType;
  connectedId?: string;
  empty?: boolean;
  /** False only for a "terminal" virtual output device's own existing
   * monitor-fanout anchor (see `handlesForDevice`) — present so its edge has
   * a real point to connect to instead of Vue Flow's node-bounding-box
   * fallback, but not draggable, so it can never be used to create a *new*
   * arbitrary forward-route (`docs/specs/UI_Spec.md`: "the frontend never
   * rendering the handle" is what keeps such a device a dead end — this
   * anchor is deliberately non-interactive, not the interactive kind that
   * rule describes). Omitted (defaults to connectable) everywhere else. */
  connectable?: boolean;
}

export interface DeviceConnections {
  in: string[];
  out: string[];
}

/**
 * For every device, the set of entity ids (stream or device) currently wired to
 * its input side and its output side. Computed once per graph so each node can
 * render one handle per live connection plus a trailing empty slot, instead of
 * funneling every connection through a single shared dot.
 */
export function computeDeviceConnections(graph: RuntimeGraph): Map<string, DeviceConnections> {
  const map = new Map<string, DeviceConnections>();

  function entry(deviceId: string): DeviceConnections {
    let existing = map.get(deviceId);
    if (!existing) {
      existing = { in: [], out: [] };
      map.set(deviceId, existing);
    }
    return existing;
  }

  for (const stream of graph.streams) {
    if (!stream.current_target) continue;
    if (stream.direction === "playback") {
      entry(stream.current_target).in.push(stream.id);
    } else {
      entry(stream.current_target).out.push(stream.id);
    }
  }

  for (const device of graph.devices) {
    for (const targetId of deviceTargetIds(device)) {
      if (targetId === device.id) continue;
      entry(device.id).out.push(targetId);
      entry(targetId).in.push(device.id);
    }
    for (const mixSource of device.mix_sources ?? []) {
      entry(device.id).in.push(mixSource.device_id);
      entry(mixSource.device_id).out.push(device.id);
    }
  }

  // A processing node's ports (PD-032) can connect to a device on either
  // side — without this, a device fed by a Fan-out/Mixer's output (or
  // feeding one) never gets a filled handle on its own side, even though
  // `collectEdges.ts` draws the edge anyway: the edge would point at a
  // handle id `handlesForDevice` never actually creates.
  for (const node of graph.processing_nodes ?? []) {
    for (const port of node.inputs ?? []) {
      if (!port.connected_id) continue;
      entry(port.connected_id).out.push(node.id);
    }
    for (const port of node.outputs ?? []) {
      if (!port.connected_id) continue;
      entry(port.connected_id).in.push(node.id);
    }
  }

  return map;
}

/** Can this side of this device genuinely carry more than one simultaneous connection? */
function isMultiCapableSide(device: Device, side: "in" | "out"): boolean {
  if (side === "in") {
    if (device.direction === "output" || device.direction === "duplex") {
      // Both physical outputs and virtual sinks can receive from many streams (and, for
      // virtual sinks, many upstream devices) at once.
      return true;
    }
    if (device.kind === "virtual" && device.direction === "input") {
      // Mic-mix target: many physical mics can feed one virtual input.
      return true;
    }
    return false;
  }

  if (device.kind === "virtual" && (device.direction === "output" || device.direction === "duplex")) {
    return isMultiSink(device);
  }
  if (device.kind === "virtual" && device.direction === "input") {
    // Many capture streams can pick the same virtual mic as their source.
    return true;
  }
  return false;
}

function buildSideHandles(
  portType: PortType,
  connectedIds: string[],
  multiCapable: boolean,
): RoutingGraphHandle[] {
  const type: "source" | "target" = portType === "audio-out" ? "source" : "target";
  const position: "left" | "right" = portType === "audio-in" ? "left" : "right";
  // Every existing connection gets a real handle regardless of `multiCapable`
  // — capping to the first one here used to silently drop a second live
  // connection a backend can genuinely report on a non-multi-capable side
  // (e.g. a virtual output device's monitor fan-out discovered via raw
  // pw-link topology; see `handlesForDevice`), leaving its edge anchored
  // nowhere real (issue #388). `multiCapable` still fully controls whether a
  // fresh *empty* slot gets appended below — this only affects how many
  // already-filled slots are shown, never whether new ones can be added.
  const unique = [...new Set(connectedIds)];
  const bound = unique;

  const filled: RoutingGraphHandle[] = bound.map((id) => ({
    id: `${portType}:${id}`,
    type,
    position,
    portType,
    connectedId: id,
  }));

  if (!multiCapable && filled.length > 0) {
    return filled;
  }

  return [...filled, { id: `${portType}:empty`, type, position, portType, empty: true }];
}

export function handlesForStream(stream: Stream): RoutingGraphHandle[] {
  if (stream.direction === "playback") {
    return [
      {
        id: "audio-out",
        type: "source",
        position: "right",
        portType: "audio-out",
        connectedId: stream.current_target,
      },
    ];
  }
  return [
    {
      id: "audio-in",
      type: "target",
      position: "left",
      portType: "audio-in",
      connectedId: stream.current_target,
    },
  ];
}

const EMPTY_CONNECTIONS: DeviceConnections = { in: [], out: [] };

/** Whether a device structurally gets an input dot and/or an output dot at
 * all — single source of truth shared by `handlesForDevice` (what actually
 * renders) and the routing-graph layout (issue #342, which column a device
 * lands in), so the two can never drift apart. */
export function deviceHandleSides(device: Device): { hasIn: boolean; hasOut: boolean } {
  const column = deviceColumn(device);
  if (!column) {
    return { hasIn: false, hasOut: false };
  }

  const isVirtualInput = device.kind === "virtual" && device.direction === "input";
  // A virtual output device is a dead end — no forward routing of any kind
  // (retired along with VirtualRole::Bus, #293), so it never gets an output
  // pin to drag from; only a dedicated Mixer/Fan-Out/EQ node routes onward.
  const isTerminalVirtualOutput = device.kind === "virtual" && device.direction === "output";
  const hasIn = column === "routing" || column === "outputs" || isVirtualInput;
  const hasOut = (column === "routing" || column === "inputs" || isVirtualInput) && !isTerminalVirtualOutput;
  return { hasIn, hasOut };
}

export function handlesForDevice(
  device: Device,
  connections: DeviceConnections = EMPTY_CONNECTIONS,
): RoutingGraphHandle[] {
  const { hasIn, hasOut } = deviceHandleSides(device);

  const handles: RoutingGraphHandle[] = [];
  if (hasIn) {
    handles.push(...buildSideHandles("audio-in", connections.in, isMultiCapableSide(device, "in")));
  }
  if (hasOut) {
    handles.push(...buildSideHandles("audio-out", connections.out, isMultiCapableSide(device, "out")));
  } else if (connections.out.length > 0) {
    // A "terminal" virtual output device (PD-033/UI_Spec.md: the frontend
    // never rendering a *connectable* output handle is what makes it a dead
    // end) still needs a real anchor for its own existing monitor fan-out to
    // current_target(s) — an is_monitor link, still generated by the live
    // backend for the ordinary "virtual sink to physical output" case —
    // or its edge falls back to Vue Flow's node-bounding-box default instead
    // of a sensible point (#388/#391). `connectable: false` keeps it
    // non-interactive: it can be landed on by an existing edge, but never
    // dragged to create a *new* arbitrary forward-route.
    handles.push(
      ...buildSideHandles("audio-out", connections.out, false).map((handle) => ({
        ...handle,
        connectable: false,
      })),
    );
  }
  return handles;
}

/**
 * A Fan-out Node has one input port and N growable output ports, each keyed
 * by the peer id currently occupying it (same `${portType}:${id}` scheme as
 * `buildSideHandles`, so `handlesForLink` needs no processing-node-specific
 * branch — it's already generic over peer id). Mixer/EQ/stub kinds reuse
 * this too: only the "how many ports, growable or fixed" answer differs per
 * kind, not the handle-building mechanics.
 */
export function handlesForProcessingNode(node: ProcessingNode): RoutingGraphHandle[] {
  const inConnected = (node.inputs ?? []).map((port) => port.connected_id).filter((id): id is string => !!id);
  const outConnected = (node.outputs ?? []).map((port) => port.connected_id).filter((id): id is string => !!id);

  // Mirrors the engine's own growability rule
  // (`CoreEngine::connect_processing_node_port`): a Mixer's inputs grow (N
  // sources summed), a Fan-out/Group's outputs grow (1 source to N
  // destinations), every other side on every kind is capped at one —
  // `buildSideHandles`'s non-multi-capable behavior already shows exactly
  // one empty slot until it's filled, then none, so a capped side never
  // grows a second (wrong) connection point once occupied.
  const inputGrowable = node.kind.kind === "mixer";
  const outputGrowable = node.kind.kind === "fan_out";

  const handles: RoutingGraphHandle[] = [];
  handles.push(...buildSideHandles("audio-in", inConnected, inputGrowable));
  // A Group node shows no output pins at all (issue #80 follow-up) — it
  // reads as a terminal output, same as a plain hardware/virtual output
  // device, which also has no output handle. Members are managed entirely
  // through the node's own member-list UI (RoutingGraphNodeGroup.vue), not
  // drag-to-connect. Revisit if per-member drag-wiring is ever wanted later.
  if (node.kind.kind !== "group") {
    handles.push(...buildSideHandles("audio-out", outConnected, outputGrowable));
  }
  return handles;
}

export function handlesForLink(
  sourceIsStream: boolean,
  targetIsStream: boolean,
  sourceId: string,
  targetId: string,
): { sourceHandle: string; targetHandle: string } {
  return {
    sourceHandle: sourceIsStream ? "audio-out" : `audio-out:${targetId}`,
    targetHandle: targetIsStream ? "audio-in" : `audio-in:${sourceId}`,
  };
}

export function graphEntityExists(
  streams: Stream[],
  devices: Device[],
  entityId: string,
  processingNodes: ProcessingNode[] = [],
): boolean {
  if (streams.some((stream) => stream.id === entityId)) {
    return true;
  }
  if (processingNodes.some((node) => node.id === entityId)) {
    return true;
  }
  const device = devices.find((entry) => entry.id === entityId);
  return device !== undefined && deviceColumn(device) !== null;
}
