import type { Device, Stream } from "../../types/graph";

/** Soundux-style passthrough: dragging an app's playback stream onto a
 * virtual mic adds the mic as a second destination (duplicated, still
 * playing at its original output too) rather than replacing the stream's
 * target the way every other stream drag does. */
export function isMicPassthroughCandidate(stream: Stream, target: Device): boolean {
  return stream.direction === "playback" && target.kind === "virtual" && target.direction === "input";
}

export function isMicMixCandidate(source: Device, target: Device): boolean {
  const sourceIsPhysicalMic = source.kind === "physical" && source.direction === "input";
  const sourceIsVirtualOutput = source.kind === "virtual" && source.direction === "output";
  return (
    (sourceIsPhysicalMic || sourceIsVirtualOutput) &&
    target.kind === "virtual" &&
    target.direction === "input"
  );
}

/** Whether a device is a virtual-output sink whose targets can be set/cleared
 * directly (single-target replace-route or multi-sink fan-out). Shared by
 * connect-time (`resolveDeviceToDevice`) and disconnect-time
 * (`resolveEdgeDisconnect`) so the two never drift apart on what counts as a
 * routable device-to-device relationship. */
export function isRoutableVirtualOutput(device: Device): boolean {
  return device.kind === "virtual" && device.direction === "output";
}
