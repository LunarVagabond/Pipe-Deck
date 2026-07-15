export interface GraphRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type GroupLayoutAxis = "row" | "column";

export interface GraphGroup {
  id: string;
  label: string;
  position: { x: number; y: number };
  size: { width: number; height: number };
  memberIds: string[];
  /** User-picked accent color for the group panel (border/header), not the member nodes. */
  color?: string;
  /** Set once a directional (left/right/top/bottom) insert has aligned members into a row/column. Free-form (multi-select-created) groups leave this unset. */
  layoutAxis?: GroupLayoutAxis;
}

export const GROUP_COLORS = [
  "#7c5cff",
  "#ff6b6b",
  "#ffb020",
  "#22c55e",
  "#38bdf8",
  "#f472b6",
] as const;

const GROUPS_KEY = "pipe-deck-routing-groups";
const GROUP_PADDING = 32;
/** Space reserved above member nodes for the group's title bar/drag handle. */
export const GROUP_HEADER_HEIGHT = 36;

export function loadGroups(): GraphGroup[] {
  try {
    const raw = localStorage.getItem(GROUPS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as GraphGroup[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function saveGroups(groups: GraphGroup[]) {
  localStorage.setItem(GROUPS_KEY, JSON.stringify(groups));
}

export interface GroupMemberInput {
  id: string;
  position: { x: number; y: number };
  width: number;
  height: number;
}

export const MEMBER_GAP = 24;

/** Bounding box (with padding + header allowance) that fits every given member. */
export function boundsForMembers(members: GroupMemberInput[]): {
  position: { x: number; y: number };
  size: { width: number; height: number };
} {
  const minX = Math.min(...members.map((member) => member.position.x));
  const minY = Math.min(...members.map((member) => member.position.y));
  const maxX = Math.max(...members.map((member) => member.position.x + member.width));
  const maxY = Math.max(...members.map((member) => member.position.y + member.height));

  return {
    position: { x: minX - GROUP_PADDING, y: minY - GROUP_PADDING - GROUP_HEADER_HEIGHT },
    size: {
      width: maxX - minX + GROUP_PADDING * 2,
      height: maxY - minY + GROUP_PADDING * 2 + GROUP_HEADER_HEIGHT,
    },
  };
}

export function createGroup(label: string, members: GroupMemberInput[]): GraphGroup {
  const { position, size } = boundsForMembers(members);

  return {
    id: `group-${Date.now()}-${Math.round(Math.random() * 1e5)}`,
    label,
    position,
    size,
    memberIds: members.map((member) => member.id),
  };
}

/** How much of `inner`'s area overlaps `outer`, from 0 to 1. */
export function containmentRatio(inner: GraphRect, outer: GraphRect): number {
  const overlapX =
    Math.max(0, Math.min(inner.x + inner.width, outer.x + outer.width) - Math.max(inner.x, outer.x));
  const overlapY =
    Math.max(0, Math.min(inner.y + inner.height, outer.y + outer.height) - Math.max(inner.y, outer.y));
  const innerArea = inner.width * inner.height;
  return innerArea > 0 ? (overlapX * overlapY) / innerArea : 0;
}

/**
 * Lays `orderedMembers` out sequentially along `axis` with a fixed gap,
 * aligned on the cross-axis (shared top for a row, shared left for a
 * column) — used both to insert a new member at a directional slot and to
 * close the gap when one leaves an already-aligned group.
 */
export function reflowMembers(
  axis: GroupLayoutAxis,
  orderedMembers: GroupMemberInput[],
): { positions: Record<string, { x: number; y: number }>; bounds: ReturnType<typeof boundsForMembers> } {
  const positions: Record<string, { x: number; y: number }> = {};
  if (orderedMembers.length === 0) {
    return {
      positions,
      bounds: { position: { x: 0, y: 0 }, size: { width: 0, height: 0 } },
    };
  }

  if (axis === "row") {
    const top = Math.min(...orderedMembers.map((member) => member.position.y));
    let cursorX = Math.min(...orderedMembers.map((member) => member.position.x));
    for (const member of orderedMembers) {
      positions[member.id] = { x: cursorX, y: top };
      cursorX += member.width + MEMBER_GAP;
    }
  } else {
    const left = Math.min(...orderedMembers.map((member) => member.position.x));
    let cursorY = Math.min(...orderedMembers.map((member) => member.position.y));
    for (const member of orderedMembers) {
      positions[member.id] = { x: left, y: cursorY };
      cursorY += member.height + MEMBER_GAP;
    }
  }

  const laidOut = orderedMembers.map((member) => ({ ...member, position: positions[member.id] }));
  return { positions, bounds: boundsForMembers(laidOut) };
}

export type GroupEdge = "left" | "right" | "top" | "bottom";

const HOVER_MARGIN = 70;

/**
 * Which edge of `group` a dragged node (`nodeRect`) is closest to, if it's
 * within `HOVER_MARGIN` of the group's (possibly expanded) bounds — used to
 * drive both the live drop-slot preview and the actual insert-on-drop.
 * Returns null when the node is too far from the group to be considered a
 * directional-insert candidate.
 */
export function nearestGroupEdge(nodeRect: GraphRect, group: GraphRect): GroupEdge | null {
  const expanded = {
    x: group.x - HOVER_MARGIN,
    y: group.y - HOVER_MARGIN,
    width: group.width + HOVER_MARGIN * 2,
    height: group.height + HOVER_MARGIN * 2,
  };
  const nodeCenterX = nodeRect.x + nodeRect.width / 2;
  const nodeCenterY = nodeRect.y + nodeRect.height / 2;
  const withinExpanded =
    nodeCenterX >= expanded.x &&
    nodeCenterX <= expanded.x + expanded.width &&
    nodeCenterY >= expanded.y &&
    nodeCenterY <= expanded.y + expanded.height;
  if (!withinExpanded) return null;

  const distances: Record<GroupEdge, number> = {
    left: Math.abs(nodeCenterX - group.x),
    right: Math.abs(nodeCenterX - (group.x + group.width)),
    top: Math.abs(nodeCenterY - group.y),
    bottom: Math.abs(nodeCenterY - (group.y + group.height)),
  };

  return (Object.keys(distances) as GroupEdge[]).reduce((closest, edge) =>
    distances[edge] < distances[closest] ? edge : closest,
  );
}
