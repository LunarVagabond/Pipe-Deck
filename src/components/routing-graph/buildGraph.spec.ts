import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeDevice, makeGraph, makeProcessingNode, makeStream } from "../../test/graphFixtures";
import { buildRoutingGraph, deviceNodeId, processingNodeNodeId, streamNodeId } from "./buildGraph";
import type { RoutingGraphNodeData } from "./buildGraph";
import type { ActionStatus } from "../../types/graph";

// Node's own global `localStorage` (unrelated to jsdom's) takes precedence
// in this Vitest/Node combo and has no-op storage methods, so
// `buildRoutingGraph`'s layout persistence silently breaks unless a real
// backing store is stubbed in for the test.
function stubLocalStorage() {
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => store.set(key, value),
    removeItem: (key: string) => store.delete(key),
    clear: () => store.clear(),
  });
}

describe("streamNodeKind route warnings", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataFor(action_status: ActionStatus | undefined): RoutingGraphNodeData | undefined {
    const stream = makeStream({
      id: "s1",
      route_explanation: action_status
        ? {
            source: "authored_rule",
            match_reasons: [],
            skipped_candidates: [],
            action_status,
            fallback_applied: false,
          }
        : undefined,
    });
    const graph = makeGraph([], [stream]);
    const node = buildRoutingGraph(graph).nodes.find((n) => n.id === streamNodeId("s1"));
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("has no warning when there is no route explanation", () => {
    expect(dataFor(undefined)?.routeWarning).toBeUndefined();
  });

  it.each<[ActionStatus, "blocked" | "unavailable"]>([
    ["blocked", "blocked"],
    ["skipped_manual_override", "blocked"],
    ["target_unavailable", "unavailable"],
  ])("maps action_status %s to routeWarning %s", (status, expected) => {
    const data = dataFor(status);
    expect(data?.routeWarning).toBe(expected);
    expect(data?.routeWarningTitle).toBeTruthy();
  });

  it.each<ActionStatus>(["applied", "simulated", "no_action"])(
    "has no warning for action_status %s",
    (status) => {
      expect(dataFor(status)?.routeWarning).toBeUndefined();
    },
  );
});

describe("streamNodeKind format-mismatch badge (#156)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataForFormats(
    stream: { sample_rate?: number; channels?: number },
    device: { sample_rate?: number; channels?: number },
  ): RoutingGraphNodeData | undefined {
    const target = makeDevice({ id: "d1", ...device });
    const s = makeStream({ id: "s1", current_target: "d1", ...stream });
    const graph = makeGraph([target], [s]);
    const node = buildRoutingGraph(graph).nodes.find((n) => n.id === streamNodeId("s1"));
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("has no badge when rate and channels match", () => {
    const data = dataForFormats(
      { sample_rate: 48000, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(data?.formatMismatch).toBeUndefined();
  });

  it("shows a badge with the rate in the title when sample rate differs", () => {
    const data = dataForFormats(
      { sample_rate: 44100, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(data?.formatMismatch).toBe(true);
    expect(data?.formatMismatchTitle).toContain("44100 Hz → 48000 Hz");
  });

  it("shows a badge when channel count differs", () => {
    const data = dataForFormats({ channels: 1 }, { channels: 2 });
    expect(data?.formatMismatch).toBe(true);
  });

  it("has no badge when either side's format is unknown (not a false positive)", () => {
    const data = dataForFormats({ sample_rate: 44100 }, {});
    expect(data?.formatMismatch).toBeUndefined();
  });
});

describe("Bluetooth device icon parity (#226)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  function dataForDevice(overrides: Partial<Parameters<typeof makeDevice>[0]>) {
    const device = makeDevice(overrides);
    const graph = makeGraph([device], []);
    const node = buildRoutingGraph(graph).nodes.find((n) => n.id === deviceNodeId(device.id));
    return node?.data as RoutingGraphNodeData | undefined;
  }

  it("sets iconOverride to bluetooth for a bluez-named output device, without changing nodeClass", () => {
    const data = dataForDevice({ system_name: "bluez_output.aa_bb_cc.1", direction: "output", kind: "physical" });
    expect(data?.nodeClass).toBe("output");
    expect(data?.iconOverride).toBe("bluetooth");
  });

  it("sets iconOverride to bluetooth for a bluez-named input device, without changing nodeClass", () => {
    const data = dataForDevice({ system_name: "bluez_input.aa_bb_cc.1", direction: "input", kind: "physical" });
    expect(data?.nodeClass).toBe("input");
    expect(data?.iconOverride).toBe("bluetooth");
  });

  it("leaves iconOverride unset for a non-Bluetooth physical output", () => {
    const data = dataForDevice({ system_name: "physical-out-1", direction: "output", kind: "physical" });
    expect(data?.nodeClass).toBe("output");
    expect(data?.iconOverride).toBeUndefined();
  });
});

describe("delay processing node (#313)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Delay subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-delay-1",
      label: "Echo",
      kind: { kind: "delay", delay_ms: 350, feedback_percent: 40, feedforward_percent: -10 },
      system_name: "pipe-deck-proc-delay-echo",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-delay-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Delay");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "delay", delay_ms: 350, feedback_percent: 40, feedforward_percent: -10 });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("limiter processing node (#311)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Limiter subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-limiter-1",
      label: "Safety Limiter",
      kind: { kind: "limiter", ceiling_db: -6, floor_db: -12, symmetric: false },
      system_name: "pipe-deck-proc-limiter-safety-limiter",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-limiter-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Limiter");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "limiter", ceiling_db: -6, floor_db: -12, symmetric: false });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("hpf processing node (#312)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the High-Pass Filter subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-hpf-1",
      label: "Rumble Filter",
      kind: { kind: "hpf", freq_hz: 150, resonance_x10: 12 },
      system_name: "pipe-deck-proc-hpf-rumble-filter",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-hpf-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("High-Pass Filter");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "hpf", freq_hz: 150, resonance_x10: 12 });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("reverb processing node (#327)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Reverb subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-reverb-1",
      label: "Room Verb",
      kind: { kind: "reverb", mix_percent: 35 },
      system_name: "pipe-deck-proc-reverb-room-verb",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-reverb-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Reverb");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "reverb", mix_percent: 35 });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("widener processing node (#314)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Stereo Widener subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-widener-1",
      label: "Wide Stereo",
      kind: { kind: "widener", width_percent: 150 },
      system_name: "pipe-deck-proc-widener-wide-stereo",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-widener-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Stereo Widener");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "widener", width_percent: 150 });
    expect(data?.supportsEffects).toBe(false);
  });
});

describe("pan processing node (#16)", () => {
  beforeEach(() => {
    stubLocalStorage();
  });

  it("builds a node with the Balance/Pan subtitle and processing-node fields", () => {
    const node = makeProcessingNode({
      id: "proc-pan-1",
      label: "Mic Balance",
      kind: { kind: "pan", balance_percent: 40 },
      system_name: "pipe-deck-proc-pan-mic-balance",
    });
    const graph = makeGraph([], [], [], [node]);
    const built = buildRoutingGraph(graph).nodes.find((n) => n.id === processingNodeNodeId("proc-pan-1"));
    const data = built?.data as RoutingGraphNodeData | undefined;

    expect(data?.subtitle).toBe("Balance/Pan");
    expect(data?.nodeKind).toBe("processingNode");
    expect(data?.processingNodeKind).toEqual({ kind: "pan", balance_percent: 40 });
    expect(data?.supportsEffects).toBe(false);
  });
});
