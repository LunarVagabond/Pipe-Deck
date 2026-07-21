import { createApp, defineComponent, h, reactive } from "vue";
import RoutingGraph from "../../../src/components/RoutingGraph.vue";
import "@vue-flow/core/dist/style.css";
import "@vue-flow/core/dist/theme-default.css";
import "@vue-flow/controls/dist/style.css";
import "../../../src/styles/main.scss";
import type { ActionStatus, RuntimeGraph } from "../../src/types/graph";

/**
 * Minimal host for RoutingGraph.vue, used by e2e/routing-graph.spec.ts to drive
 * the component with a synthetic RuntimeGraph without needing a real Tauri/
 * PipeWire backend. Mutating `graph` here is equivalent to what happens when
 * the app's "graph-updated" Tauri event delivers a fresh RuntimeGraph.
 */
export interface RoutingGraphHarness {
  graph: RuntimeGraph;
  connectStreamToDevice(streamId: string, deviceId: string): void;
  touchDevice(deviceId: string): void;
  /** Seeds a minimal route_explanation on a stream so tests can drive the
   * on-graph warning badge (issue #154) without a full explanation fixture. */
  setStreamRouteStatus(streamId: string, actionStatus: ActionStatus): void;
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
  ],
  streams: [
    {
      id: "stream-1",
      app_name: "Test App",
      direction: "playback",
      volume_percent: 60,
      muted: false,
    },
  ],
  links: [],
});

const harness: RoutingGraphHarness = {
  graph,
  connectStreamToDevice(streamId, deviceId) {
    const stream = graph.streams.find((entry) => entry.id === streamId);
    if (stream) {
      stream.current_target = deviceId;
    }
    graph.links.push({ id: `link-${streamId}-${deviceId}`, source_id: streamId, target_id: deviceId });
  },
  touchDevice(deviceId) {
    const device = graph.devices.find((entry) => entry.id === deviceId);
    if (!device) return;
    device.volume_percent = ((device.volume_percent ?? 0) + 1) % 101;
    device.muted = !device.muted;
  },
  setStreamRouteStatus(streamId, actionStatus) {
    const stream = graph.streams.find((entry) => entry.id === streamId);
    if (!stream) return;
    stream.route_explanation = {
      source: "authored_rule",
      match_reasons: [],
      skipped_candidates: [],
      action_status: actionStatus,
      fallback_applied: false,
    };
  },
};

declare global {
  interface Window {
    __harness: RoutingGraphHarness;
  }
}

window.__harness = harness;

const Harness = defineComponent({
  setup() {
    return () =>
      h("div", { style: "height:100vh;display:flex;flex-direction:column;" }, [
        h("div", { style: "flex:1;min-height:0;display:flex;flex-direction:column;" }, [
          h(RoutingGraph, { graph }),
        ]),
      ]);
  },
});

createApp(Harness).mount("#app");
