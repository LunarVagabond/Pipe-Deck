import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import RoutingGraph from "./RoutingGraph.vue";
import { makeGraph } from "../test/graphFixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

// jsdom in this project's vitest config doesn't back window.localStorage by
// default (see groups.spec.ts's note on loadGroups/saveGroups) — RoutingGraph.vue
// reads/writes it directly (routing groups, and an onMounted cleanup of a
// legacy "pipe-deck-routing-reroutes" key), so a minimal in-memory stub is
// needed just to get past mount, independent of the VueFlow question below.
function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
    removeItem: (key: string) => store.delete(key),
    clear: () => store.clear(),
  });
}

// Named so the interaction test below can find it via findComponent({ name })
// and $emit a custom event directly — $emit doesn't require a declared
// `emits` option to reach the parent's @pane-context-menu/@pane-click
// listeners, so a bare stub template is enough.
const VueFlowStub = { name: "VueFlowStub", template: "<div><slot /></div>" };

function mountRoutingGraph(graph = makeGraph()) {
  return mount(RoutingGraph, {
    props: { graph },
    global: {
      stubs: {
        VueFlow: VueFlowStub,
        Background: true,
        Controls: true,
      },
    },
  });
}

describe("RoutingGraph.vue smoke mount", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("mounts with VueFlow/Background/Controls stubbed", () => {
    expect(() => mountRoutingGraph()).not.toThrow();
  });

  it("marks the canvas idle when the graph has no links yet", () => {
    const wrapper = mountRoutingGraph(makeGraph());
    expect(wrapper.find(".routing-graph-canvas--idle").exists()).toBe(true);
  });

  it("does not mark the canvas idle once a link exists", () => {
    const wrapper = mountRoutingGraph(
      makeGraph(
        [
          { id: "dev-1", system_name: "physical-out-1", label: "Speakers", kind: "physical", direction: "output", volume_percent: 80, muted: false },
        ],
        [{ id: "stream-1", app_name: "Test App", direction: "playback", volume_percent: 60, muted: false }],
        [{ id: "link-1", source_id: "stream-1", target_id: "dev-1" }],
      ),
    );
    expect(wrapper.find(".routing-graph-canvas--idle").exists()).toBe(false);
  });

  it("opens the context menu with a pane target on right-click and closes it on pane click", async () => {
    const wrapper = mountRoutingGraph();
    const vueFlow = wrapper.findComponent({ name: "VueFlowStub" });

    await vueFlow.vm.$emit("pane-context-menu", { preventDefault: () => {}, clientX: 40, clientY: 60 });
    expect(wrapper.find(".routing-graph-context-menu").exists()).toBe(true);

    await vueFlow.vm.$emit("pane-click");
    expect(wrapper.find(".routing-graph-context-menu").exists()).toBe(false);
  });
});
