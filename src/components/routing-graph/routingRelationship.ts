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
  // A terminal Output (virtual) (#287) is a true dead end — it can't feed a
  // mic mix any more than it can fan out to another sink; only a Bus
  // (still routable onward) qualifies as a mic-mix source.
  const sourceIsVirtualBus =
    source.kind === "virtual" && source.direction === "output" && source.virtual_role === "bus";
  return (
    (sourceIsPhysicalMic || sourceIsVirtualBus) &&
    target.kind === "virtual" &&
    target.direction === "input"
  );
}

/** Whether a device is a virtual-output sink whose targets can be set/cleared
 * directly (single-target replace-route or multi-sink fan-out). Shared by
 * connect-time (`resolveDeviceToDevice`) and disconnect-time
 * (`resolveEdgeDisconnect`) so the two never drift apart on what counts as a
 * routable device-to-device relationship. A terminal Output (virtual) (#287)
 * is excluded — it's a dead end, only a Bus can route onward. */
export function isRoutableVirtualOutput(device: Device): boolean {
  return device.kind === "virtual" && device.direction === "output" && device.virtual_role === "bus";
}
