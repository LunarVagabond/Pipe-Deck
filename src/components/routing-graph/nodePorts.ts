import type { Device, RuntimeGraph, Stream } from "../../types/graph";
import { deviceColumn, deviceTargetIds, isMultiSink } from "../../utils/routingLayout";
import type { PortType } from "./portTypes";

export interface RoutingGraphHandle {
  id: string;
  type: "source" | "target";
  position: "left" | "right";
  portType: PortType;
  connectedId?: string;
  empty?: boolean;
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
    for (const sourceId of device.mix_source_ids ?? []) {
      entry(device.id).in.push(sourceId);
      entry(sourceId).out.push(device.id);
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
  const unique = [...new Set(connectedIds)];
  const bound = multiCapable ? unique : unique.slice(0, 1);

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

export function handlesForDevice(
  device: Device,
  connections: DeviceConnections = EMPTY_CONNECTIONS,
): RoutingGraphHandle[] {
  const column = deviceColumn(device);
  if (!column) {
    return [];
  }

  const isVirtualInput = device.kind === "virtual" && device.direction === "input";
  const hasIn = column === "routing" || column === "outputs" || isVirtualInput;
  const hasOut = column === "routing" || column === "inputs" || isVirtualInput;

  const handles: RoutingGraphHandle[] = [];
  if (hasIn) {
    handles.push(...buildSideHandles("audio-in", connections.in, isMultiCapableSide(device, "in")));
  }
  if (hasOut) {
    handles.push(...buildSideHandles("audio-out", connections.out, isMultiCapableSide(device, "out")));
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
): boolean {
  if (streams.some((stream) => stream.id === entityId)) {
    return true;
  }
  const device = devices.find((entry) => entry.id === entityId);
  return device !== undefined && deviceColumn(device) !== null;
}
