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

test.describe("RoutingGraph keyboard connect", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("a port is keyboard-focusable and Enter arms Vue Flow's click-to-connect state on it", async ({
    page,
  }) => {
    const outputPort = page.locator(".routing-graph-handle.source").first();
    await outputPort.focus();
    await expect(outputPort).toBeFocused();

    await page.keyboard.press("Enter");
    await expect(outputPort).toHaveClass(/connecting/);
  });

  test("Escape cancels a picked-up port", async ({ page }) => {
    const outputPort = page.locator(".routing-graph-handle.source").first();
    await outputPort.focus();
    await page.keyboard.press("Enter");
    await expect(outputPort).toHaveClass(/connecting/);

    await page.keyboard.press("Escape");
    await expect(outputPort).not.toHaveClass(/connecting/);
  });

  test("pressing Enter again on the same picked-up port cancels it", async ({ page }) => {
    const outputPort = page.locator(".routing-graph-handle.source").first();
    await outputPort.focus();
    await page.keyboard.press("Enter");
    await expect(outputPort).toHaveClass(/connecting/);

    await page.keyboard.press("Enter");
    await expect(outputPort).not.toHaveClass(/connecting/);
  });

  test("Enter on a picked-up output port, then Enter on a compatible input port, completes the click-connect sequence", async ({
    page,
  }) => {
    const outputPort = page.locator(".routing-graph-handle.source").first();
    const inputPort = page.locator(".routing-graph-handle.target.is-empty").first();

    await outputPort.focus();
    await page.keyboard.press("Enter");
    await expect(outputPort).toHaveClass(/connecting/);

    await inputPort.focus();
    await page.keyboard.press("Enter");

    // This harness has no live Tauri runtime, so the resulting `invoke()`
    // call can't actually apply the route — but Vue Flow always clears its
    // armed click-connect state once a second click resolves (valid or not),
    // so seeing it clear here proves the keyboard Enter on the input port
    // reached Vue Flow's own connect-completion handling, the same path a
    // mouse click on the port would take.
    await expect(outputPort).not.toHaveClass(/connecting/);
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

  test("dragging a member within the group tracks the cursor instead of jumping away", async ({ page }) => {
    await createGroupFromFirstTwoNodes(page);

    const member = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").first();
    const beforeBox = await member.boundingBox();
    if (!beforeBox) throw new Error("missing member bounding box");

    const dx = 20;
    const dy = 15;
    await page.mouse.move(beforeBox.x + beforeBox.width / 2, beforeBox.y + beforeBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      beforeBox.x + beforeBox.width / 2 + dx,
      beforeBox.y + beforeBox.height / 2 + dy,
      { steps: 5 },
    );

    // Check the position mid-drag (before mouseup) — a member with a stale
    // absolute/relative position mix jumps away the instant the drag starts,
    // well before any detach threshold is reached.
    const midDragBox = await member.boundingBox();
    await page.mouse.up();
    if (!midDragBox) throw new Error("missing member bounding box mid-drag");

    expect(Math.abs(midDragBox.x - (beforeBox.x + dx))).toBeLessThan(20);
    expect(Math.abs(midDragBox.y - (beforeBox.y + dy))).toBeLessThan(20);
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

  test("dragging a loose node near a group's right edge inserts it as a member there", async ({
    page,
  }) => {
    await createGroupFromFirstTwoNodes(page);

    const looseNode = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").nth(2);
    const looseBox = await looseNode.boundingBox();
    const groupBox = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!looseBox || !groupBox) throw new Error("missing bounding boxes");

    // Drop just outside the group's right edge, vertically centered — this
    // is what should trigger the "insert at right edge" drop-slot preview
    // and, on release, the actual insertion.
    const targetX = groupBox.x + groupBox.width + 20;
    const targetY = groupBox.y + groupBox.height / 2;

    await page.mouse.move(looseBox.x + looseBox.width / 2, looseBox.y + looseBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(targetX, targetY, { steps: 10 });

    // A live drop-slot preview ghost should appear while hovering the edge.
    await expect(page.locator(".routing-graph-drop-slot-overlay")).toHaveCount(1);

    await page.mouse.up();
    await expect(page.locator(".routing-graph-drop-slot-overlay")).toHaveCount(0);

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

    // Reflowing into a row can shrink the group's *height* if the original
    // free-form members happened to be stacked vertically (their height
    // collapses to a single row once aligned), so area/height aren't
    // reliable invariants here — but a row with one more member in it is
    // always at least as wide as before.
    expect(groupBoxAfter.width).toBeGreaterThan(groupBox.width);
  });

  test("a member leaving a row-aligned (directionally-inserted) group reflows the remaining members", async ({
    page,
  }) => {
    await createGroupFromFirstTwoNodes(page);

    // Insert the third node at the right edge first, so the group is
    // row-aligned (layoutAxis: "row") rather than free-form.
    const looseNode = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").nth(2);
    const looseBox = await looseNode.boundingBox();
    const groupBoxInit = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!looseBox || !groupBoxInit) throw new Error("missing bounding boxes");

    await page.mouse.move(looseBox.x + looseBox.width / 2, looseBox.y + looseBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(groupBoxInit.x + groupBoxInit.width + 20, groupBoxInit.y + groupBoxInit.height / 2, {
      steps: 10,
    });
    await page.mouse.up();
    await page.waitForTimeout(200);

    const groupBoxWithThree = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!groupBoxWithThree) throw new Error("missing group bounding box with three members");

    // Now drag the (now-aligned) first member far away to detach it.
    const member = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").first();
    const memberBox = await member.boundingBox();
    if (!memberBox) throw new Error("missing member bounding box");

    await page.mouse.move(memberBox.x + memberBox.width / 2, memberBox.y + memberBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(memberBox.x + memberBox.width / 2 + 700, memberBox.y + memberBox.height / 2 + 500, {
      steps: 10,
    });
    await page.mouse.up();
    await page.waitForTimeout(200);

    const groupBoxAfterDetach = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!groupBoxAfterDetach) throw new Error("missing group bounding box after detach");

    // A row with one fewer member is narrower than one with three — the
    // detached member's old slot was closed up by the reflow rather than
    // just leaving the bounding box shrink-wrapped around a stale gap.
    expect(groupBoxAfterDetach.width).toBeLessThan(groupBoxWithThree.width);
  });

  test("the group shrinks to fit after a member is dragged out", async ({ page }) => {
    await createGroupFromFirstTwoNodes(page);

    const groupBoxBefore = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!groupBoxBefore) throw new Error("missing group bounding box before detach");

    const member = page.locator(".vue-flow__node:not(.vue-flow__node-groupNode)").first();
    const memberBox = await member.boundingBox();
    if (!memberBox) throw new Error("missing member bounding box");

    const dx = 700;
    const dy = 500;
    await page.mouse.move(memberBox.x + memberBox.width / 2, memberBox.y + memberBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(memberBox.x + memberBox.width / 2 + dx, memberBox.y + memberBox.height / 2 + dy, {
      steps: 10,
    });
    await page.mouse.up();

    const groupBoxAfter = await page.locator(".vue-flow__node-groupNode").boundingBox();
    if (!groupBoxAfter) throw new Error("missing group bounding box after detach");

    expect(groupBoxAfter.width * groupBoxAfter.height).toBeLessThan(
      groupBoxBefore.width * groupBoxBefore.height,
    );
  });

  test("picking a group color updates the group panel's border color", async ({ page }) => {
    await createGroupFromFirstTwoNodes(page);

    const group = page.locator(".vue-flow__node-groupNode .routing-graph-group");
    await expect(group).toHaveCSS("border-top-color", "rgba(255, 255, 255, 0.25)");

    await page.click(".routing-graph-group-color-swatch");
    await page.waitForSelector(".routing-graph-group-color-popover");
    await page.click(".routing-graph-group-color-option");

    await expect(group).not.toHaveCSS("border-top-color", "rgba(255, 255, 255, 0.25)");
  });
});

test.describe("RoutingGraph bring node here", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("right-click pane -> Bring node here -> pick a node relocates it to the click point", async ({
    page,
  }) => {
    const streamNode = page.locator(".vue-flow__node", { hasText: "Test App" });
    const boxBefore = await streamNode.boundingBox();
    if (!boxBefore) throw new Error("missing bounding box before relocation");

    // Right-click a spot on the canvas well away from the node's current
    // position, but high enough up that the menu (which grows downward from
    // the click point) still fits inside the default 720px-tall viewport.
    const clickPoint = { x: boxBefore.x + 400, y: boxBefore.y + 60 };
    await page.mouse.click(clickPoint.x, clickPoint.y, { button: "right" });
    await page.waitForSelector(".routing-graph-context-menu");

    const trigger = page.locator(".routing-graph-context-menu button", { hasText: "Bring node here" });
    await trigger.click();
    await page.waitForSelector(".routing-graph-node-picker");

    const pickButton = page.locator(".routing-graph-node-picker button", { hasText: "Test App" });
    await pickButton.click();

    await expect(page.locator(".routing-graph-context-menu")).toHaveCount(0);

    const boxAfter = await streamNode.boundingBox();
    if (!boxAfter) throw new Error("missing bounding box after relocation");

    // The node should have moved substantially toward the click point rather
    // than staying at its pre-relocation spot.
    expect(Math.abs(boxAfter.x - boxBefore.x)).toBeGreaterThan(50);

    const layoutRaw = await page.evaluate(() => localStorage.getItem("pipe-deck-routing-layout"));
    expect(layoutRaw).toBeTruthy();
    const layout = JSON.parse(layoutRaw ?? "{}");
    expect(layout["stream:stream-1"]).toBeDefined();
  });
});

test.describe("RoutingGraph route-warning badge", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("a blocked route renders the blocked warning badge on the stream's node", async ({ page }) => {
    const node = page.locator(".vue-flow__node", { hasText: "Test App" });
    await expect(node.locator(".routing-graph-node-warning-badge")).toHaveCount(0);

    await page.evaluate(() => window.__harness.setStreamRouteStatus("stream-1", "blocked"));

    await expect(node.locator(".routing-graph-node-warning-badge--blocked")).toHaveCount(1);
  });

  test("a target_unavailable route renders the unavailable warning badge", async ({ page }) => {
    const node = page.locator(".vue-flow__node", { hasText: "Test App" });

    await page.evaluate(() => window.__harness.setStreamRouteStatus("stream-1", "target_unavailable"));

    await expect(node.locator(".routing-graph-node-warning-badge--unavailable")).toHaveCount(1);
  });

  test("an applied route does not render a warning badge", async ({ page }) => {
    const node = page.locator(".vue-flow__node", { hasText: "Test App" });

    await page.evaluate(() => window.__harness.setStreamRouteStatus("stream-1", "applied"));

    await expect(node.locator(".routing-graph-node-warning-badge")).toHaveCount(0);
  });
});
