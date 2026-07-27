import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeGraph, makeProcessingNode, makeStream } from "../../test/graphFixtures";
import { buildRoutingGraph, processingNodeNodeId, streamNodeId } from "./buildGraph";
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
