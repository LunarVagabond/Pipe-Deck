import { MEMBER_GAP, nearestGroupEdge } from "./groups";
import type { GraphGroup, GraphRect, GroupEdge, GroupLayoutAxis, GroupMemberInput } from "./groups";

/** Finds which group (if any) a loose node dragged to `nodeRect` should join, and at which edge. */
export function findDropTarget(
  nodeRect: GraphRect,
  groups: GraphGroup[],
): { group: GraphGroup; axis: GroupLayoutAxis; edge: GroupEdge } | null {
  for (const group of groups) {
    const groupRect = {
      x: group.position.x,
      y: group.position.y,
      width: group.size.width,
      height: group.size.height,
    };
    const edge = nearestGroupEdge(nodeRect, groupRect);
    if (edge) {
      const axis: GroupLayoutAxis = edge === "left" || edge === "right" ? "row" : "column";
      return { group, axis, edge };
    }
  }
  return null;
}

/** Where a new member would land if inserted at `edge` of a group with the given `members`, given its current members. */
export function computeSlotPosition(
  members: GroupMemberInput[],
  groupPosition: { x: number; y: number },
  axis: GroupLayoutAxis,
  edge: GroupEdge,
  nodeRect: { width: number; height: number },
): { x: number; y: number } {
  if (members.length === 0) {
    return { x: groupPosition.x, y: groupPosition.y };
  }
  if (axis === "row") {
    const top = Math.min(...members.map((member) => member.position.y));
    if (edge === "left") {
      const minX = Math.min(...members.map((member) => member.position.x));
      return { x: minX - MEMBER_GAP - nodeRect.width, y: top };
    }
    const maxX = Math.max(...members.map((member) => member.position.x + member.width));
    return { x: maxX + MEMBER_GAP, y: top };
  }
  const left = Math.min(...members.map((member) => member.position.x));
  if (edge === "top") {
    const minY = Math.min(...members.map((member) => member.position.y));
    return { x: left, y: minY - MEMBER_GAP - nodeRect.height };
  }
  const maxY = Math.max(...members.map((member) => member.position.y + member.height));
  return { x: left, y: maxY + MEMBER_GAP };
}
