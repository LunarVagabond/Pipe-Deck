import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import RuleListItem from "./RuleListItem.vue";
import type { Rule } from "../../types/graph";

function makeRule(overrides: Partial<Rule> = {}): Rule {
  return {
    id: "rule-1",
    name: "Discord to headset",
    enabled: true,
    priority: 10,
    conditions: [{ type: "identity", value: "discord" }],
    action: { target_system_name: "virtual-headset" },
    safeguards: { fallback_policy: "keep_current" },
    ...overrides,
  };
}

function mountItem(overrides: Partial<Rule> = {}, propsOverrides: Record<string, unknown> = {}) {
  return mount(RuleListItem, {
    props: {
      rule: makeRule(overrides),
      targetKindLabel: "Output",
      targetName: "Headset",
      liveMatchCount: 0,
      canMoveUp: true,
      canMoveDown: true,
      ...propsOverrides,
    },
  });
}

describe("RuleListItem", () => {
  it("renders rule name, priority, target, and formatted conditions", () => {
    const wrapper = mountItem();

    expect(wrapper.find(".rule-name-meta strong").text()).toBe("Discord to headset");
    expect(wrapper.find(".rule-meta").text()).toContain("10");
    expect(wrapper.find(".rule-target-kind").text()).toBe("Output");
    expect(wrapper.find(".rule-target-name").text()).toBe("Headset");
    expect(wrapper.find(".rule-condition-text").text()).toBe("discord");
  });

  it("shows a live-match badge when the rule currently matches something", () => {
    const wrapper = mountItem({}, { liveMatchCount: 3 });

    const badge = wrapper.find(".rule-match-badge");
    expect(badge.classes()).toContain("rule-match-badge-active");
    expect(badge.text()).toBe("Matching 3 now");
  });

  it("shows an idle badge when there is no live match", () => {
    const wrapper = mountItem({}, { liveMatchCount: 0 });

    const badge = wrapper.find(".rule-match-badge");
    expect(badge.classes()).toContain("rule-match-badge-idle");
    expect(badge.text()).toBe("No live match");
  });

  it("applies the disabled row class when the rule is disabled", () => {
    const wrapper = mountItem({ enabled: false });

    expect(wrapper.find("tr").classes()).toContain("rule-row-disabled");
  });

  it("disables the move buttons per canMoveUp/canMoveDown and emits reorder events", async () => {
    const wrapper = mountItem({}, { canMoveUp: false, canMoveDown: true });

    const [up, down] = wrapper.findAll(".rule-priority-btn");
    expect(up.attributes("disabled")).toBeDefined();
    expect(down.attributes("disabled")).toBeUndefined();

    await down.trigger("click");
    expect(wrapper.emitted("move-down")).toHaveLength(1);
  });

  it("emits edit, delete, and toggle-enabled", async () => {
    const wrapper = mountItem();

    await wrapper.find(".rule-card-actions button:not(.danger)").trigger("click");
    await wrapper.find(".rule-card-actions button.danger").trigger("click");
    expect(wrapper.emitted("edit")).toHaveLength(1);
    expect(wrapper.emitted("delete")).toHaveLength(1);

    await wrapper.findComponent({ name: "ToggleSwitch" }).vm.$emit("update:modelValue", false);
    expect(wrapper.emitted("toggle-enabled")?.[0]).toEqual([false]);
  });
});
