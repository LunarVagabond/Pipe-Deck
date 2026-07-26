import type { Device, Stream } from "../../types/graph";

/** Soundux-style passthrough: dragging an app's playback stream onto a
 * virtual mic adds the mic as a second destination (duplicated, still
 * playing at its original output too) rather than replacing the stream's
 * target the way every other stream drag does. */
export function isMicPassthroughCandidate(stream: Stream, target: Device): boolean {
  return stream.direction === "playback" && target.kind === "virtual" && target.direction === "input";
}
