import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeProcessingNode, makeStream } from "../../test/graphFixtures";
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

  it("draws no edge from a device's legacy mix_sources — retired in favor of the Mixer Node", () => {
    const physMic = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    const graph = makeGraph([physMic, virtualMic], [], []);

    expect(collectRoutingEdges(graph)).toHaveLength(0);
  });

  it("builds one edge per live fan-out target via graph.links, even for multiple targets from one source", () => {
    // #293: multi-target fan-out edges are no longer synthesized from
    // device.current_targets (that authoring mechanism is retired) — the
    // live backend already emits one `pwlink-*` link per fan-out target
    // directly (`graph_routing.rs::apply_pw_link_device_routes`), so
    // rendering just needs to draw whatever's in graph.links.
    const sink = makeDevice({ id: "sink1", kind: "virtual", direction: "output" });
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", kind: "physical", direction: "output" });
    const graph = makeGraph(
      [sink, out1, out2],
      [],
      [
        { id: "pwlink-sink1-out1", source_id: "sink1", target_id: "out1" },
        { id: "pwlink-sink1-out2", source_id: "sink1", target_id: "out2" },
      ],
    );

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

  it("builds an edge for each connected port on a processing node — PD-032's 4th edge shape", () => {
    const source = makeDevice({ id: "src1", kind: "virtual", direction: "output" });
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", kind: "physical", direction: "output" });
    const node = makeProcessingNode({
      id: "proc-1",
      inputs: [{ index: 0, connected_id: "src1" }],
      outputs: [
        { index: 0, connected_id: "out1" },
        { index: 1, connected_id: "out2" },
      ],
    });
    const graph = makeGraph([source, out1, out2], [], [], [node]);

    const edges = collectRoutingEdges(graph);

    expect(edges).toHaveLength(3);
    expect(edges).toContainEqual(expect.objectContaining({ source: "device:src1", target: "processingNode:proc-1" }));
    expect(edges).toContainEqual(expect.objectContaining({ source: "processingNode:proc-1", target: "device:out1" }));
    expect(edges).toContainEqual(expect.objectContaining({ source: "processingNode:proc-1", target: "device:out2" }));
  });

  it("drops a processing-node edge whose port peer no longer exists in the graph", () => {
    const node = makeProcessingNode({ id: "proc-1", outputs: [{ index: 0, connected_id: "gone" }] });
    const graph = makeGraph([], [], [], [node]);

    expect(collectRoutingEdges(graph)).toHaveLength(0);
  });

  it("never draws a Group node's edges to its own members, but keeps its own input edge — issue #80", () => {
    const source = makeDevice({ id: "src1", kind: "virtual", direction: "output" });
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", kind: "physical", direction: "output" });
    const node = makeProcessingNode({
      id: "group-1",
      kind: { kind: "group", volume_percent: 100, muted: false },
      inputs: [{ index: 0, connected_id: "src1" }],
      outputs: [
        { index: 0, connected_id: "out1" },
        { index: 1, connected_id: "out2" },
      ],
    });
    const graph = makeGraph([source, out1, out2], [], [], [node]);

    // A Group behaves like a terminal/leaf node — its members render as an
    // inline name list on the node itself (RoutingGraphNodeGroup.vue), never
    // as graph edges to the real device nodes elsewhere on the canvas.
    const edges = collectRoutingEdges(graph);
    expect(edges).toHaveLength(1);
    expect(edges).toContainEqual(expect.objectContaining({ source: "device:src1", target: "processingNode:group-1" }));
  });

  it("still draws a Fan-out node's member edges — unlike Group, Fan-out keeps its real wiring visible", () => {
    const out1 = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const node = makeProcessingNode({
      id: "fan-1",
      kind: { kind: "fan_out", volume_percent: 100, muted: false },
      outputs: [{ index: 0, connected_id: "out1" }],
    });
    const graph = makeGraph([out1], [], [], [node]);

    const edges = collectRoutingEdges(graph);
    expect(edges).toHaveLength(1);
    expect(edges).toContainEqual(expect.objectContaining({ source: "processingNode:fan-1", target: "device:out1" }));
  });
});
