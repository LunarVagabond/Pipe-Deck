export interface GraphRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface GraphGroup {
  id: string;
  label: string;
  position: { x: number; y: number };
  size: { width: number; height: number };
  memberIds: string[];
}

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

interface GroupMemberInput {
  id: string;
  position: { x: number; y: number };
  width: number;
  height: number;
}

export function createGroup(label: string, members: GroupMemberInput[]): GraphGroup {
  const minX = Math.min(...members.map((member) => member.position.x));
  const minY = Math.min(...members.map((member) => member.position.y));
  const maxX = Math.max(...members.map((member) => member.position.x + member.width));
  const maxY = Math.max(...members.map((member) => member.position.y + member.height));

  return {
    id: `group-${Date.now()}-${Math.round(Math.random() * 1e5)}`,
    label,
    position: { x: minX - GROUP_PADDING, y: minY - GROUP_PADDING - GROUP_HEADER_HEIGHT },
    size: {
      width: maxX - minX + GROUP_PADDING * 2,
      height: maxY - minY + GROUP_PADDING * 2 + GROUP_HEADER_HEIGHT,
    },
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
