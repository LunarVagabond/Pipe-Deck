import { describe, expect, it } from "vitest";
import { computeSlotPosition, findDropTarget } from "./dropTarget";
import type { GraphGroup } from "./groups";

function makeGroup(overrides: Partial<GraphGroup> = {}): GraphGroup {
  return {
    id: "group-1",
    label: "Group",
    position: { x: 0, y: 0 },
    size: { width: 200, height: 200 },
    memberIds: ["a"],
    ...overrides,
  };
}

describe("findDropTarget", () => {
  it("returns null when no group is nearby", () => {
    const nodeRect = { x: 1000, y: 1000, width: 50, height: 50 };
    expect(findDropTarget(nodeRect, [makeGroup()])).toBeNull();
  });

  it("returns the nearest group with a row axis for a left/right edge", () => {
    const group = makeGroup();
    const nodeRect = { x: -60, y: 80, width: 20, height: 20 };
    const result = findDropTarget(nodeRect, [group]);
    expect(result).toEqual({ group, axis: "row", edge: "left" });
  });

  it("returns the nearest group with a column axis for a top/bottom edge", () => {
    const group = makeGroup();
    const nodeRect = { x: 80, y: 250, width: 20, height: 20 };
    const result = findDropTarget(nodeRect, [group]);
    expect(result).toEqual({ group, axis: "column", edge: "bottom" });
  });

  it("checks groups in order and returns the first match", () => {
    const near = makeGroup({ id: "near", position: { x: 0, y: 0 }, size: { width: 100, height: 100 } });
    const far = makeGroup({ id: "far", position: { x: 1000, y: 1000 }, size: { width: 100, height: 100 } });
    const nodeRect = { x: -60, y: 40, width: 20, height: 20 };
    expect(findDropTarget(nodeRect, [far, near])?.group.id).toBe("near");
  });
});

describe("computeSlotPosition", () => {
  const groupPosition = { x: 10, y: 10 };

  it("falls back to the group's own position when it has no members yet", () => {
    expect(computeSlotPosition([], groupPosition, "row", "right", { width: 50, height: 50 })).toEqual(
      groupPosition,
    );
  });

  it("places a left-edge row insert to the left of the leftmost member", () => {
    const members = [{ id: "a", position: { x: 100, y: 20 }, width: 100, height: 50 }];
    const position = computeSlotPosition(members, groupPosition, "row", "left", { width: 80, height: 50 });
    expect(position).toEqual({ x: 100 - 24 - 80, y: 20 });
  });

  it("places a right-edge row insert to the right of the rightmost member", () => {
    const members = [{ id: "a", position: { x: 100, y: 20 }, width: 100, height: 50 }];
    const position = computeSlotPosition(members, groupPosition, "row", "right", { width: 80, height: 50 });
    expect(position).toEqual({ x: 100 + 100 + 24, y: 20 });
  });

  it("places a top-edge column insert above the topmost member", () => {
    const members = [{ id: "a", position: { x: 30, y: 100 }, width: 100, height: 50 }];
    const position = computeSlotPosition(members, groupPosition, "column", "top", { width: 80, height: 40 });
    expect(position).toEqual({ x: 30, y: 100 - 24 - 40 });
  });

  it("places a bottom-edge column insert below the bottommost member", () => {
    const members = [{ id: "a", position: { x: 30, y: 100 }, width: 100, height: 50 }];
    const position = computeSlotPosition(members, groupPosition, "column", "bottom", { width: 80, height: 40 });
    expect(position).toEqual({ x: 30, y: 100 + 50 + 24 });
  });
});
