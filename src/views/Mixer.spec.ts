import { flushPromises, mount } from "@vue/test-utils";
import { ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Mixer from "./Mixer.vue";
import { useGlobalSearch } from "../composables/useGlobalSearch";
import { makeDevice, makeGraph, makeStream } from "../test/graphFixtures";
import type { RuntimeGraph } from "../types/graph";

const graph = ref<RuntimeGraph>({
  devices: [],
  streams: [],
  links: [],
});
const loading = ref(false);
const error = ref<string | null>(null);
const refresh = vi.fn();

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

vi.mock("../stores/runtimeGraph", () => ({
  useRuntimeGraph: () => ({ graph, loading, error, refresh }),
}));

let activeWrapper: ReturnType<typeof mount> | undefined;
const { searchQuery } = useGlobalSearch();

function mountMixer() {
  activeWrapper = mount(Mixer, { attachTo: document.body });
  return activeWrapper;
}

beforeEach(() => {
  searchQuery.value = "";
  graph.value = makeGraph();
  loading.value = false;
  error.value = null;
  invokeMock.mockReset();
});

afterEach(() => {
  activeWrapper?.unmount();
  activeWrapper = undefined;
});

describe("Mixer view", () => {
  it("shows an empty state when no mixer channels are available", () => {
    graph.value = { devices: [], streams: [], links: [] };
    loading.value = false;
    error.value = null;

    const wrapper = mountMixer();

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

    const wrapper = mountMixer();

    expect(wrapper.find(".empty").exists()).toBe(false);
    expect(wrapper.findComponent({ name: "MixerStrip" }).exists()).toBe(true);
  });

  it("filters stream channels with the shared query while keeping devices and hiding system streams", async () => {
    graph.value = makeGraph(
      [makeDevice({ id: "speakers", label: "Speakers" })],
      [
        makeStream({ id: "music-1", app_name: "Music" }),
        makeStream({ id: "music-2", app_name: "Music" }),
        makeStream({
          id: "browser",
          app_name: "Browser",
          executable: "firefox",
        }),
        makeStream({ id: "system", app_name: "Music system", is_system: true }),
      ],
    );
    searchQuery.value = " music ";
    const wrapper = mountMixer();
    await flushPromises();
    const labels = () =>
      wrapper.findAll(".stream-channel-label").map((node) => node.text());
    const devices = () =>
      wrapper.findAll(".node-card-title strong").map((node) => node.text());
    expect(labels()).toEqual(["Music", "Music"]);
    expect(devices()).toEqual(["Speakers"]);

    searchQuery.value = "firefox";
    await flushPromises();
    expect(labels()).toEqual(["Browser"]);
    expect(devices()).toEqual(["Speakers"]);

    searchQuery.value = "nothing-matches";
    await flushPromises();
    expect(labels()).toEqual([]);
    expect(devices()).toEqual(["Speakers"]);

    for (const query of ["", " \t "]) {
      searchQuery.value = query;
      await flushPromises();
      expect(labels()).toEqual(["Music", "Music", "Browser"]);
      expect(devices()).toEqual(["Speakers"]);
    }
  });
});
