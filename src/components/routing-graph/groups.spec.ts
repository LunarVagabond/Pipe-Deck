import { describe, expect, it } from "vitest";
import {
  boundsForMembers,
  containmentRatio,
  createGroup,
  GROUP_HEADER_HEIGHT,
  nearestGroupEdge,
  reflowMembers,
} from "./groups";

// loadGroups/saveGroups are localStorage-backed and intentionally not covered
// here — see the PR description for that scope decision.

describe("boundsForMembers", () => {
  it("fits a padded box around a single member", () => {
    const bounds = boundsForMembers([{ id: "a", position: { x: 100, y: 100 }, width: 200, height: 80 }]);
    expect(bounds.position).toEqual({ x: 68, y: 68 - GROUP_HEADER_HEIGHT });
    expect(bounds.size).toEqual({ width: 264, height: 144 + GROUP_HEADER_HEIGHT });
  });

  it("expands to cover multiple members", () => {
    const bounds = boundsForMembers([
      { id: "a", position: { x: 0, y: 0 }, width: 100, height: 50 },
      { id: "b", position: { x: 200, y: 100 }, width: 100, height: 50 },
    ]);
    expect(bounds.position).toEqual({ x: -32, y: -32 - GROUP_HEADER_HEIGHT });
    expect(bounds.size).toEqual({ width: 364, height: 214 + GROUP_HEADER_HEIGHT });
  });
});

describe("createGroup", () => {
  it("builds a group with a unique id and bounds fitting its members", () => {
    const members = [{ id: "a", position: { x: 0, y: 0 }, width: 100, height: 50 }];
    const group = createGroup("My Group", members);

    expect(group.label).toBe("My Group");
    expect(group.memberIds).toEqual(["a"]);
    expect(group.id).toMatch(/^group-/);
    expect(group.position).toEqual(boundsForMembers(members).position);
  });
});

describe("containmentRatio", () => {
  it("is 1 when inner is fully inside outer", () => {
    const inner = { x: 10, y: 10, width: 20, height: 20 };
    const outer = { x: 0, y: 0, width: 100, height: 100 };
    expect(containmentRatio(inner, outer)).toBe(1);
  });

  it("is 0 when there's no overlap", () => {
    const inner = { x: 200, y: 200, width: 20, height: 20 };
    const outer = { x: 0, y: 0, width: 100, height: 100 };
    expect(containmentRatio(inner, outer)).toBe(0);
  });

  it("is a fraction when partially overlapping", () => {
    const inner = { x: 90, y: 0, width: 20, height: 20 };
    const outer = { x: 0, y: 0, width: 100, height: 100 };
    expect(containmentRatio(inner, outer)).toBeCloseTo(0.5);
  });
});

describe("reflowMembers", () => {
  it("lays members out left-to-right along a shared top for a row", () => {
    const members = [
      { id: "a", position: { x: 0, y: 10 }, width: 100, height: 50 },
      { id: "b", position: { x: 0, y: 40 }, width: 80, height: 60 },
    ];
    const { positions } = reflowMembers("row", members);

    expect(positions.a).toEqual({ x: 0, y: 10 });
    expect(positions.b).toEqual({ x: 124, y: 10 });
  });

  it("lays members out top-to-bottom along a shared left for a column", () => {
    const members = [
      { id: "a", position: { x: 10, y: 0 }, width: 100, height: 50 },
      { id: "b", position: { x: 40, y: 0 }, width: 80, height: 60 },
    ];
    const { positions } = reflowMembers("column", members);

    expect(positions.a).toEqual({ x: 10, y: 0 });
    expect(positions.b).toEqual({ x: 10, y: 74 });
  });

  it("returns empty positions and zero bounds for no members", () => {
    const { positions, bounds } = reflowMembers("row", []);
    expect(positions).toEqual({});
    expect(bounds).toEqual({ position: { x: 0, y: 0 }, size: { width: 0, height: 0 } });
  });
});

describe("nearestGroupEdge", () => {
  const group = { x: 0, y: 0, width: 200, height: 200 };

  it("returns null when the node is far outside the hover margin", () => {
    const nodeRect = { x: 1000, y: 1000, width: 50, height: 50 };
    expect(nearestGroupEdge(nodeRect, group)).toBeNull();
  });

  it("picks the closest edge when hovering near the left side", () => {
    const nodeRect = { x: -60, y: 80, width: 20, height: 20 };
    expect(nearestGroupEdge(nodeRect, group)).toBe("left");
  });

  it("picks the closest edge when hovering near the bottom", () => {
    const nodeRect = { x: 80, y: 250, width: 20, height: 20 };
    expect(nearestGroupEdge(nodeRect, group)).toBe("bottom");
  });
});
