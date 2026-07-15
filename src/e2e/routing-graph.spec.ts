import { expect, test } from "@playwright/test";
import type { RoutingGraphHarness } from "./fixtures/routing-graph-harness-main";

declare global {
  interface Window {
    __harness: RoutingGraphHarness;
  }
}

test.describe("RoutingGraph edge rendering", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("a new connection renders its edge immediately, without a refresh", async ({ page }) => {
    await expect(page.locator(".vue-flow__edge")).toHaveCount(0);

    await page.evaluate(() => window.__harness.connectStreamToDevice("stream-1", "dev-out-1"));

    // No wait for a resize/refresh/navigation — the edge must appear on its own.
    await expect(page.locator(".vue-flow__edge")).toHaveCount(1);
    const path = page.locator(".vue-flow__edge-path");
    await expect(path).toHaveAttribute("d", /^M\d/);
  });

  test("an unrelated graph update (e.g. volume or mute) does not drop an existing edge", async ({
    page,
  }) => {
    await page.evaluate(() => window.__harness.connectStreamToDevice("stream-1", "dev-out-1"));
    await expect(page.locator(".vue-flow__edge")).toHaveCount(1);

    await page.evaluate(() => window.__harness.touchDevice("dev-out-1"));

    await expect(page.locator(".vue-flow__edge")).toHaveCount(1);
    const path = page.locator(".vue-flow__edge-path");
    await expect(path).toHaveAttribute("d", /^M\d/);
  });

  test("no Vue Flow edge-validation warnings are logged for a live connection", async ({ page }) => {
    const warnings: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "warning" && msg.text().includes("Vue Flow")) {
        warnings.push(msg.text());
      }
    });

    await page.evaluate(() => window.__harness.connectStreamToDevice("stream-1", "dev-out-1"));
    await page.evaluate(() => window.__harness.touchDevice("dev-out-1"));
    await expect(page.locator(".vue-flow__edge")).toHaveCount(1);

    expect(warnings).toEqual([]);
  });
});

test.describe("RoutingGraph multi-select drag", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("dragging one node in a multi-selection moves all selected nodes and it persists across a rebuild", async ({
    page,
  }) => {
    const nodes = page.locator(".vue-flow__node");
    await expect(nodes).toHaveCount(2);

    const boxA = await nodes.nth(0).boundingBox();
    const boxB = await nodes.nth(1).boundingBox();
    if (!boxA || !boxB) throw new Error("missing bounding boxes");

    // vue-flow's default multiSelectionKeyCode is Control (Meta on macOS), not
    // Shift — Control-click both nodes to build a multi-selection.
    await nodes.nth(0).click();
    await nodes.nth(1).click({ modifiers: ["Control"] });
    await expect(page.locator(".vue-flow__node.selected")).toHaveCount(2);

    const dx = 120;
    const dy = 80;

    // Drag the first (grabbed) node; the second should move along with it.
    await page.mouse.move(boxA.x + boxA.width / 2, boxA.y + boxA.height / 2);
    await page.mouse.down();
    await page.mouse.move(boxA.x + boxA.width / 2 + dx, boxA.y + boxA.height / 2 + dy, { steps: 10 });
    await page.mouse.up();

    const newBoxA = await nodes.nth(0).boundingBox();
    const newBoxB = await nodes.nth(1).boundingBox();
    if (!newBoxA || !newBoxB) throw new Error("missing bounding boxes after drag");

    // The canvas may be zoomed, so the on-screen delta won't exactly match the
    // mouse movement in px — assert both nodes moved by roughly the same
    // amount instead of an exact pixel count.
    const movedA = { x: newBoxA.x - boxA.x, y: newBoxA.y - boxA.y };
    const movedB = { x: newBoxB.x - boxB.x, y: newBoxB.y - boxB.y };
    expect(movedA.x).toBeGreaterThan(50);
    expect(movedA.y).toBeGreaterThan(30);
    expect(Math.abs(movedA.x - movedB.x)).toBeLessThan(2);
    expect(Math.abs(movedA.y - movedB.y)).toBeLessThan(2);

    // Force a graph rebuild (mirrors a live "graph-updated" push) and confirm
    // neither node snaps back to its pre-drag position.
    await page.evaluate(() => window.__harness.touchDevice("dev-out-1"));
    await page.waitForTimeout(200);

    const afterRebuildA = await nodes.nth(0).boundingBox();
    const afterRebuildB = await nodes.nth(1).boundingBox();
    if (!afterRebuildA || !afterRebuildB) throw new Error("missing bounding boxes after rebuild");

    expect(afterRebuildA.x).toBeCloseTo(newBoxA.x, 0);
    expect(afterRebuildA.y).toBeCloseTo(newBoxA.y, 0);
    expect(afterRebuildB.x).toBeCloseTo(newBoxB.x, 0);
    expect(afterRebuildB.y).toBeCloseTo(newBoxB.y, 0);
  });
});

test.describe("RoutingGraph grouping", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness-groups.html");
    await page.waitForSelector(".vue-flow__node");
  });

  async function createGroupFromFirstTwoNodes(page: import("@playwright/test").Page) {
    const nodes = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)");
    await nodes.nth(0).click();
    await nodes.nth(1).click({ modifiers: ["Control"] });
    await page.keyboard.press("g");
    await page.waitForSelector(".prompt-dialog-input");
    await page.click(".prompt-dialog-actions button.primary");
    await expect(page.locator(".vue-flow__node-groupNode")).toHaveCount(1);
  }

  test("selecting two nodes and pressing G creates a group containing them", async ({ page }) => {
    await createGroupFromFirstTwoNodes(page);
  });

  test("dragging a member out of its group leaves it near its pre-drag position, not off-screen", async ({
    page,
  }) => {
    await createGroupFromFirstTwoNodes(page);

    const member = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").first();
    const beforeBox = await member.boundingBox();
    if (!beforeBox) throw new Error("missing member bounding box");

    // Drag far enough to clear the group's bounds and trigger a detach.
    const dx = 700;
    const dy = 500;
    await page.mouse.move(beforeBox.x + beforeBox.width / 2, beforeBox.y + beforeBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      beforeBox.x + beforeBox.width / 2 + dx,
      beforeBox.y + beforeBox.height / 2 + dy,
      { steps: 10 },
    );
    await page.mouse.up();

    const afterDragBox = await member.boundingBox();
    if (!afterDragBox) throw new Error("missing member bounding box after drag");

    // Force a rebuild (mirrors a live graph-updated push) — a stale/relative
    // layout entry would surface here as a jump far from where the drag left it.
    await page.waitForTimeout(200);
    const afterRebuildBox = await member.boundingBox();
    if (!afterRebuildBox) throw new Error("missing member bounding box after rebuild");

    expect(Math.abs(afterRebuildBox.x - afterDragBox.x)).toBeLessThan(5);
    expect(Math.abs(afterRebuildBox.y - afterDragBox.y)).toBeLessThan(5);
  });

  test("ungrouping via the × button leaves members near their pre-ungroup position", async ({
    page,
  }) => {
    await createGroupFromFirstTwoNodes(page);

    const members = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)");
    const boxesBefore = await Promise.all([members.nth(0).boundingBox(), members.nth(1).boundingBox()]);
    if (!boxesBefore[0] || !boxesBefore[1]) throw new Error("missing member bounding boxes before ungroup");

    await page.click(".routing-graph-group-ungroup");
    await expect(page.locator(".vue-flow__node-groupNode")).toHaveCount(0);

    const boxesAfter = await Promise.all([members.nth(0).boundingBox(), members.nth(1).boundingBox()]);
    if (!boxesAfter[0] || !boxesAfter[1]) throw new Error("missing member bounding boxes after ungroup");

    for (let i = 0; i < 2; i += 1) {
      expect(Math.abs(boxesAfter[i]!.x - boxesBefore[i]!.x)).toBeLessThan(5);
      expect(Math.abs(boxesAfter[i]!.y - boxesBefore[i]!.y)).toBeLessThan(5);
    }
  });

  test("dragging a loose node into an existing group's bounds adds it as a member", async ({
    page,
  }) => {
    await createGroupFromFirstTwoNodes(page);

    const looseNode = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").nth(2);
    const looseBox = await looseNode.boundingBox();
    const groupBox = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!looseBox || !groupBox) throw new Error("missing bounding boxes");

    const targetX = groupBox.x + groupBox.width / 2;
    const targetY = groupBox.y + groupBox.height / 2;

    await page.mouse.move(looseBox.x + looseBox.width / 2, looseBox.y + looseBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(targetX, targetY, { steps: 10 });
    await page.mouse.up();

    // Vue Flow renders every node as a DOM sibling regardless of `parentNode`
    // (it's used for position tracking, not DOM nesting), so membership can't
    // be asserted via a nested-element selector. Force a rebuild and confirm
    // the node's rendered position now falls inside the group's bounds —
    // that's only true once buildRoutingGraph resolves it as a member and
    // computes its position relative to the group.
    await page.waitForTimeout(200);
    const memberBox = await looseNode.boundingBox();
    const groupBoxAfter = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!memberBox || !groupBoxAfter) throw new Error("missing bounding boxes after drop");

    const memberCenterX = memberBox.x + memberBox.width / 2;
    const memberCenterY = memberBox.y + memberBox.height / 2;
    expect(memberCenterX).toBeGreaterThanOrEqual(groupBoxAfter.x);
    expect(memberCenterX).toBeLessThanOrEqual(groupBoxAfter.x + groupBoxAfter.width);
    expect(memberCenterY).toBeGreaterThanOrEqual(groupBoxAfter.y);
    expect(memberCenterY).toBeLessThanOrEqual(groupBoxAfter.y + groupBoxAfter.height);
  });
});
