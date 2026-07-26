import type { Connection } from "@vue-flow/core";
import type { Device, ProcessingNode, RuntimeGraph, Stream } from "../../types/graph";
import {
  sinksForStream,
  streamDisplayLabel,
} from "../../utils/routingLayout";
import { deviceNodeId, parseGraphNodeId, processingNodeNodeId, streamNodeId } from "./nodeIds";
import { canConnectPorts } from "./portTypes";
import { isMicPassthroughCandidate } from "./routingRelationship";

export type RoutingConnectionAction =
  | { type: "stream_target"; streamId: string; targetDeviceId: string }
  | { type: "clear_stream_target"; streamId: string; previousTargetDeviceId: string }
  | { type: "stream_mic_passthrough_add"; streamId: string; micDeviceId: string }
  // PD-032 processing nodes: `peerId` is a device or stream id, resolved
  // server-side against the engine's own graph (`connect_processing_node_port`),
  // same "computed against live state, not a client-built full list" caution
  // as the mic-mix actions above.
  | { type: "processing_node_connect"; nodeId: string; direction: "input" | "output"; peerId: string }
  | { type: "processing_node_disconnect"; nodeId: string; direction: "input" | "output"; portIndex: number }
  // Retargeting an edge that used to touch a processing-node port — dragging
  // either end to a new peer, even one that isn't itself a processing node
  // (e.g. moving a Mixer input's stream to route straight to a device
  // instead) — must disconnect the old port before applying whatever the
  // new drop resolves to. A growable side (a Mixer's inputs, a Fan-out's
  // outputs) has no capacity check to fall back on, and even a non-growable
  // side's "already occupied" rejection doesn't help once the *new*
  // connection no longer targets that node at all — without this, the old
  // connection is silently left live (still summed into the mix, still
  // gain-controlled by a slider the graph no longer visually shows it
  // connected to) instead of being replaced.
  | {
      type: "processing_node_retarget";
      disconnect: { nodeId: string; direction: "input" | "output"; portIndex: number };
      then: Exclude<RoutingConnectionAction, { type: "processing_node_retarget" }>;
    };

export interface PreviousEdge {
  source: string;
  target: string;
  sourceHandle?: string | null;
  targetHandle?: string | null;
}

export interface ConnectionContext {
  mode: "connect" | "edge_update" | "edge_disconnect";
  previousEdge?: PreviousEdge;
}

export function resolveConnectionAction(
  graph: RuntimeGraph,
  connection: Connection,
  context: ConnectionContext = { mode: "connect" },
): { action: RoutingConnectionAction } | { error: string } {
  if (context.mode === "edge_disconnect") {
    return resolveEdgeDisconnect(graph, context.previousEdge);
  }

  if (!connection.source || !connection.target) {
    return { error: "Drag needs both a source and a target port." };
  }

  // Mirrors RoutingGraph.vue's isValidConnection: during an edge-update (retarget)
  // drag, the unmoved end of `connection` still carries its original, occupied
  // handle id rather than an empty slot, so it needs the same allowance here or a
  // real retarget that passed the pre-drop gate would be rejected at this later
  // resolution step instead.
  const alsoFillable =
    context.mode === "edge_update" && context.previousEdge
      ? [context.previousEdge.sourceHandle, context.previousEdge.targetHandle]
      : [];
  if (!canConnectPorts(connection.sourceHandle, connection.targetHandle, true, alsoFillable)) {
    return {
      error: "Connect an output port to an open input slot — this target's slot is already in use or the wrong direction.",
    };
  }

  const source = parseGraphNodeId(connection.source);
  const target = parseGraphNodeId(connection.target);
  if (!source || !target) {
    return { error: "Could not identify one end of this connection — try refreshing the routing view." };
  }

  const primary = resolvePrimaryConnectionAction(graph, source, target);
  if ("error" in primary) {
    return primary;
  }

  if (context.mode === "edge_update" && context.previousEdge) {
    const staleDisconnect = resolveStaleProcessingNodePortDisconnect(graph, context.previousEdge, primary.action);
    if (staleDisconnect && primary.action.type !== "processing_node_retarget") {
      return { action: { type: "processing_node_retarget", disconnect: staleDisconnect, then: primary.action } };
    }
  }

  return primary;
}

function resolvePrimaryConnectionAction(
  graph: RuntimeGraph,
  source: { kind: "stream" | "device" | "processingNode"; id: string },
  target: { kind: "stream" | "device" | "processingNode"; id: string },
): { action: RoutingConnectionAction } | { error: string } {
  if (source.kind === "stream" && target.kind === "device") {
    return resolveStreamToDevice(graph, source.id, target.id);
  }

  if (source.kind === "device" && target.kind === "stream") {
    return resolveStreamToDevice(graph, target.id, source.id);
  }

  if (source.kind === "device" && target.kind === "device") {
    const sourceDevice = findDevice(graph, source.id);
    const targetDevice = findDevice(graph, target.id);
    const sourceLabel = sourceDevice?.label ?? source.id;
    const targetLabel = targetDevice?.label ?? target.id;
    return {
      error: `"${sourceLabel}" can't be routed directly to "${targetLabel}" — plain devices no longer route onward to each other. Use a Mixer or Fan-Out node, or drag an application stream instead.`,
    };
  }

  if (source.kind === "processingNode" || target.kind === "processingNode") {
    return resolveProcessingNodeConnection(graph, source, target);
  }

  const sourceStream = findStream(graph, source.id);
  const targetStream = findStream(graph, target.id);
  const sourceLabel = sourceStream ? labelFor(sourceStream) : source.id;
  const targetLabel = targetStream ? labelFor(targetStream) : target.id;
  return {
    error: `"${sourceLabel}" and "${targetLabel}" are both application streams — connect a stream to a device instead.`,
  };
}

function findStream(graph: RuntimeGraph, streamId: string): Stream | undefined {
  return graph.streams.find((stream) => stream.id === streamId);
}

function findDevice(graph: RuntimeGraph, deviceId: string): Device | undefined {
  return graph.devices.find((device) => device.id === deviceId);
}

function findProcessingNode(graph: RuntimeGraph, nodeId: string): ProcessingNode | undefined {
  return (graph.processing_nodes ?? []).find((node) => node.id === nodeId);
}

function labelFor(entity: Stream | Device): string {
  return "app_name" in entity ? streamDisplayLabel(entity) : entity.label;
}

function peerLabel(graph: RuntimeGraph, peerId: string): string {
  const stream = findStream(graph, peerId);
  if (stream) return labelFor(stream);
  const device = findDevice(graph, peerId);
  if (device) return labelFor(device);
  const node = findProcessingNode(graph, peerId);
  if (node) return node.label;
  return peerId;
}

/**
 * A processing node's input accepts a stream or device; its output feeds a
 * stream-less device (or, in a later phase, another processing node).
 * `source`/`target` follow vue-flow's drag direction — a processing node as
 * `source` means its output is being dragged out to `target`; as `target`
 * means `source` is being dragged into one of its input ports.
 */
/** A single `{node, direction, peerId}` connect action for one side of a
 * drag. Shared by the node-to-node case (which needs to check and apply
 * both ends) and the node-to-device/stream case (which only ever has one). */
function resolveProcessingNodePortConnect(
  graph: RuntimeGraph,
  node: ProcessingNode,
  direction: "input" | "output",
  peerId: string,
): { action: RoutingConnectionAction } | { error: string } {
  const ports = (direction === "input" ? node.inputs : node.outputs) ?? [];
  if (ports.some((port) => port.connected_id === peerId)) {
    return { error: `"${peerLabel(graph, peerId)}" is already connected to "${node.label}".` };
  }
  return { action: { type: "processing_node_connect", nodeId: node.id, direction, peerId } };
}

/**
 * Detects a retarget drag whose OLD edge touched a processing-node port that
 * the NEW connection (`primaryAction`, already resolved against whatever the
 * drop actually landed on — a device, a stream, or another processing node)
 * no longer represents. Deliberately independent of what kind the new
 * connection is: dragging a Mixer input's stream away to route straight to
 * a device instead resolves to a plain `stream_target` action that has no
 * idea a Mixer was ever involved, so the check can't be scoped to "both ends
 * are still processing nodes" the way the old, narrower version of this fix
 * was — that gap is exactly what left a moved Mixer input still live and
 * still gain-controlled after the edge visually moved elsewhere.
 */
function resolveStaleProcessingNodePortDisconnect(
  graph: RuntimeGraph,
  previousEdge: PreviousEdge,
  primaryAction: RoutingConnectionAction,
): { nodeId: string; direction: "input" | "output"; portIndex: number } | null {
  const previousSource = parseGraphNodeId(previousEdge.source);
  const previousTarget = parseGraphNodeId(previousEdge.target);
  if (!previousSource || !previousTarget) {
    return null;
  }

  const candidates: Array<{ nodeId: string; direction: "input" | "output"; peerId: string }> = [];
  if (previousSource.kind === "processingNode") {
    candidates.push({ nodeId: previousSource.id, direction: "output", peerId: previousTarget.id });
  }
  if (previousTarget.kind === "processingNode") {
    candidates.push({ nodeId: previousTarget.id, direction: "input", peerId: previousSource.id });
  }

  for (const candidate of candidates) {
    // Nothing to do if the new drop resolved to exactly the same node/
    // direction/peer still being connected — a no-op re-drop, not a retarget.
    if (
      primaryAction.type === "processing_node_connect" &&
      primaryAction.nodeId === candidate.nodeId &&
      primaryAction.direction === candidate.direction &&
      primaryAction.peerId === candidate.peerId
    ) {
      continue;
    }

    const node = findProcessingNode(graph, candidate.nodeId);
    if (!node) {
      continue;
    }
    const ports = (candidate.direction === "input" ? node.inputs : node.outputs) ?? [];
    const oldPort = ports.find((port) => port.connected_id === candidate.peerId);
    if (oldPort) {
      return { nodeId: node.id, direction: candidate.direction, portIndex: oldPort.index };
    }
  }

  return null;
}

function resolveProcessingNodeConnection(
  graph: RuntimeGraph,
  source: { kind: "stream" | "device" | "processingNode"; id: string },
  target: { kind: "stream" | "device" | "processingNode"; id: string },
): { action: RoutingConnectionAction } | { error: string } {
  if (source.kind === "processingNode" && target.kind === "processingNode") {
    // Chaining: source's output feeds target's input. Only one side of a
    // drag can actually be applied per action, so this resolves (and
    // validates) the target's input side — `applyConnection.ts`'s single
    // `invoke()` per action already updates both ends of the link
    // server-side (a port's `connected_id` is derived from the live graph,
    // not authored independently per node).
    const sourceNode = findProcessingNode(graph, source.id);
    const targetNode = findProcessingNode(graph, target.id);
    if (!sourceNode || !targetNode) {
      return { error: "Processing node not found." };
    }
    if (sourceNode.id === targetNode.id) {
      return { error: "A processing node can't feed its own input." };
    }
    return resolveProcessingNodePortConnect(graph, targetNode, "input", sourceNode.id);
  }

  const nodeId = source.kind === "processingNode" ? source.id : target.id;
  const node = findProcessingNode(graph, nodeId);
  if (!node) {
    return { error: "Processing node not found." };
  }

  const direction: "input" | "output" = source.kind === "processingNode" ? "output" : "input";
  const peerId = source.kind === "processingNode" ? target.id : source.id;
  return resolveProcessingNodePortConnect(graph, node, direction, peerId);
}

function resolveProcessingNodePortDisconnect(
  graph: RuntimeGraph,
  node: ProcessingNode,
  direction: "input" | "output",
  peerId: string,
): { action: RoutingConnectionAction } | { error: string } {
  const ports = (direction === "input" ? node.inputs : node.outputs) ?? [];
  const port = ports.find((entry) => entry.connected_id === peerId);
  if (!port) {
    return { error: `"${node.label}" isn't currently connected to "${peerLabel(graph, peerId)}" — nothing to disconnect.` };
  }
  return { action: { type: "processing_node_disconnect", nodeId: node.id, direction, portIndex: port.index } };
}

function resolveProcessingNodeDisconnect(
  graph: RuntimeGraph,
  source: { kind: "stream" | "device" | "processingNode"; id: string },
  target: { kind: "stream" | "device" | "processingNode"; id: string },
): { action: RoutingConnectionAction } | { error: string } {
  if (source.kind === "processingNode" && target.kind === "processingNode") {
    // Same direction convention as the connect side: source's output feeds
    // target's input, so the disconnect is authored as removing the
    // target's input port.
    const sourceNode = findProcessingNode(graph, source.id);
    const targetNode = findProcessingNode(graph, target.id);
    if (!sourceNode || !targetNode) {
      return { error: "Processing node not found." };
    }
    return resolveProcessingNodePortDisconnect(graph, targetNode, "input", sourceNode.id);
  }

  const nodeId = source.kind === "processingNode" ? source.id : target.id;
  const node = findProcessingNode(graph, nodeId);
  if (!node) {
    return { error: "Processing node not found." };
  }

  const direction: "input" | "output" = source.kind === "processingNode" ? "output" : "input";
  const peerId = source.kind === "processingNode" ? target.id : source.id;
  return resolveProcessingNodePortDisconnect(graph, node, direction, peerId);
}

function resolveStreamToDevice(
  graph: RuntimeGraph,
  streamId: string,
  deviceId: string,
): { action: RoutingConnectionAction } | { error: string } {
  const stream = findStream(graph, streamId);
  const device = findDevice(graph, deviceId);
  if (!stream || !device) {
    return { error: "Stream or device not found." };
  }

  const allowed = sinksForStream(graph.devices, stream);
  if (!allowed.some((entry) => entry.id === deviceId)) {
    const directionWord = stream.direction === "playback" ? "playback output" : "capture input";
    return {
      error: `"${labelFor(stream)}" is a ${stream.direction} stream — "${device.label}" doesn't accept that direction. Pick a ${directionWord} instead.`,
    };
  }

  if (isMicPassthroughCandidate(stream, device)) {
    if (stream.current_target === deviceId) {
      return { error: `"${labelFor(stream)}" is already sending audio to "${device.label}".` };
    }
    return {
      action: {
        type: "stream_mic_passthrough_add",
        streamId,
        micDeviceId: deviceId,
      },
    };
  }

  return {
    action: {
      type: "stream_target",
      streamId,
      targetDeviceId: deviceId,
    },
  };
}


function resolveEdgeDisconnect(
  graph: RuntimeGraph,
  previousEdge?: PreviousEdge,
): { action: RoutingConnectionAction } | { error: string } {
  if (!previousEdge?.source || !previousEdge.target) {
    return { error: "Nothing to disconnect." };
  }

  const source = parseGraphNodeId(previousEdge.source);
  const target = parseGraphNodeId(previousEdge.target);
  if (!source || !target) {
    return { error: "Unknown node type." };
  }

  if (source.kind === "stream" && target.kind === "device") {
    const stream = findStream(graph, source.id);
    const device = findDevice(graph, target.id);
    if (!stream || stream.current_target !== target.id) {
      const streamLabel = stream ? labelFor(stream) : source.id;
      const deviceLabel = device?.label ?? target.id;
      return { error: `"${streamLabel}" isn't currently routed to "${deviceLabel}" — nothing to disconnect.` };
    }
    return {
      action: {
        type: "clear_stream_target",
        streamId: source.id,
        previousTargetDeviceId: target.id,
      },
    };
  }

  if (source.kind === "device" && target.kind === "stream") {
    const stream = findStream(graph, target.id);
    const device = findDevice(graph, source.id);
    if (!stream || stream.current_target !== source.id) {
      const streamLabel = stream ? labelFor(stream) : target.id;
      const deviceLabel = device?.label ?? source.id;
      return { error: `"${streamLabel}" isn't currently routed to "${deviceLabel}" — nothing to disconnect.` };
    }
    return {
      action: {
        type: "clear_stream_target",
        streamId: target.id,
        previousTargetDeviceId: source.id,
      },
    };
  }

  if (source.kind === "processingNode" || target.kind === "processingNode") {
    return resolveProcessingNodeDisconnect(graph, source, target);
  }

  return { error: "Nothing to disconnect." };
}

function graphNodeIdFor(graph: RuntimeGraph, entityId: string): string {
  if (graph.streams.some((stream) => stream.id === entityId)) {
    return streamNodeId(entityId);
  }
  if ((graph.processing_nodes ?? []).some((node) => node.id === entityId)) {
    return processingNodeNodeId(entityId);
  }
  return deviceNodeId(entityId);
}

export function nodeIdsForLink(
  graph: RuntimeGraph,
  sourceId: string,
  targetId: string,
): { source: string; target: string } {
  return {
    source: graphNodeIdFor(graph, sourceId),
    target: graphNodeIdFor(graph, targetId),
  };
}
