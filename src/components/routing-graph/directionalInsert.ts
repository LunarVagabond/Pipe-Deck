import { boundsForMembers, reflowMembers } from "./groups";
import type { GroupEdge, GroupLayoutAxis, GroupMemberInput } from "./groups";
import { computeSlotPosition } from "./dropTarget";

export interface DirectionalInsertPlan {
  orderedMembers: GroupMemberInput[];
  positions: Record<string, { x: number; y: number }>;
  bounds: ReturnType<typeof boundsForMembers>;
}

/**
 * Computes where a loose node lands, and how the rest of a group's members
 * reflow, when it's directionally inserted at `edge` of a group whose
 * current members are `existingMembers`. Pure planning step — callers are
 * responsible for persisting `positions` (e.g. via `saveNodePosition`) and
 * updating the group's own `memberIds`/`layoutAxis`/`position`/`size` from
 * `bounds`.
 */
export function planDirectionalInsert(
  existingMembers: GroupMemberInput[],
  groupPosition: { x: number; y: number },
  axis: GroupLayoutAxis,
  edge: GroupEdge,
  node: { id: string; dimensions: { width: number; height: number } },
): DirectionalInsertPlan {
  const nodeRect = {
    width: node.dimensions.width || 200,
    height: node.dimensions.height || 80,
  };
  const slotPosition = computeSlotPosition(
    existingMembers,
    groupPosition,
    axis,
    edge,
    nodeRect,
  );
  const newMember: GroupMemberInput = {
    id: node.id,
    position: slotPosition,
    width: nodeRect.width,
    height: nodeRect.height,
  };
  const prepend = edge === "left" || edge === "top";
  const orderedMembers = prepend
    ? [newMember, ...existingMembers]
    : [...existingMembers, newMember];

  const { positions, bounds } = reflowMembers(axis, orderedMembers);
  return { orderedMembers, positions, bounds };
}
