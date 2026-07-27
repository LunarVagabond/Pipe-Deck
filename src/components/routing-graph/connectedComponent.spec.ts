import { describe, expect, it } from "vitest";
import { computeConnectedComponent } from "./connectedComponent";
import type { BuiltRoutingGraph } from "./buildGraph";

function edge(source: string, target: string): BuiltRoutingGraph["edges"][number] {
  return { id: `${source}->${target}`, source, target };
}

describe("computeConnectedComponent", () => {
  it("finds nodes reachable both upstream and downstream of the start node", () => {
    // Source1,Source2 -> Mixer -> EQ1 -> EQ2 -> FanOut -> [EQ3, HardOutput]
    const edges = [
      edge("stream:source1", "processingNode:mixer"),
      edge("stream:source2", "processingNode:mixer"),
      edge("processingNode:mixer", "processingNode:eq1"),
      edge("processingNode:eq1", "processingNode:eq2"),
      edge("processingNode:eq2", "processingNode:fanout"),
      edge("processingNode:fanout", "processingNode:eq3"),
      edge("processingNode:fanout", "device:hardOutput"),
    ];

    const fromEq2 = computeConnectedComponent("processingNode:eq2", edges);
    expect(fromEq2).toEqual(
      new Set([
        "processingNode:eq2",
        "processingNode:eq1",
        "processingNode:mixer",
        "stream:source1",
        "stream:source2",
        "processingNode:fanout",
        "processingNode:eq3",
        "device:hardOutput",
      ]),
    );

    // Isolating EQ3 must still reach EQ1 upstream, back through fan-out and EQ2.
    const fromEq3 = computeConnectedComponent("processingNode:eq3", edges);
    expect(fromEq3.has("processingNode:eq1")).toBe(true);
    expect(fromEq3.has("processingNode:eq2")).toBe(true);
  });

  it("does not reach a disconnected chain elsewhere in the graph", () => {
    const edges = [
      edge("processingNode:eq1", "processingNode:eq2"),
      edge("processingNode:eq3", "processingNode:eq4"),
    ];

    const result = computeConnectedComponent("processingNode:eq1", edges);
    expect(result).toEqual(new Set(["processingNode:eq1", "processingNode:eq2"]));
    expect(result.has("processingNode:eq3")).toBe(false);
    expect(result.has("processingNode:eq4")).toBe(false);
  });

  it("returns just the start node when it has no edges", () => {
    const result = computeConnectedComponent("processingNode:lonely", []);
    expect(result).toEqual(new Set(["processingNode:lonely"]));
  });
});
