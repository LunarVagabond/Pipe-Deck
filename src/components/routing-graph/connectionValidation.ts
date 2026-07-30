import { canConnectPorts } from "./portTypes";
import type { Connection } from "@vue-flow/core";

/** Just the two fields this needs off a Vue Flow `Edge` — avoids importing the full (generically deep) `Edge` type here. */
export interface PendingEdgeUpdate {
  sourceHandle?: string | null;
  targetHandle?: string | null;
}

/**
 * Vue Flow reuses `isValidConnection` both for a live user drag (a bare
 * Connection, no `id`) and to re-validate every already-persisted edge on
 * each resync (which carries its own `id`). Only the former should require
 * the target to be the open trailing slot.
 *
 * During an edge-update (retarget) drag, Vue Flow builds the same bare-
 * Connection shape for the live candidate as a fresh connect drag, but the
 * unmoved end still carries its original, occupied handle id rather than an
 * empty slot — `pendingEdgeUpdate` allows that specific handle through so
 * only the genuinely moved end has to land on a real empty slot.
 */
export function resolveConnectionValidity(
  connection: Connection,
  pendingEdgeUpdate: PendingEdgeUpdate | null,
): boolean {
  const isExistingEdge = Boolean((connection as unknown as { id?: string }).id);
  if (isExistingEdge) {
    return canConnectPorts(connection.sourceHandle, connection.targetHandle, false);
  }
  const alsoFillable = pendingEdgeUpdate
    ? [pendingEdgeUpdate.sourceHandle, pendingEdgeUpdate.targetHandle]
    : [];
  return canConnectPorts(connection.sourceHandle, connection.targetHandle, true, alsoFillable);
}
