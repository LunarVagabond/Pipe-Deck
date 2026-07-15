import { createApp, defineComponent, h, reactive } from "vue";
import RoutingGraph from "../../../src/components/RoutingGraph.vue";
import PromptDialog from "../../../src/components/PromptDialog.vue";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "../../../src/styles/main.scss";
import type { RuntimeGraph } from "../../src/types/graph";

/**
 * Minimal host for RoutingGraph.vue used by the grouping e2e specs. Grouping
 * needs at least 3 independent nodes (2 to select+group, 1 to leave loose for
 * the re-add-to-group case) and the global PromptDialog mounted, since "G"
 * grouping prompts for a name via the shared usePrompt() store — the routing
 * harness used by routing-graph.spec.ts only has 2 nodes and doesn't mount it.
 */
export interface RoutingGraphGroupsHarness {
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
});

const harness: RoutingGraphGroupsHarness = { graph };

declare global {
  interface Window {
    __groupsHarness: RoutingGraphGroupsHarness;
  }
}

window.__groupsHarness = harness;

const Harness = defineComponent({
  setup() {
    return () =>
      h("div", { style: "height:100vh;display:flex;flex-direction:column;" }, [
        h(PromptDialog),
        h("div", { style: "flex:1;min-height:0;display:flex;flex-direction:column;" }, [
          h(RoutingGraph, { graph }),
        ]),
      ]);
  },
});

createApp(Harness).mount("#app");
