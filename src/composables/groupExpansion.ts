/** Per-Group-node collapse/expand state (issue #80, PD-035) — purely a
 * canvas display preference, not backend state, so it's persisted the same
 * lightweight way `routing-graph/groups.ts`'s cosmetic bounding boxes are:
 * a flat localStorage-backed id list, no store/composable ceremony. Default
 * is collapsed (a Group's internal wiring to its members is hidden until
 * the user explicitly expands it) — see `collectEdges.ts`'s use of this.
 */

const EXPANDED_GROUPS_KEY = "pipe-deck-expanded-groups";

export function loadExpandedGroupNodeIds(): Set<string> {
  try {
    const raw = localStorage.getItem(EXPANDED_GROUPS_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw) as string[];
    return Array.isArray(parsed) ? new Set(parsed) : new Set();
  } catch {
    return new Set();
  }
}

export function saveExpandedGroupNodeIds(ids: ReadonlySet<string>) {
  localStorage.setItem(EXPANDED_GROUPS_KEY, JSON.stringify([...ids]));
}
