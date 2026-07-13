import type { Connection } from "@vue-flow/core";
import type { Device, RuntimeGraph, Stream } from "../../types/graph";
import {
  isMultiSink,
  sinksForStream,
  targetsForVirtualSink,
} from "../../utils/routingLayout";
import { deviceNodeId, parseGraphNodeId, streamNodeId } from "./nodeIds";
import { canConnectPorts } from "./portTypes";

export type RoutingConnectionAction =
  | { type: "stream_target"; streamId: string; targetDeviceId: string }
  | { type: "clear_stream_target"; streamId: string; previousTargetDeviceId: string }
  | { type: "device_route"; sourceDeviceId: string; targetDeviceId: string }
  | { type: "device_targets"; sourceDeviceId: string; targetDeviceIds: string[] }
  // Computed server-side against the engine's own graph (see
  // `add_mix_source`/`remove_mix_source`) rather than a client-computed full
  // list, so two mixing actions fired close together can't race and drop one.
  | { type: "mic_mix_add"; virtualMicDeviceId: string; sourceDeviceId: string }
  | { type: "mic_mix_remove"; virtualMicDeviceId: string; sourceDeviceId: string }
  | { type: "stream_mic_passthrough_add"; streamId: string; micDeviceId: string };

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
    return { error: "Incomplete connection." };
  }

  if (!canConnectPorts(connection.sourceHandle, connection.targetHandle)) {
    return { error: "Connect an output to an open input slot." };
  }

  const source = parseGraphNodeId(connection.source);
  const target = parseGraphNodeId(connection.target);
  if (!source || !target) {
    return { error: "Unknown node type." };
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

  return { error: "Streams cannot connect directly to each other." };
}

function findStream(graph: RuntimeGraph, streamId: string): Stream | undefined {
  return graph.streams.find((stream) => stream.id === streamId);
}

function findDevice(graph: RuntimeGraph, deviceId: string): Device | undefined {
  return graph.devices.find((device) => device.id === deviceId);
}

/** Soundux-style passthrough: dragging an app's playback stream onto a
 * virtual mic adds the mic as a second destination (duplicated, still
 * playing at its original output too) rather than replacing the stream's
 * target the way every other stream drag does. */
function isMicPassthroughCandidate(stream: Stream, target: Device): boolean {
  return stream.direction === "playback" && target.kind === "virtual" && target.direction === "input";
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
    return { error: "This target is not valid for the stream direction." };
  }

  if (isMicPassthroughCandidate(stream, device)) {
    if (stream.current_target === deviceId || stream.current_targets?.includes(deviceId)) {
      return { error: "This app's audio is already sent to this microphone." };
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

function isMicMixCandidate(source: Device, target: Device): boolean {
  const sourceIsPhysicalMic = source.kind === "physical" && source.direction === "input";
  const sourceIsVirtualOutput = source.kind === "virtual" && source.direction === "output";
  return (
    (sourceIsPhysicalMic || sourceIsVirtualOutput) &&
    target.kind === "virtual" &&
    target.direction !== "duplex"
  );
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

  if (isMicMixCandidate(source, target)) {
    const existingMix = target.mix_sources ?? [];
    if (existingMix.some((mixSource) => mixSource.device_id === source.id)) {
      return { error: "This microphone is already mixed into this device." };
    }
    return {
      action: {
        type: "mic_mix_add",
        virtualMicDeviceId: target.id,
        sourceDeviceId: source.id,
      },
    };
  }

  const allowed = targetsForVirtualSink(graph.devices, source);
  if (!allowed.some((entry) => entry.id === targetDeviceId)) {
    return { error: "Virtual sinks can only route to outputs or virtual inputs." };
  }

  if (source.kind !== "virtual" || source.direction !== "output") {
    return { error: "Only virtual output sinks support device-to-device routing." };
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
      return { error: "This output is already connected." };
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
    if (!stream || stream.current_target !== target.id) {
      return { error: "Connection not found." };
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
    if (!stream || stream.current_target !== source.id) {
      return { error: "Connection not found." };
    }
    return {
      action: {
        type: "clear_stream_target",
        streamId: target.id,
        previousTargetDeviceId: source.id,
      },
    };
  }

  if (source.kind !== "device" || target.kind !== "device") {
    return { error: "Nothing to disconnect." };
  }

  const device = findDevice(graph, source.id);
  const targetDevice = findDevice(graph, target.id);
  if (!device || !targetDevice) {
    return { error: "Device not found." };
  }

  if (isMicMixCandidate(device, targetDevice)) {
    const existingMix = targetDevice.mix_sources ?? [];
    if (!existingMix.some((mixSource) => mixSource.device_id === device.id)) {
      return { error: "Connection not found." };
    }
    return {
      action: {
        type: "mic_mix_remove",
        virtualMicDeviceId: targetDevice.id,
        sourceDeviceId: device.id,
      },
    };
  }

  if (device.kind !== "virtual" || device.direction !== "output") {
    return { error: "Only virtual sink routes can be disconnected from the graph." };
  }

  const existing = existingDeviceTargets(device);
  const remaining = existing.filter((id) => id !== target.id);
  if (remaining.length === existing.length) {
    return { error: "Connection not found." };
  }

  return {
    action: {
      type: "device_targets",
      sourceDeviceId: source.id,
      targetDeviceIds: remaining,
    },
  };
}

export function nodeIdsForLink(
  graph: RuntimeGraph,
  sourceId: string,
  targetId: string,
): { source: string; target: string } {
  const sourceIsStream = graph.streams.some((stream) => stream.id === sourceId);
  const targetIsStream = graph.streams.some((stream) => stream.id === targetId);
  return {
    source: sourceIsStream ? streamNodeId(sourceId) : deviceNodeId(sourceId),
    target: targetIsStream ? streamNodeId(targetId) : deviceNodeId(targetId),
  };
}
