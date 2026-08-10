import { createApp, defineComponent, h, reactive } from "vue";
import RoutingGraph from "../../../src/components/RoutingGraph.vue";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "../../../src/styles/main.scss";
import type { RuntimeGraph } from "../../src/types/graph";

/**
 * Minimal host for RoutingGraph.vue used to test the Group node's inline
 * member list (issue #80, PD-035 revision) — a fresh, dedicated fixture
 * rather than reusing routing-graph-harness.html/-groups.html since neither
 * seeds a `processing_nodes` entry.
 */
export interface RoutingGraphGroupNodeHarness {
  graph: RuntimeGraph;
}

const graph = reactive<RuntimeGraph>({
  devices: [
    {
      id: "dev-out-1",
      system_name: "physical-out-1",
      label: "Speakers",
      kind: "physical",
      direction: "output",
      volume_percent: 80,
      muted: false,
    },
    {
      id: "dev-out-2",
      system_name: "physical-out-2",
      label: "Headphones",
      kind: "physical",
      direction: "output",
      volume_percent: 80,
      muted: false,
    },
    {
      id: "dev-out-3",
      system_name: "physical-out-3",
      label: "HDMI",
      kind: "physical",
      direction: "output",
      volume_percent: 80,
      muted: false,
    },
  ],
  streams: [],
  links: [],
  processing_nodes: [
    {
      id: "proc-group-1",
      label: "Test Group",
      kind: { kind: "group", volume_percent: 100, muted: false },
      system_name: "pipe-deck-proc-group-test-group",
      bypassed: false,
      live: true,
      inputs: [],
      outputs: [
        { index: 0, connected_id: "dev-out-1" },
        { index: 1, connected_id: "dev-out-2" },
      ],
    },
  ],
});

const harness: RoutingGraphGroupNodeHarness = { graph };

declare global {
  interface Window {
    __groupNodeHarness: RoutingGraphGroupNodeHarness;
  }
}

window.__groupNodeHarness = harness;

const Harness = defineComponent({
  setup() {
    return () =>
      h("div", { style: "height:100vh;display:flex;flex-direction:column;" }, [
        h(
          "div",
          { style: "flex:1;min-height:0;display:flex;flex-direction:column;" },
          [h(RoutingGraph, { graph })],
        ),
      ]);
  },
});

createApp(Harness).mount("#app");
