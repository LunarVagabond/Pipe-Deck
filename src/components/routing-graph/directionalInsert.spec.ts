import { describe, expect, it } from "vitest";
import { planDirectionalInsert } from "./directionalInsert";
import type { GroupMemberInput } from "./groups";

describe("planDirectionalInsert", () => {
  const groupPosition = { x: 0, y: 0 };

  it("prepends a member inserted at the left edge and reflows the row", () => {
    const existing: GroupMemberInput[] = [{ id: "a", position: { x: 100, y: 10 }, width: 100, height: 50 }];
    const node = { id: "new", dimensions: { width: 80, height: 50 } };

    const plan = planDirectionalInsert(existing, groupPosition, "row", "left", node);

    expect(plan.orderedMembers.map((m) => m.id)).toEqual(["new", "a"]);
    expect(plan.positions.new.x).toBeLessThan(plan.positions.a.x);
  });

  it("appends a member inserted at the right edge and reflows the row", () => {
    const existing: GroupMemberInput[] = [{ id: "a", position: { x: 100, y: 10 }, width: 100, height: 50 }];
    const node = { id: "new", dimensions: { width: 80, height: 50 } };

    const plan = planDirectionalInsert(existing, groupPosition, "row", "right", node);

    expect(plan.orderedMembers.map((m) => m.id)).toEqual(["a", "new"]);
    expect(plan.positions.a.x).toBeLessThan(plan.positions.new.x);
  });

  it("prepends a member inserted at the top edge and reflows the column", () => {
    const existing: GroupMemberInput[] = [{ id: "a", position: { x: 10, y: 100 }, width: 100, height: 50 }];
    const node = { id: "new", dimensions: { width: 80, height: 40 } };

    const plan = planDirectionalInsert(existing, groupPosition, "column", "top", node);

    expect(plan.orderedMembers.map((m) => m.id)).toEqual(["new", "a"]);
    expect(plan.positions.new.y).toBeLessThan(plan.positions.a.y);
  });

  it("falls back to a 200x80 default rect when the node has no measured dimensions yet", () => {
    const node = { id: "new", dimensions: { width: 0, height: 0 } };

    const plan = planDirectionalInsert([], groupPosition, "row", "right", node);

    expect(plan.orderedMembers).toEqual([{ id: "new", position: groupPosition, width: 200, height: 80 }]);
  });

  it("recomputes bounds to fit the full reflowed set", () => {
    const existing: GroupMemberInput[] = [{ id: "a", position: { x: 100, y: 10 }, width: 100, height: 50 }];
    const node = { id: "new", dimensions: { width: 80, height: 50 } };

    const plan = planDirectionalInsert(existing, groupPosition, "row", "right", node);

    expect(plan.bounds.size.width).toBeGreaterThan(100);
  });
});
