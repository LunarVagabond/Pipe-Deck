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
