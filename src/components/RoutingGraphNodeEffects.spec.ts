import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import RoutingGraphNodeEffects from "./RoutingGraphNodeEffects.vue";
import { emptyDynamicsStage, emptyEq5BandStage, type EffectChainConfig } from "../types/graph";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

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

beforeEach(() => {
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "get_effect_capabilities") {
      return Promise.resolve({ builtin_eq: true, builtin_gain: true, builtin_limiter: false });
    }
    return Promise.resolve({ success: true });
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("RoutingGraphNodeEffects bypass toggle", () => {
  it("does not render a bypass button when the chain has no stages", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_effect_chains" ? Promise.resolve({}) : Promise.resolve({ success: true }),
    );
    const wrapper = mount(RoutingGraphNodeEffects, {
      props: { channelType: "device", entityId: "dev-1", deviceId: "dev-1", label: "Virtual Output" },
    });
    await flushPromises();

    expect(wrapper.find(".routing-graph-node-bypass").exists()).toBe(false);
  });

  it("renders a bypass button reflecting live state, and toggling it applies live params", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chainWithStage({ bypassed: false }) });
      if (cmd === "get_effect_capabilities") {
        return Promise.resolve({ builtin_eq: true, builtin_gain: true, builtin_limiter: false });
      }
      return Promise.resolve({ success: true });
    });
    const wrapper = mount(RoutingGraphNodeEffects, {
      props: { channelType: "device", entityId: "dev-1", deviceId: "dev-1", label: "Virtual Output" },
    });
    await flushPromises();

    const button = wrapper.get(".routing-graph-node-bypass");
    expect(button.classes()).not.toContain("bypassed");
    expect(button.attributes("aria-label")).toBe("Bypass effects");

    await button.trigger("click");
    await vi.advanceTimersByTimeAsync(60);
    await flushPromises();

    expect(invokeMock).toHaveBeenCalledWith(
      "set_effect_chain_live_params",
      expect.objectContaining({ deviceId: "dev-1", config: expect.objectContaining({ bypassed: true }) }),
    );
  });

  it("reflects an already-bypassed chain with the resume label and active state", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_effect_chains") return Promise.resolve({ "dev-1": chainWithStage({ bypassed: true }) });
      if (cmd === "get_effect_capabilities") {
        return Promise.resolve({ builtin_eq: true, builtin_gain: true, builtin_limiter: false });
      }
      return Promise.resolve({ success: true });
    });
    const wrapper = mount(RoutingGraphNodeEffects, {
      props: { channelType: "device", entityId: "dev-1", deviceId: "dev-1", label: "Virtual Output" },
    });
    await flushPromises();

    const button = wrapper.get(".routing-graph-node-bypass");
    expect(button.classes()).toContain("bypassed");
    expect(button.attributes("aria-label")).toBe("Resume effects processing");
  });
});
