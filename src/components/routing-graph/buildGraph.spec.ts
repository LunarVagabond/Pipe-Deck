import { beforeEach, describe, expect, it, vi } from "vitest";
import { makeGraph, makeStream } from "../../test/graphFixtures";
import { buildRoutingGraph, streamNodeId } from "./buildGraph";
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
