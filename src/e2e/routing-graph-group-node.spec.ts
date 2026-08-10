import { expect, test } from "@playwright/test";

test.describe("Group node member list (issue #80, PD-035 revision)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/src/e2e/fixtures/routing-graph-harness-group-node.html");
    await page.waitForSelector(".vue-flow__node");
  });

  test("draws no edges from the Group node to its real members", async ({
    page,
  }) => {
    // The three plain device nodes (two members, one not-yet-added) plus
    // the group node itself — no edges at all, since the group has no
    // input connected in this fixture and its output side never renders as
    // edges regardless.
    await expect(page.locator(".vue-flow__node")).toHaveCount(4);
    await expect(page.locator(".vue-flow__edge")).toHaveCount(0);
  });

  test("the group node has no output pin, only an input pin", async ({
    page,
  }) => {
    const groupNode = page.locator(".vue-flow__node", {
      hasText: "Test Group",
    });
    await expect(
      groupNode.locator(".routing-graph-node-ports--in .vue-flow__handle"),
    ).toHaveCount(1);
    await expect(
      groupNode.locator(".routing-graph-node-ports--out .vue-flow__handle"),
    ).toHaveCount(0);
  });

  test("member list is hidden until expanded, then shows both member names", async ({
    page,
  }) => {
    await expect(page.locator(".routing-graph-group-members")).toHaveCount(0);

    await page.click('button[aria-label="Show members"]');

    const members = page.locator(
      ".routing-graph-group-member:not(.routing-graph-group-member--empty)",
    );
    await expect(members).toHaveCount(2);
    await expect(members.nth(0)).toContainText("Speakers");
    await expect(members.nth(1)).toContainText("Headphones");
  });

  test("hovering a member row highlights that member's real node", async ({
    page,
  }) => {
    await page.click('button[aria-label="Show members"]');

    const speakersNode = page
      .locator(".vue-flow__node", { hasText: "Speakers" })
      .first();
    await expect(speakersNode.locator(".routing-graph-node")).not.toHaveClass(
      /routing-graph-node--highlighted/,
    );

    const speakersRow = page.locator(".routing-graph-group-member", {
      hasText: "Speakers",
    });
    await speakersRow.hover();
    await expect(speakersNode.locator(".routing-graph-node")).toHaveClass(
      /routing-graph-node--highlighted/,
    );

    // Moving off the row clears it.
    await page.mouse.move(0, 0);
    await expect(speakersNode.locator(".routing-graph-node")).not.toHaveClass(
      /routing-graph-node--highlighted/,
    );
  });

  test("remove button on a member row is present and clickable", async ({
    page,
  }) => {
    await page.click('button[aria-label="Show members"]');

    const speakersRow = page.locator(".routing-graph-group-member", {
      hasText: "Speakers",
    });
    const removeButton = speakersRow.locator(
      "button.routing-graph-group-member-remove",
    );
    await expect(removeButton).toBeVisible();

    // No live Tauri runtime in this harness, so disconnect_processing_node_port
    // can't actually apply — this just proves the click reaches the handler
    // without throwing/crashing the page (mirrors the existing keyboard-connect
    // test's same "no backend" caveat).
    await removeButton.click();
    await expect(page.locator(".vue-flow__node")).toHaveCount(4);
  });

  test("+ add member button opens a picker of outputs not already in the group", async ({
    page,
  }) => {
    await page.click('button[aria-label="Add output to group"]');

    const picker = page.locator(".routing-graph-node-picker");
    await expect(picker).toBeVisible();
    const options = picker.locator("button");
    await expect(options).toHaveCount(1);
    await expect(options.first()).toContainText("HDMI");

    // No live Tauri runtime in this harness, so connect_processing_node_port
    // can't actually apply — this just proves the click reaches the handler
    // and closes the picker, same "no backend" caveat as the remove-button test.
    await options.first().click();
    await expect(page.locator(".routing-graph-node-picker")).toHaveCount(0);
  });

  test("the real member node itself shows no group-connection wiring or badge", async ({
    page,
  }) => {
    const speakersNode = page
      .locator(".vue-flow__node", { hasText: "Speakers" })
      .first();
    // A plain output device node — no edges point at or from it, and it
    // carries no special class indicating group membership.
    await expect(page.locator(".vue-flow__edge")).toHaveCount(0);
    await expect(speakersNode).not.toHaveClass(/group/);
  });
});
