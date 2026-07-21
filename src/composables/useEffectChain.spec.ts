import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { defineComponent } from "vue";
import { mount, flushPromises } from "@vue/test-utils";
import { emptyDynamicsStage, emptyEq5BandStage, type EffectChainConfig } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());
const pushNoticeMock = vi.hoisted(() => vi.fn());
const handleApplyResultMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({ handleApplyResult: handleApplyResultMock }),
  useNotices: () => ({ pushNotice: pushNoticeMock }),
}));

function chainWithStage(overrides: Partial<EffectChainConfig> = {}): EffectChainConfig {
  return {
    stages: [emptyEq5BandStage("stage-1")],
    compressor: emptyDynamicsStage(),
    limiter: emptyDynamicsStage(),
    noise_gate: emptyDynamicsStage(),
    bypassed: false,
    ...overrides,
  };
}

let capturedListener: (() => void) | undefined;

async function mountEffectChain() {
  const { useEffectChain } = await import("./useEffectChain");
  let composable!: ReturnType<typeof useEffectChain>;
  const wrapper = mount(
    defineComponent({
      setup() {
        composable = useEffectChain();
        return () => null;
      },
    }),
  );
  await flushPromises();
  return { wrapper, get composable() {
    return composable;
  } };
}

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  invokeMock.mockReset();
  pushNoticeMock.mockReset();
  handleApplyResultMock.mockReset();
  capturedListener = undefined;
  listenMock.mockReset();
  listenMock.mockImplementation((_event: string, callback: () => void) => {
    capturedListener = callback;
    return Promise.resolve(vi.fn());
  });
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "get_effect_chains") return Promise.resolve({});
    if (cmd === "get_effect_capabilities") {
      return Promise.resolve({ builtin_eq: true, builtin_gain: true, builtin_limiter: false });
    }
    return Promise.resolve({ success: true });
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("mount lifecycle", () => {
  it("fetches chains and capabilities on mount and stops loading", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chainWithStage() });
      if (cmd === "get_effect_capabilities") {
        return Promise.resolve({ builtin_eq: true, builtin_gain: false, builtin_limiter: false });
      }
      return Promise.resolve({ success: true });
    });

    const { composable } = await mountEffectChain();

    expect(invokeMock).toHaveBeenCalledWith("get_effect_chains");
    expect(invokeMock).toHaveBeenCalledWith("get_effect_capabilities");
    expect(composable.loading.value).toBe(false);
    expect(composable.chains.value["dev-1"]).toBeDefined();
    expect(composable.capabilities.value).toEqual({ builtin_eq: true, builtin_gain: false, builtin_limiter: false });
    expect(listenMock).toHaveBeenCalledWith("graph-updated", expect.any(Function));
  });

  it("resets chains to empty on a get_effect_chains failure", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.reject(new Error("fetch failed"));
      if (cmd === "get_effect_capabilities") {
        return Promise.resolve({ builtin_eq: true, builtin_gain: true, builtin_limiter: false });
      }
      return Promise.resolve({ success: true });
    });

    const { composable } = await mountEffectChain();

    expect(composable.chains.value).toEqual({});
    expect(composable.loading.value).toBe(false);
  });

  it("resets capabilities to the all-false default on a get_effect_capabilities failure", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({});
      if (cmd === "get_effect_capabilities") return Promise.reject(new Error("fetch failed"));
      return Promise.resolve({ success: true });
    });

    const { composable } = await mountEffectChain();

    expect(composable.capabilities.value).toEqual({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
  });

  it("re-fetches when a graph-updated event fires", async () => {
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    capturedListener?.();
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith("get_effect_chains");
    expect(composable).toBeDefined();
  });

  it("calls unlisten on unmount", async () => {
    const unlisten = vi.fn();
    listenMock.mockImplementation(() => Promise.resolve(unlisten));

    const { wrapper } = await mountEffectChain();
    wrapper.unmount();

    expect(unlisten).toHaveBeenCalled();
  });
});

describe("chainFor", () => {
  it("returns the fetched chain for a known device", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chainWithStage() });
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });

    const { composable } = await mountEffectChain();

    expect(composable.chainFor("dev-1").stages).toHaveLength(1);
  });

  it("falls back to an empty chain for an unknown device", async () => {
    const { composable } = await mountEffectChain();

    expect(composable.chainFor("unknown-dev")).toEqual({
      stages: [],
      compressor: emptyDynamicsStage(),
      limiter: emptyDynamicsStage(),
      noise_gate: emptyDynamicsStage(),
      bypassed: false,
    });
  });
});

describe("addEq5BandStage", () => {
  it("adds the stage and refreshes on success, showing the restart toast once", async () => {
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    await composable.addEq5BandStage("dev-1");

    expect(invokeMock).toHaveBeenCalledWith(
      "add_effect_stage",
      expect.objectContaining({ deviceId: "dev-1", stage: expect.objectContaining({ kind: "eq5band" }) }),
    );
    expect(invokeMock).toHaveBeenCalledWith("get_effect_chains");
    expect(pushNoticeMock).toHaveBeenCalledTimes(1);
    expect(pushNoticeMock).toHaveBeenCalledWith("info", expect.stringContaining("restarts"));

    await composable.addEq5BandStage("dev-1");
    expect(pushNoticeMock).toHaveBeenCalledTimes(1);
  });

  it("reports a failure via handleApplyResult without throwing", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "add_effect_stage") return Promise.reject(new Error("nope"));
      if (cmd === "get_effect_chains") return Promise.resolve({});
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });
    const { composable } = await mountEffectChain();

    await composable.addEq5BandStage("dev-1");

    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "nope" }, "");
  });
});

describe("removeStage", () => {
  it("removes the stage and refreshes on success", async () => {
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    await composable.removeStage("dev-1", "stage-1");

    expect(invokeMock).toHaveBeenCalledWith("remove_effect_stage", { deviceId: "dev-1", stageId: "stage-1" });
    expect(invokeMock).toHaveBeenCalledWith("get_effect_chains");
  });

  it("stringifies a non-Error rejection for handleApplyResult", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "remove_effect_stage") return Promise.reject("plain string failure");
      if (cmd === "get_effect_chains") return Promise.resolve({});
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });
    const { composable } = await mountEffectChain();

    await composable.removeStage("dev-1", "stage-1");

    expect(handleApplyResultMock).toHaveBeenCalledWith(
      { success: false, message: "plain string failure" },
      "",
    );
  });
});

describe("reorderStages", () => {
  it("optimistically reorders stages before the invoke resolves", async () => {
    const chain = chainWithStage({ stages: [emptyEq5BandStage("a"), emptyEq5BandStage("b")] });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      if (cmd === "reorder_effect_stages") return new Promise(() => {});
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });
    const { composable } = await mountEffectChain();

    void composable.reorderStages("dev-1", ["b", "a"]);

    expect(composable.chains.value["dev-1"].stages.map((s) => s.id)).toEqual(["b", "a"]);
  });

  it("filters out ids that don't match any existing stage", async () => {
    const chain = chainWithStage({ stages: [emptyEq5BandStage("a"), emptyEq5BandStage("b")] });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      if (cmd === "reorder_effect_stages") return new Promise(() => {});
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });
    const { composable } = await mountEffectChain();

    void composable.reorderStages("dev-1", ["missing", "b", "a"]);

    expect(composable.chains.value["dev-1"].stages.map((s) => s.id)).toEqual(["b", "a"]);
  });

  it("is a no-op locally when the chain doesn't exist yet", async () => {
    const { composable } = await mountEffectChain();

    await composable.reorderStages("unknown-dev", ["a", "b"]);

    expect(composable.chains.value["unknown-dev"]).toBeUndefined();
  });

  it("refreshes in both the success and failure paths", async () => {
    const chain = chainWithStage();
    let getEffectChainsCalls = 0;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") {
        getEffectChainsCalls += 1;
        return Promise.resolve({ "dev-1": chain });
      }
      if (cmd === "reorder_effect_stages") return Promise.reject(new Error("reorder failed"));
      return Promise.resolve({ builtin_eq: false, builtin_gain: false, builtin_limiter: false });
    });
    const { composable } = await mountEffectChain();
    const callsAfterMount = getEffectChainsCalls;

    await composable.reorderStages("dev-1", ["stage-1"]);

    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "reorder failed" }, "");
    expect(getEffectChainsCalls).toBe(callsAfterMount + 1);
  });
});

describe("scheduleStageUpdate", () => {
  it("applies the update locally right away, and debounces the live-params invoke", async () => {
    const chain = chainWithStage();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    const updatedStage = { ...emptyEq5BandStage("stage-1"), eq_bass: 5 };
    composable.scheduleStageUpdate("dev-1", updatedStage);

    expect(composable.chains.value["dev-1"].stages[0].eq_bass).toBe(5);
    await vi.advanceTimersByTimeAsync(59);
    expect(invokeMock).not.toHaveBeenCalledWith("set_effect_chain_live_params", expect.anything());

    await vi.advanceTimersByTimeAsync(1);
    expect(invokeMock).toHaveBeenCalledWith(
      "set_effect_chain_live_params",
      expect.objectContaining({ deviceId: "dev-1" }),
    );
  });

  it("collapses rapid repeated calls into a single invoke with the latest value", async () => {
    const chain = chainWithStage();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    composable.scheduleStageUpdate("dev-1", { ...emptyEq5BandStage("stage-1"), eq_bass: 1 });
    await vi.advanceTimersByTimeAsync(20);
    composable.scheduleStageUpdate("dev-1", { ...emptyEq5BandStage("stage-1"), eq_bass: 2 });
    await vi.advanceTimersByTimeAsync(60);

    const liveParamCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "set_effect_chain_live_params");
    expect(liveParamCalls).toHaveLength(1);
    expect(liveParamCalls[0][1]).toEqual(
      expect.objectContaining({
        config: expect.objectContaining({ stages: [expect.objectContaining({ eq_bass: 2 })] }),
      }),
    );
  });

  it("reports a failure via handleApplyResult", async () => {
    const chain = chainWithStage();
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      if (cmd === "set_effect_chain_live_params") return Promise.reject(new Error("push failed"));
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();

    composable.scheduleStageUpdate("dev-1", emptyEq5BandStage("stage-1"));
    await vi.advanceTimersByTimeAsync(60);
    await flushPromises();

    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "push failed" }, "");
  });
});

describe("setBypassed", () => {
  it("debounces a live-params push when the chain has stages", async () => {
    const chain = chainWithStage({ bypassed: false });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    composable.setBypassed("dev-1", true);

    expect(composable.chains.value["dev-1"].bypassed).toBe(true);
    expect(invokeMock).not.toHaveBeenCalledWith("set_effect_chain_live_params", expect.anything());

    await vi.advanceTimersByTimeAsync(60);
    expect(invokeMock).toHaveBeenCalledWith(
      "set_effect_chain_live_params",
      expect.objectContaining({ deviceId: "dev-1", config: expect.objectContaining({ bypassed: true }) }),
    );
  });

  it("persists immediately with no debounce when the chain has no stages", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({});
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    composable.setBypassed("dev-1", true);
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith(
      "set_device_effects",
      expect.objectContaining({ deviceId: "dev-1", config: expect.objectContaining({ bypassed: true }) }),
    );
  });

  it("reports a failure via handleApplyResult for the immediate persist-only path", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({});
      if (cmd === "set_device_effects") return Promise.reject(new Error("persist failed"));
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();

    composable.setBypassed("dev-1", true);
    await flushPromises();

    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "persist failed" }, "");
  });
});

describe("setDynamicsStageEnabled", () => {
  it("applies the change locally and persists via set_device_effects", async () => {
    const { composable } = await mountEffectChain();
    invokeMock.mockClear();

    await composable.setDynamicsStageEnabled("dev-1", "compressor", true);

    expect(composable.chains.value["dev-1"].compressor.enabled).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith(
      "set_device_effects",
      expect.objectContaining({ deviceId: "dev-1", config: expect.objectContaining({ compressor: expect.objectContaining({ enabled: true }) }) }),
    );
  });

  it("reports a failure via handleApplyResult", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({});
      if (cmd === "set_device_effects") return Promise.reject(new Error("nope"));
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();

    await composable.setDynamicsStageEnabled("dev-1", "limiter", true);

    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "nope" }, "");
  });
});

describe("pendingWrites guard", () => {
  it("keeps an in-flight write's optimistic value instead of letting a concurrent refresh stomp it", async () => {
    const chain = chainWithStage();
    let resolveLiveParams: (() => void) | undefined;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chain });
      if (cmd === "set_effect_chain_live_params") {
        return new Promise<void>((resolve) => {
          resolveLiveParams = resolve;
        });
      }
      return Promise.resolve({ success: true });
    });
    const { composable } = await mountEffectChain();

    const updatedStage = { ...emptyEq5BandStage("stage-1"), eq_bass: 7 };
    composable.scheduleStageUpdate("dev-1", updatedStage);
    await vi.advanceTimersByTimeAsync(60);
    // The write is now in flight (pendingWrites["dev-1"] === true) but not yet resolved.

    // A stale graph-updated refresh lands while the write is still pending.
    capturedListener?.();
    await flushPromises();

    expect(composable.chains.value["dev-1"].stages[0].eq_bass).toBe(7);

    resolveLiveParams?.();
    await flushPromises();
  });
});
