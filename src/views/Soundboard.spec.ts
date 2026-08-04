import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import Soundboard from "./Soundboard.vue";

describe("Soundboard", () => {
  it("renders the empty-state skeleton", () => {
    const wrapper = mount(Soundboard);

    expect(wrapper.find(".soundboard-view").exists()).toBe(true);
    expect(wrapper.find(".soundboard-empty-state").text()).toContain("No sound clips configured yet.");
  });
});
