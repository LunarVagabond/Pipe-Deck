import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import RouteExplanationPanel from "./RouteExplanationPanel.vue";
import { makeStream } from "../test/graphFixtures";

function mountPanel() {
  const stream = makeStream({
    id: "s1",
    route_explanation: {
      source: "authored_rule",
      match_reasons: ["app_name matched \"Test App\""],
      skipped_candidates: [],
      action_status: "applied",
      fallback_applied: false,
    },
  });
  return mount(RouteExplanationPanel, {
    props: { stream, devices: [] },
  });
}

describe("RouteExplanationPanel", () => {
  it("starts collapsed with aria-expanded false and no rendered detail region", () => {
    const wrapper = mountPanel();
    const toggle = wrapper.get(".route-explanation-toggle");

    expect(toggle.attributes("aria-expanded")).toBe("false");
    expect(wrapper.find(".route-explanation-detail").exists()).toBe(false);
  });

  it("expands on click, exposing a labelled region matched by aria-controls/id", async () => {
    const wrapper = mountPanel();
    const toggle = wrapper.get(".route-explanation-toggle");

    await toggle.trigger("click");

    expect(toggle.attributes("aria-expanded")).toBe("true");
    const controlsId = toggle.attributes("aria-controls");
    expect(controlsId).toBeTruthy();

    const detail = wrapper.get(".route-explanation-detail");
    expect(detail.attributes("id")).toBe(controlsId);
    expect(detail.attributes("role")).toBe("region");
    expect(detail.attributes("aria-labelledby")).toBe(toggle.attributes("id"));
  });

  it("never renders a live region, so refresh churn can't spam announcements", async () => {
    const wrapper = mountPanel();
    await wrapper.get(".route-explanation-toggle").trigger("click");

    expect(wrapper.find("[aria-live]").exists()).toBe(false);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });

  it("hides the decorative chevron from assistive tech", () => {
    const wrapper = mountPanel();
    expect(wrapper.get(".route-explanation-chevron").attributes("aria-hidden")).toBe("true");
  });

  it("'Change route' focuses the matching data-stream-route-select element", async () => {
    const select = document.createElement("select");
    select.setAttribute("data-stream-route-select", "s1");
    document.body.appendChild(select);

    const stream = makeStream({
      id: "s1",
      route_explanation: {
        source: "authored_rule",
        match_reasons: [],
        skipped_candidates: [],
        action_status: "applied",
        fallback_applied: false,
      },
    });
    const wrapper = mount(RouteExplanationPanel, {
      props: { stream, devices: [] },
      attachTo: document.body,
    });

    try {
      await wrapper.get(".route-explanation-toggle").trigger("click");
      await wrapper.get(".route-explanation-fix").trigger("click");

      expect(document.activeElement).toBe(select);
    } finally {
      wrapper.unmount();
      select.remove();
    }
  });
});
