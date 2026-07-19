import { mount } from "@vue/test-utils";
import { ref } from "vue";
import { describe, expect, it, vi } from "vitest";
import Mixer from "./Mixer.vue";
import { makeDevice } from "../test/graphFixtures";
import type { RuntimeGraph } from "../types/graph";

const graph = ref<RuntimeGraph>({
  devices: [],
  streams: [],
  links: [],
});
const loading = ref(false);
const error = ref<string | null>(null);
const refresh = vi.fn();

vi.mock("../stores/runtimeGraph", () => ({
  useRuntimeGraph: () => ({ graph, loading, error, refresh }),
}));

describe("Mixer view", () => {
  it("shows an empty state when no mixer channels are available", () => {
    graph.value = { devices: [], streams: [], links: [] };
    loading.value = false;
    error.value = null;

    const wrapper = mount(Mixer);

    expect(wrapper.find(".empty").text()).toContain("No active audio streams");
    expect(wrapper.findComponent({ name: "MixerStrip" }).exists()).toBe(false);
  });

  it("renders mixer controls when a channel is available", () => {
    graph.value = {
      devices: [makeDevice({ id: "output-1", volume_percent: 62 })],
      streams: [],
      links: [],
    };
    loading.value = false;
    error.value = null;

    const wrapper = mount(Mixer);

    expect(wrapper.find(".empty").exists()).toBe(false);
    expect(wrapper.findComponent({ name: "MixerStrip" }).exists()).toBe(true);
  });
});
