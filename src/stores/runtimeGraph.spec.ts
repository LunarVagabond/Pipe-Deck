import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";
import { flushPromises, mount } from "@vue/test-utils";
import { useAppConfig, useRuntimeGraph } from "./runtimeGraph";
import type {
  AppConfig,
  ProfileIndexEntry,
  RuntimeGraph,
} from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

type GraphEvent = { payload: RuntimeGraph };
type Composable = ReturnType<typeof useRuntimeGraph>;
type ConfigComposable = ReturnType<typeof useAppConfig>;

let capturedListener: ((event: GraphEvent) => void) | undefined;
let unlistenMock: ReturnType<typeof vi.fn>;
const activeWrappers: Array<{ unmount: () => void }> = [];
let harnessMode: "runtime" | "config" = "runtime";
let runtimeComposable!: Composable;
let configComposable!: ConfigComposable;

const Harness = defineComponent({
  setup() {
    if (harnessMode === "runtime") {
      runtimeComposable = useRuntimeGraph();
    } else {
      configComposable = useAppConfig();
    }
    return () => null;
  },
});

const initialGraph: RuntimeGraph = {
  devices: [],
  streams: [],
  links: [],
};

const defaultConfig: AppConfig = {
  version: 7,
  profile_index: [],
  preferences: { theme_mode: "dark" },
};

const defaultProfiles: ProfileIndexEntry[] = [
  { id: "profile-1", name: "Work", file: "work.toml" },
];

function graphWithDevice(id: string): RuntimeGraph {
  return {
    devices: [
      {
        id,
        system_name: `${id}-system`,
        label: id,
        kind: "physical",
        direction: "output",
      },
    ],
    streams: [],
    links: [],
  };
}

function mountHarness(mode: "runtime" | "config") {
  harnessMode = mode;
  const wrapper = mount(Harness);
  activeWrappers.push(wrapper);
  return wrapper;
}

async function mountRuntimeGraph() {
  const wrapper = mountHarness("runtime");
  await flushPromises();
  return { wrapper, composable: runtimeComposable };
}

async function mountAppConfig() {
  const wrapper = mountHarness("config");
  await flushPromises();
  return { wrapper, composable: configComposable };
}

beforeEach(() => {
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockImplementation((command: string) => {
    if (command === "get_runtime_graph") return Promise.resolve(initialGraph);
    if (command === "get_config") return Promise.resolve(defaultConfig);
    if (command === "list_profiles") return Promise.resolve(defaultProfiles);
    return Promise.resolve(undefined);
  });
  listenMock.mockReset();
  capturedListener = undefined;
  unlistenMock = vi.fn();
  listenMock.mockImplementation(
    (_event: string, callback: (event: GraphEvent) => void) => {
      capturedListener = callback;
      return Promise.resolve(unlistenMock);
    },
  );
});

afterEach(() => {
  for (const wrapper of activeWrappers) wrapper.unmount();
  activeWrappers.length = 0;
  vi.useRealTimers();
});

describe("useRuntimeGraph", () => {
  it("starts loading with no error, then stores the initial graph and registers the listener", async () => {
    let resolveGraph!: (graph: RuntimeGraph) => void;
    invokeMock.mockImplementation((command: string) =>
      command === "get_runtime_graph"
        ? new Promise<RuntimeGraph>((resolve) => {
            resolveGraph = resolve;
          })
        : Promise.resolve(undefined),
    );

    mountHarness("runtime");
    const composable = runtimeComposable;

    expect(composable.graph.value).toEqual(initialGraph);
    expect(composable.loading.value).toBe(true);
    expect(composable.error.value).toBeNull();

    resolveGraph(graphWithDevice("initial-device"));
    await flushPromises();

    expect(composable.graph.value).toEqual(graphWithDevice("initial-device"));
    expect(composable.loading.value).toBe(false);
    expect(composable.error.value).toBeNull();
    expect(listenMock).toHaveBeenCalledWith(
      "graph-updated",
      expect.any(Function),
    );
  });

  it.each([
    [new Error("PipeWire unavailable"), "PipeWire unavailable"],
    ["plain backend failure", "plain backend failure"],
  ])(
    "stringifies a %s refresh rejection and resets loading",
    async (rejection, message) => {
      invokeMock.mockImplementation((command: string) =>
        command === "get_runtime_graph"
          ? Promise.reject(rejection)
          : Promise.resolve(undefined),
      );

      const { composable } = await mountRuntimeGraph();

      expect(composable.error.value).toBe(message);
      expect(composable.loading.value).toBe(false);
      expect(listenMock).toHaveBeenCalledWith(
        "graph-updated",
        expect.any(Function),
      );
    },
  );

  it("refreshes through the returned public action and settles its state", async () => {
    const { composable } = await mountRuntimeGraph();
    const refreshedGraph = graphWithDevice("manual-refresh");
    let resolveRefresh!: (graph: RuntimeGraph) => void;
    invokeMock.mockImplementation((command: string) =>
      command === "get_runtime_graph"
        ? new Promise<RuntimeGraph>((resolve) => {
            resolveRefresh = resolve;
          })
        : Promise.resolve(undefined),
    );

    const refresh = composable.refresh();

    expect(composable.loading.value).toBe(true);
    expect(composable.error.value).toBeNull();

    resolveRefresh(refreshedGraph);
    await expect(refresh).resolves.toBeUndefined();

    expect(composable.graph.value).toEqual(refreshedGraph);
    expect(composable.loading.value).toBe(false);
    expect(composable.error.value).toBeNull();
  });

  it.each([
    [new Error("manual refresh failed"), "manual refresh failed"],
    ["plain manual refresh failure", "plain manual refresh failure"],
  ])(
    "normalizes a public refresh %s rejection and resets loading",
    async (rejection, message) => {
      const { composable } = await mountRuntimeGraph();
      composable.error.value = "stale error";
      let rejectRefresh!: (reason?: unknown) => void;
      invokeMock.mockImplementation((command: string) =>
        command === "get_runtime_graph"
          ? new Promise<RuntimeGraph>((_resolve, reject) => {
              rejectRefresh = reject;
            })
          : Promise.resolve(undefined),
      );

      const refresh = composable.refresh();

      expect(composable.loading.value).toBe(true);
      expect(composable.error.value).toBeNull();

      rejectRefresh(rejection);
      await expect(refresh).resolves.toBeUndefined();

      expect(composable.error.value).toBe(message);
      expect(composable.loading.value).toBe(false);
    },
  );

  it("applies an event after the trailing debounce and keeps the latest payload", async () => {
    const { composable } = await mountRuntimeGraph();
    const firstEvent = graphWithDevice("first-event");
    const latestEvent = graphWithDevice("latest-event");

    capturedListener?.({ payload: firstEvent });
    await vi.advanceTimersByTimeAsync(40);
    expect(composable.graph.value).toEqual(initialGraph);

    capturedListener?.({ payload: latestEvent });
    await vi.advanceTimersByTimeAsync(99);
    expect(composable.graph.value).toEqual(initialGraph);

    await vi.advanceTimersByTimeAsync(1);
    expect(composable.graph.value).toEqual(latestEvent);
  });

  it("applies the latest payload at the 150 ms max-wait even during sustained events", async () => {
    const { composable } = await mountRuntimeGraph();
    const firstEvent = graphWithDevice("first-event");
    const secondEvent = graphWithDevice("second-event");
    const latestEvent = graphWithDevice("latest-event");

    capturedListener?.({ payload: firstEvent });
    await vi.advanceTimersByTimeAsync(80);
    capturedListener?.({ payload: secondEvent });
    await vi.advanceTimersByTimeAsync(50);
    capturedListener?.({ payload: latestEvent });
    await vi.advanceTimersByTimeAsync(19);

    expect(composable.graph.value).toEqual(initialGraph);

    await vi.advanceTimersByTimeAsync(1);

    expect(composable.graph.value).toEqual(latestEvent);
  });

  it("does not apply an event immediately and clears stale error/loading when it is applied", async () => {
    const { composable } = await mountRuntimeGraph();
    const eventGraph = graphWithDevice("event-device");
    composable.error.value = "stale error";
    composable.loading.value = true;

    capturedListener?.({ payload: eventGraph });

    expect(composable.graph.value).toEqual(initialGraph);
    expect(composable.error.value).toBe("stale error");
    expect(composable.loading.value).toBe(true);

    await vi.advanceTimersByTimeAsync(100);

    expect(composable.graph.value).toEqual(eventGraph);
    expect(composable.error.value).toBeNull();
    expect(composable.loading.value).toBe(false);
  });

  it("unlistens on unmount", async () => {
    const { wrapper } = await mountRuntimeGraph();

    wrapper.unmount();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it("cancels a pending event when unmounted", async () => {
    const { wrapper, composable } = await mountRuntimeGraph();
    const pendingGraph = graphWithDevice("pending-event");
    capturedListener?.({ payload: pendingGraph });

    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(200);

    expect(composable.graph.value).toEqual(initialGraph);
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });
});

describe("useAppConfig", () => {
  it("loads config and profiles successfully", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") return Promise.resolve(defaultConfig);
      if (command === "list_profiles") return Promise.resolve(defaultProfiles);
      return Promise.resolve(undefined);
    });

    const { composable } = await mountAppConfig();

    expect(invokeMock).toHaveBeenNthCalledWith(1, "get_config");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "list_profiles");
    expect(composable.config.value).toEqual(defaultConfig);
    expect(composable.profiles.value).toEqual(defaultProfiles);
  });

  it.each(["get_config", "list_profiles"])(
    "uses the fallback config and empty profiles when %s fails",
    async (failedCommand) => {
      invokeMock.mockImplementation((command: string) => {
        if (command === failedCommand) {
          return Promise.reject(new Error(`${failedCommand} failed`));
        }
        if (command === "get_config") return Promise.resolve(defaultConfig);
        if (command === "list_profiles")
          return Promise.resolve(defaultProfiles);
        return Promise.resolve(undefined);
      });

      const { composable } = await mountAppConfig();

      expect(composable.config.value).toEqual({
        version: 1,
        profile_index: [],
        preferences: {},
      });
      expect(composable.profiles.value).toEqual([]);
    },
  );
});
