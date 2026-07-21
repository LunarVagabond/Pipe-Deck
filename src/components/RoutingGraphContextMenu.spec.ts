import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import RoutingGraphContextMenu from "./RoutingGraphContextMenu.vue";
import type { RoutingGraphPaneMenuTarget } from "../composables/routingGraphContext";

function paneTarget(): RoutingGraphPaneMenuTarget {
  return { kind: "pane", x: 100, y: 200 };
}

describe("RoutingGraphContextMenu bring-node-here picker", () => {
  it("opens the node list and emits bring-node-here with the picked id", async () => {
    const wrapper = mount(RoutingGraphContextMenu, {
      props: {
        target: paneTarget(),
        nodes: [
          { id: "stream:s1", label: "Discord" },
          { id: "device:d1", label: "Speakers" },
        ],
      },
    });

    expect(wrapper.find(".routing-graph-node-picker").exists()).toBe(false);

    const trigger = wrapper.findAll("button").find((b) => b.text().includes("Bring node here"));
    await trigger?.trigger("click");

    const picker = wrapper.find(".routing-graph-node-picker");
    expect(picker.exists()).toBe(true);
    const nodeButtons = picker.findAll("button");
    expect(nodeButtons).toHaveLength(2);
    expect(nodeButtons[0].text()).toBe("Discord");

    await nodeButtons[0].trigger("click");

    expect(wrapper.emitted("bring-node-here")).toEqual([["stream:s1"]]);
    expect(wrapper.find(".routing-graph-node-picker").exists()).toBe(false);
  });

  it("closes the picker when the target changes", async () => {
    const wrapper = mount(RoutingGraphContextMenu, {
      props: {
        target: paneTarget(),
        nodes: [{ id: "stream:s1", label: "Discord" }],
      },
    });

    const buttons = wrapper.findAll("button");
    const trigger = buttons.find((b) => b.text().includes("Bring node here"));
    await trigger?.trigger("click");
    expect(wrapper.find(".routing-graph-node-picker").exists()).toBe(true);

    await wrapper.setProps({ target: { kind: "pane", x: 5, y: 5 } });
    expect(wrapper.find(".routing-graph-node-picker").exists()).toBe(false);
  });
});
