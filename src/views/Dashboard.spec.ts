import { flushPromises, mount, shallowMount } from "@vue/test-utils";
import { ref } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App.vue";
import Dashboard from "./Dashboard.vue";
import { useGlobalSearch } from "../composables/useGlobalSearch";
import { navigateKey } from "../composables/navigation";
import { makeDevice, makeGraph, makeStream } from "../test/graphFixtures";
import type { AppConfig, RuntimeGraph } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("../stores/updateStatus", () => ({
  useUpdateStatus: () => ({
    updateStatus: ref("unknown"),
    updateStatusText: ref("Unknown"),
    checkForUpdatesNow: vi.fn(),
  }),
}));
vi.mock("../stores/ruleDraft", () => ({
  useRuleDraft: () => ({
    requestRuleForIdentity: vi.fn(),
    consumePendingIdentity: vi.fn().mockReturnValue(null),
  }),
}));

const graph = ref<RuntimeGraph>(makeGraph());
const config = ref<AppConfig | null>(null);
vi.mock("../stores/runtimeGraph", () => ({
  useRuntimeGraph: () => ({
    graph,
    loading: ref(false),
    error: ref(null),
    refresh: vi.fn(),
  }),
  useAppConfig: () => ({ config }),
}));

let wrapper: ReturnType<typeof mount> | undefined;
const { searchQuery } = useGlobalSearch();

beforeEach(() => {
  searchQuery.value = "";
  config.value = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation((command: string) => {
    if (command === "get_config") return Promise.resolve({ preferences: {} });
    return Promise.resolve(null);
  });
  graph.value = {
    ...makeGraph(
      [
        makeDevice({ id: "speaker", label: "Speakers" }),
        makeDevice({ id: "mic", label: "Desk mic", direction: "input" }),
        makeDevice({
          id: "virtual",
          label: "Virtual bus",
          kind: "virtual",
          current_target: "route-single",
          current_targets: ["route-multi"],
        }),
        makeDevice({
          id: "internal",
          label: "Speakers internal",
          system_name: "pipe-deck-feed-x",
        }),
      ],
      [
        makeStream({ id: "music", current_target: "speaker" }),
        makeStream({ id: "call", direction: "capture", current_target: "mic" }),
        makeStream({ id: "system", is_system: true }),
      ],
    ),
    recent_stream_identities: [
      {
        app_name: "Recent app",
        direction: "playback",
        is_live: false,
        last_seen_secs: Math.floor(Date.now() / 1000) - 30,
      },
    ],
  };
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
});

describe("Dashboard global search", () => {
  it("filters only device cards, preserving all stats and recently seen entries", async () => {
    wrapper = mount(Dashboard, {
      attachTo: document.body,
      global: { provide: { [navigateKey as symbol]: vi.fn() } },
    });
    await flushPromises();
    const labels = () =>
      wrapper!
        .findAll(".device-summary-card .stat-label")
        .map((node) => node.text());
    const stats = () =>
      wrapper!.findAll(".stat-value").map((node) => node.text());

    expect(labels()).toEqual(["Speakers", "Desk mic", "Virtual bus"]);
    expect(stats()).toEqual(["1", "1", "4", "1"]);

    searchQuery.value = " speakers ";
    await flushPromises();
    expect(labels()).toEqual(["Speakers"]);
    expect(stats()).toEqual(["1", "1", "4", "1"]);
    expect(
      wrapper.findAll(".recently-seen-info strong").map((node) => node.text()),
    ).toEqual(["Recent app"]);

    searchQuery.value = "internal";
    await flushPromises();
    expect(labels()).toEqual([]);
    expect(stats()).toEqual(["1", "1", "4", "1"]);

    for (const query of ["", " \t "]) {
      searchQuery.value = query;
      await flushPromises();
      expect(labels()).toEqual(["Speakers", "Desk mic", "Virtual bus"]);
      expect(stats()).toEqual(["1", "1", "4", "1"]);
    }
  });

  it("binds the accessible shell search input to the shared query in both directions", async () => {
    searchQuery.value = "Speakers";
    wrapper = shallowMount(App, { attachTo: document.body });
    await flushPromises();
    const input = wrapper.get<HTMLInputElement>('.topbar input[type="search"]');
    expect(input.attributes("aria-label")).toBe(
      "Search devices, streams, and rules",
    );
    expect(input.attributes("placeholder")).toBe(
      "Search devices, streams, and rules",
    );
    expect(input.element.value).toBe("Speakers");
    await input.setValue("Music");
    expect(searchQuery.value).toBe("Music");
    searchQuery.value = "";
    await flushPromises();
    expect(input.element.value).toBe("");
  });
});
