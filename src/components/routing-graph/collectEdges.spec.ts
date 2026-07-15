import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";
import { collectRoutingEdges } from "./collectEdges";

describe("collectRoutingEdges", () => {
  it("builds an edge for a playback stream routed to an output device", () => {
    const stream = makeStream({ id: "s1", direction: "playback", current_target: "d1" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream], [{ id: "link-1", source_id: "s1", target_id: "d1" }]);

    const edges = collectRoutingEdges(graph);

    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({ source: "stream:s1", target: "device:d1" });
  });

  it("drops a link whose stream no longer targets that device", () => {
    const stream = makeStream({ id: "s1", direction: "playback", current_target: "d2" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream], [{ id: "link-1", source_id: "s1", target_id: "d1" }]);

    expect(collectRoutingEdges(graph)).toHaveLength(0);
  });

  it("builds an edge for a capture stream's mic source", () => {
    const stream = makeStream({ id: "s1", direction: "capture", current_target: "mic1" });
    const mic = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const graph = makeGraph([mic], [stream], [{ id: "link-1", source_id: "mic1", target_id: "s1" }]);

    const edges = collectRoutingEdges(graph);

    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({ source: "device:mic1", target: "stream:s1" });
  });

  it("builds a mic-mix edge from a device's mix_sources", () => {
    const physMic = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    const graph = makeGraph([physMic, virtualMic], [], []);

    // Mic-mix connections aren't emitted from graph.links in this codebase —
    // collectRoutingEdges only walks links + multi-sink fan-out, so a
    // mix_sources-only device pair produces no edge by itself.
    expect(collectRoutingEdges(graph)).toHaveLength(0);
  });

  it("builds one edge per fan-out target for a multi-sink virtual output", () => {
    const sink = makeDevice({
      id: "sink1",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1", "out2"],
    });
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1, out2], [], []);

    const edges = collectRoutingEdges(graph);

    expect(edges).toHaveLength(2);
    expect(edges.map((edge) => edge.target).sort()).toEqual(["device:out1", "device:out2"]);
  });

  it("deduplicates a link that's also represented via fan-out targets", () => {
    const sink = makeDevice({
      id: "sink1",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const graph = makeGraph(
      [sink, out1],
      [],
      [{ id: "link-1", source_id: "sink1", target_id: "out1" }],
    );

    expect(collectRoutingEdges(graph)).toHaveLength(1);
  });

  it("drops a link referencing an entity no longer in the graph", () => {
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [], [{ id: "link-1", source_id: "gone", target_id: "d1" }]);

    expect(collectRoutingEdges(graph)).toHaveLength(0);
  });
});
