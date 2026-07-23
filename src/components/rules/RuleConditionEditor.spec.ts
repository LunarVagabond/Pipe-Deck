import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import RuleConditionEditor from "./RuleConditionEditor.vue";
import type { RuleCondition } from "../../types/graph";

function mountEditor(condition: RuleCondition, overrides: Record<string, unknown> = {}) {
  return mount(RuleConditionEditor, {
    props: {
      condition,
      active: false,
      canRemove: true,
      suggestions: [],
      ...overrides,
    },
  });
}

describe("RuleConditionEditor", () => {
  it("shows a plain value input for a simple condition type", () => {
    const wrapper = mountEditor({ type: "identity", value: "firefox" });

    const input = wrapper.find(".condition-field-grow input");
    expect(input.exists()).toBe(true);
    expect((input.element as HTMLInputElement).value).toBe("firefox");
  });

  it("shows field + pattern inputs for a regex condition", () => {
    const wrapper = mountEditor({ type: "regex", field: "app_name", pattern: "Disc.*" });

    const selects = wrapper.findAll("select");
    expect(selects).toHaveLength(2);
    const patternInput = wrapper.find('input[placeholder="e.g. Discord.*"]');
    expect((patternInput.element as HTMLInputElement).value).toBe("Disc.*");
  });

  it("shows a direction dropdown for a direction condition", () => {
    const wrapper = mountEditor({ type: "direction", value: "capture" });

    const selects = wrapper.findAll("select");
    expect((selects[1].element as HTMLSelectElement).value).toBe("capture");
  });

  it("shows a category dropdown for a category condition", () => {
    const wrapper = mountEditor({ type: "category", value: "Music" });

    const selects = wrapper.findAll("select");
    expect((selects[1].element as HTMLSelectElement).value).toBe("Music");
  });

  it("emits activate when clicked and remove when the remove button is clicked", async () => {
    const wrapper = mountEditor({ type: "identity", value: "" });

    await wrapper.trigger("click");
    expect(wrapper.emitted("activate")).toHaveLength(1);

    await wrapper.find(".condition-remove").trigger("click");
    expect(wrapper.emitted("remove")).toHaveLength(1);
  });

  it("disables the remove button when canRemove is false", () => {
    const wrapper = mountEditor({ type: "identity", value: "" }, { canRemove: false });

    expect(wrapper.find(".condition-remove").attributes("disabled")).toBeDefined();
  });

  it("applies the active class when active is true", () => {
    const wrapper = mountEditor({ type: "identity", value: "" }, { active: true });

    expect(wrapper.find(".condition-card").classes()).toContain("active");
  });

  it("renders suggestion chips and updates the value when one is clicked", async () => {
    const condition: RuleCondition = { type: "identity", value: "" };
    const wrapper = mountEditor(condition, { suggestions: ["firefox", "discord"] });

    const chips = wrapper.findAll(".condition-suggestion-chip");
    expect(chips.map((chip) => chip.text())).toEqual(["firefox", "discord"]);

    await chips[0].trigger("click");
    expect(condition.value).toBe("firefox");
  });

  it("mutates the condition in place when its value input changes", async () => {
    const condition: RuleCondition = { type: "executable", value: "" };
    const wrapper = mountEditor(condition);

    await wrapper.find(".condition-field-grow input").setValue("discord");
    expect(condition.value).toBe("discord");
  });
});
