import type { Connection } from "@vue-flow/core";
import type { Device, ProcessingNode, RuntimeGraph, Stream } from "../../types/graph";
import {
  isMultiSink,
  sinksForStream,
  streamDisplayLabel,
  targetsForVirtualSink,
} from "../../utils/routingLayout";
import { deviceNodeId, parseGraphNodeId, processingNodeNodeId, streamNodeId } from "./nodeIds";
import { canConnectPorts } from "./portTypes";
import { isMicPassthroughCandidate, isRoutableVirtualOutput } from "./routingRelationship";

export type RoutingConnectionAction =
  | { type: "stream_target"; streamId: string; targetDeviceId: string }
  | { type: "clear_stream_target"; streamId: string; previousTargetDeviceId: string }
  | { type: "device_route"; sourceDeviceId: string; targetDeviceId: string }
  | { type: "device_targets"; sourceDeviceId: string; targetDeviceIds: string[] }
  | { type: "stream_mic_passthrough_add"; streamId: string; micDeviceId: string }
  // PD-032 processing nodes: `peerId` is a device or stream id, resolved
  // server-side against the engine's own graph (`connect_processing_node_port`),
  // same "computed against live state, not a client-built full list" caution
  // as the mic-mix actions above.
  | { type: "processing_node_connect"; nodeId: string; direction: "input" | "output"; peerId: string }
  | { type: "processing_node_disconnect"; nodeId: string; direction: "input" | "output"; portIndex: number };

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

  if (source.kind === "stream" && target.kind === "device") {
    return resolveStreamToDevice(graph, source.id, target.id);
  }

  if (source.kind === "device" && target.kind === "stream") {
    return resolveStreamToDevice(graph, target.id, source.id);
  }

  if (source.kind === "device" && target.kind === "device") {
    return resolveDeviceToDevice(graph, source.id, target.id, context);
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

function existingDeviceTargets(device: Device): string[] {
  if (device.current_targets?.length) {
    return [...device.current_targets];
  }
  return device.current_target ? [device.current_target] : [];
}

function resolveDeviceToDevice(
  graph: RuntimeGraph,
  sourceDeviceId: string,
  targetDeviceId: string,
  context: ConnectionContext,
): { action: RoutingConnectionAction } | { error: string } {
  const source = findDevice(graph, sourceDeviceId);
  const target = findDevice(graph, targetDeviceId);
  if (!source || !target) {
    return { error: "Device not found." };
  }

  const allowed = targetsForVirtualSink(graph.devices, source);
  if (!allowed.some((entry) => entry.id === targetDeviceId)) {
    return {
      error: `"${source.label}" can only route to a physical output, another virtual output, or a virtual input — "${target.label}" isn't one of those.`,
    };
  }

  if (!isRoutableVirtualOutput(source)) {
    return {
      error: `"${source.label}" isn't a virtual output sink, so it can't be routed directly to another device. Drag an application stream instead.`,
    };
  }

  const existing = existingDeviceTargets(source);

  if (context.mode === "edge_update" && context.previousEdge) {
    const previousTarget = parseGraphNodeId(context.previousEdge.target);
    if (previousTarget?.kind === "device") {
      const withoutPrevious = existing.filter((id) => id !== previousTarget.id);
      if (isMultiSink(source)) {
        const next = [...withoutPrevious, targetDeviceId];
        return {
          action: {
            type: "device_targets",
            sourceDeviceId,
            targetDeviceIds: [...new Set(next)],
          },
        };
      }
    }
  }

  if (isMultiSink(source)) {
    if (existing.includes(targetDeviceId)) {
      return { error: `"${source.label}" is already routed to "${target.label}".` };
    }
    return {
      action: {
        type: "device_targets",
        sourceDeviceId,
        targetDeviceIds: [...existing, targetDeviceId],
      },
    };
  }

  return {
    action: {
      type: "device_route",
      sourceDeviceId,
      targetDeviceId,
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

  if (source.kind !== "device" || target.kind !== "device") {
    return { error: "Nothing to disconnect." };
  }

  const device = findDevice(graph, source.id);
  const targetDevice = findDevice(graph, target.id);
  if (!device || !targetDevice) {
    return { error: "Device not found." };
  }

  if (!isRoutableVirtualOutput(device)) {
    return {
      error: `"${device.label}" isn't a virtual sink route — only virtual-output connections can be dragged off to disconnect them.`,
    };
  }

  const existing = existingDeviceTargets(device);
  const remaining = existing.filter((id) => id !== target.id);
  if (remaining.length === existing.length) {
    return { error: `"${device.label}" isn't currently routed to "${targetDevice.label}" — nothing to disconnect.` };
  }

  return {
    action: {
      type: "device_targets",
      sourceDeviceId: source.id,
      targetDeviceIds: remaining,
    },
  };
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
