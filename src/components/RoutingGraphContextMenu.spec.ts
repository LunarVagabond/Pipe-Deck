import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import RoutingGraphContextMenu from "./RoutingGraphContextMenu.vue";
import type { RoutingGraphMultiNodeMenuTarget, RoutingGraphPaneMenuTarget } from "../composables/routingGraphContext";

function paneTarget(): RoutingGraphPaneMenuTarget {
  return { kind: "pane", x: 100, y: 200 };
}

function multiNodeTarget(): RoutingGraphMultiNodeMenuTarget {
  return {
    kind: "multi-node",
    x: 100,
    y: 200,
    memberDeviceIds: ["device:out1", "device:out2"],
    memberLabels: ["Speakers", "Recorder"],
  };
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

describe("RoutingGraphContextMenu multi-node target (issue #80)", () => {
  it("shows the selected count and emits group-outputs on click", async () => {
    const wrapper = mount(RoutingGraphContextMenu, {
      props: { target: multiNodeTarget() },
    });

    expect(wrapper.text()).toContain("2 outputs selected");

    const button = wrapper.findAll("button").find((b) => b.text().includes("Group Selected Outputs"));
    expect(button?.exists()).toBe(true);
    await button?.trigger("click");

    expect(wrapper.emitted("group-outputs")).toHaveLength(1);
  });
});
