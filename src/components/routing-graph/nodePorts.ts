import type { Device, Stream } from "../../types/graph";
import { deviceColumn } from "../../utils/routingLayout";
import type { PortType } from "./portTypes";

export interface RoutingGraphHandle {
  id: PortType;
  type: "source" | "target";
  position: "left" | "right";
}

const AUDIO_IN: RoutingGraphHandle = { id: "audio-in", type: "target", position: "left" };
const AUDIO_OUT: RoutingGraphHandle = { id: "audio-out", type: "source", position: "right" };

export function handlesForStream(stream: Stream): RoutingGraphHandle[] {
  return stream.direction === "playback" ? [AUDIO_OUT] : [AUDIO_IN];
}

export function handlesForDevice(device: Device): RoutingGraphHandle[] {
  const column = deviceColumn(device);
  if (!column) {
    return [];
  }

  if (column === "routing") {
    return [AUDIO_IN, AUDIO_OUT];
  }
  if (column === "outputs") {
    return [AUDIO_IN];
  }

  if (device.kind === "virtual" && device.direction === "input") {
    return [AUDIO_IN, AUDIO_OUT];
  }

  return [AUDIO_OUT];
}

export function handlesForLink(): { sourceHandle: PortType; targetHandle: PortType } {
  return { sourceHandle: "audio-out", targetHandle: "audio-in" };
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
