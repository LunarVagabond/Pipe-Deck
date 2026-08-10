import { describe, expect, it } from "vitest";
import { findNodePath } from "./latencyPath";
import type { BuiltRoutingGraph } from "./buildGraph";

function edge(
  source: string,
  target: string,
): BuiltRoutingGraph["edges"][number] {
  return { id: `${source}->${target}`, source, target };
}

describe("findNodePath", () => {
  it("finds the ordered path through a multi-hop chain, including a processing node", () => {
    // stream:source -> processingNode:eq -> device:output
    const edges = [
      edge("stream:source", "processingNode:eq"),
      edge("processingNode:eq", "device:output"),
    ];

    const path = findNodePath("stream:source", "device:output", edges);
    expect(path).toEqual([
      "stream:source",
      "processingNode:eq",
      "device:output",
    ]);
  });

  it("only follows edges in their drawn direction", () => {
    const edges = [edge("device:a", "device:b")];

    expect(findNodePath("device:a", "device:b", edges)).toEqual([
      "device:a",
      "device:b",
    ]);
    expect(findNodePath("device:b", "device:a", edges)).toBeNull();
  });

  it("returns null when the target is unreachable", () => {
    const edges = [edge("device:a", "device:b"), edge("device:c", "device:d")];

    expect(findNodePath("device:a", "device:d", edges)).toBeNull();
  });

  it("returns a single-element path when source and target are the same node", () => {
    expect(findNodePath("device:a", "device:a", [])).toEqual(["device:a"]);
  });

  it("picks the shortest path when multiple routes exist", () => {
    const edges = [
      edge("device:a", "device:b"),
      edge("device:b", "device:d"),
      edge("device:a", "device:c"),
      edge("device:c", "device:e"),
      edge("device:e", "device:d"),
    ];

    expect(findNodePath("device:a", "device:d", edges)).toEqual([
      "device:a",
      "device:b",
      "device:d",
    ]);
  });
});
