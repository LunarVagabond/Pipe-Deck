import { expect, test } from "@playwright/test";
import type { RoutingGraphHarness } from "./fixtures/routing-graph-harness-main";

declare global {
  interface Window {
    __harness: RoutingGraphHarness;
  }
}

test.describe("RoutingGraph edge rendering", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/e2e/fixtures/routing-graph-harness.html");
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
