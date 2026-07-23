import type { Connection } from "@vue-flow/core";
import type { Device, RuntimeGraph, Stream } from "../../types/graph";
import {
  isMultiSink,
  sinksForStream,
  streamDisplayLabel,
  targetsForVirtualSink,
} from "../../utils/routingLayout";
import { deviceNodeId, parseGraphNodeId, streamNodeId } from "./nodeIds";
import { canConnectPorts } from "./portTypes";
import { isMicMixCandidate, isMicPassthroughCandidate, isRoutableVirtualOutput } from "./routingRelationship";

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

function labelFor(entity: Stream | Device): string {
  return "app_name" in entity ? streamDisplayLabel(entity) : entity.label;
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

  if (isMicMixCandidate(source, target)) {
    const existingMix = target.mix_sources ?? [];
    if (existingMix.some((mixSource) => mixSource.device_id === source.id)) {
      return { error: `"${source.label}" is already mixed into "${target.label}".` };
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
      return { error: `"${device.label}" isn't currently mixed into "${targetDevice.label}" — nothing to disconnect.` };
    }
    return {
      action: {
        type: "mic_mix_remove",
        virtualMicDeviceId: targetDevice.id,
        sourceDeviceId: device.id,
      },
    };
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
