import type { Stream } from "../types/graph";

/** Mirrors the backend's `StreamIdentityKey`/`stream_identity_key`
 * (`core/stream_identity.rs`), used there for manual-override detection —
 * here it lets the frontend recognize "the same stream" across a PipeWire
 * node-id change (e.g. Firefox recreating its audio node when a tab's
 * playback pauses/resumes) instead of seeing a disconnected new node.
 * Deliberately excludes `direction`, matching the backend key. */
export function streamIdentityKey(stream: Stream): string {
  return `${stream.app_name}\0${stream.executable ?? ""}\0${stream.media_name ?? ""}`;
}
