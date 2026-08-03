import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeProcessingNode, makeStream } from "../../test/graphFixtures";
import {
  computeDeviceConnections,
  graphEntityExists,
  handlesForDevice,
  handlesForProcessingNode,
  handlesForStream,
} from "./nodePorts";

describe("computeDeviceConnections", () => {
  it("tracks a playback stream as an input connection on its target", () => {
    const stream = makeStream({ id: "s1", direction: "playback", current_target: "d1" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);

    const connections = computeDeviceConnections(graph);

    expect(connections.get("d1")).toEqual({ in: ["s1"], out: [] });
  });

  it("tracks a capture stream as an output connection on its source", () => {
    const stream = makeStream({ id: "s1", direction: "capture", current_target: "mic1" });
    const mic = makeDevice({ id: "mic1", direction: "input" });
    const graph = makeGraph([mic], [stream]);

    const connections = computeDeviceConnections(graph);

    expect(connections.get("mic1")).toEqual({ in: [], out: ["s1"] });
  });

  it("tracks fan-out device targets on both ends", () => {
    const sink = makeDevice({
      id: "sink1",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1", "out2"],
    });
    const out1 = makeDevice({ id: "out1", direction: "output" });
    const out2 = makeDevice({ id: "out2", direction: "output" });
    const graph = makeGraph([sink, out1, out2]);

    const connections = computeDeviceConnections(graph);

    expect(connections.get("sink1")?.out.sort()).toEqual(["out1", "out2"]);
    expect(connections.get("out1")?.in).toEqual(["sink1"]);
    expect(connections.get("out2")?.in).toEqual(["sink1"]);
  });

  it("tracks mix_sources on both the mic and the mix target", () => {
    const physMic = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    const graph = makeGraph([physMic, virtualMic]);

    const connections = computeDeviceConnections(graph);

    expect(connections.get("mic2")?.in).toEqual(["mic1"]);
    expect(connections.get("mic1")?.out).toEqual(["mic2"]);
  });

  it("tracks a processing node's ports on the devices at either end", () => {
    const source = makeDevice({ id: "src1", kind: "virtual", direction: "output" });
    const target = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const node = makeProcessingNode({
      id: "proc1",
      inputs: [{ index: 0, connected_id: "src1" }],
      outputs: [{ index: 0, connected_id: "out1" }],
    });
    const graph = makeGraph([source, target], [], [], [node]);

    const connections = computeDeviceConnections(graph);

    expect(connections.get("src1")?.out).toEqual(["proc1"]);
    expect(connections.get("out1")?.in).toEqual(["proc1"]);
  });
});

describe("handlesForStream", () => {
  it("gives a playback stream a single source handle", () => {
    const stream = makeStream({ direction: "playback", current_target: "d1" });
    expect(handlesForStream(stream)).toEqual([
      { id: "audio-out", type: "source", position: "right", portType: "audio-out", connectedId: "d1" },
    ]);
  });

  it("gives a capture stream a single target handle", () => {
    const stream = makeStream({ direction: "capture", current_target: "mic1" });
    expect(handlesForStream(stream)).toEqual([
      { id: "audio-in", type: "target", position: "left", portType: "audio-in", connectedId: "mic1" },
    ]);
  });
});

describe("handlesForDevice", () => {
  it("gives a physical output device one input handle plus a trailing empty slot", () => {
    const device = makeDevice({ id: "d1", kind: "physical", direction: "output" });
    const handles = handlesForDevice(device, { in: ["s1"], out: [] });

    expect(handles).toEqual([
      { id: "audio-in:s1", type: "target", position: "left", portType: "audio-in", connectedId: "s1" },
      { id: "audio-in:empty", type: "target", position: "left", portType: "audio-in", empty: true },
    ]);
  });

  it("gives an unconnected virtual output device zero output handles — #293, no fresh forward-route slot", () => {
    const device = makeDevice({ id: "term1", kind: "virtual", direction: "output" });
    const handles = handlesForDevice(device, { in: ["s1"], out: [] });

    expect(handles.some((h) => h.portType === "audio-out")).toBe(false);
    expect(handles.some((h) => h.portType === "audio-in")).toBe(true);
  });

  it("gives a virtual output device's existing monitor connection a real but non-connectable anchor, with no extra empty slot (#388/#391)", () => {
    const device = makeDevice({ id: "sink1", kind: "virtual", direction: "output" });
    const handles = handlesForDevice(device, { in: [], out: ["headphones"] });
    const outHandles = handles.filter((h) => h.portType === "audio-out");

    // A real anchor for the existing monitor connection so its edge has
    // somewhere real to land — but non-connectable (UI_Spec.md: "the
    // frontend never rendering the handle" means never a *draggable* one,
    // not no anchor at all), and never a fresh empty slot, since #293/PD-033
    // still means no *new* arbitrary forward-route from a plain device.
    expect(outHandles).toEqual([
      {
        id: "audio-out:headphones",
        type: "source",
        position: "right",
        portType: "audio-out",
        connectedId: "headphones",
        connectable: false,
      },
    ]);
  });

  it("gives every one of a virtual output device's existing multi-target monitor connections a non-connectable anchor, with no extra empty slot (#388/#391)", () => {
    const device = makeDevice({ id: "sink1", kind: "virtual", direction: "output" });
    const handles = handlesForDevice(device, { in: [], out: ["headphones", "stream-output"] });
    const outHandles = handles.filter((h) => h.portType === "audio-out");

    expect(outHandles.map((h) => h.id)).toEqual(["audio-out:headphones", "audio-out:stream-output"]);
    expect(outHandles.every((h) => h.connectable === false)).toBe(true);
  });

  it("caps a non-multi-capable side at a single filled handle with no trailing empty slot", () => {
    const device = makeDevice({ id: "d1", kind: "physical", direction: "input" });
    const handles = handlesForDevice(device, { in: [], out: ["s1"] });

    expect(handles).toEqual([
      { id: "audio-out:s1", type: "source", position: "right", portType: "audio-out", connectedId: "s1" },
    ]);
  });

  it("returns no handles for a device outside any known column", () => {
    const device = makeDevice({ id: "feed1", system_name: "pipe-deck-feed-1", direction: "output" });
    expect(handlesForDevice(device)).toEqual([]);
  });
});

describe("handlesForProcessingNode", () => {
  it("gives an unconnected fan-out node one empty input slot and one empty output slot", () => {
    const node = makeProcessingNode();
    const handles = handlesForProcessingNode(node);
    expect(handles).toEqual([
      { id: "audio-in:empty", type: "target", position: "left", portType: "audio-in", empty: true },
      { id: "audio-out:empty", type: "source", position: "right", portType: "audio-out", empty: true },
    ]);
  });

  it("gives a connected fan-out node one handle per output plus a trailing empty slot", () => {
    const node = makeProcessingNode({
      inputs: [{ index: 0, connected_id: "src1" }],
      outputs: [
        { index: 0, connected_id: "out1" },
        { index: 1, connected_id: "out2" },
      ],
    });
    const handles = handlesForProcessingNode(node);
    const outHandles = handles.filter((h) => h.portType === "audio-out");
    expect(outHandles.map((h) => h.id)).toEqual(["audio-out:out1", "audio-out:out2", "audio-out:empty"]);
    expect(handles.some((h) => h.id === "audio-in:src1")).toBe(true);
  });

  it("shows a real handle per connection on a non-growable side even if somehow multiply connected, but never adds an empty slot", () => {
    const node = makeProcessingNode({
      inputs: [
        { index: 0, connected_id: "src1" },
        { index: 1, connected_id: "src2" },
      ],
    });
    const inHandles = handlesForProcessingNode(node).filter((h) => h.portType === "audio-in");
    // Every existing connection gets a real handle to anchor to — silently
    // dropping the second one used to leave its edge anchored nowhere real
    // (issue #388). The side still never grows a fresh *empty* slot beyond
    // what's already connected, so this can't be used to add a third.
    expect(inHandles.map((h) => h.id)).toEqual(["audio-in:src1", "audio-in:src2"]);
  });

  it("grows a mixer node's inputs but caps its output at one slot", () => {
    const node = makeProcessingNode({
      kind: { kind: "mixer", input_gains_percent: [100, 100] },
      inputs: [
        { index: 0, connected_id: "src1" },
        { index: 1, connected_id: "src2" },
      ],
      outputs: [{ index: 0, connected_id: "out1" }],
    });
    const handles = handlesForProcessingNode(node);
    const inHandles = handles.filter((h) => h.portType === "audio-in");
    const outHandles = handles.filter((h) => h.portType === "audio-out");
    expect(inHandles.map((h) => h.id)).toEqual(["audio-in:src1", "audio-in:src2", "audio-in:empty"]);
    expect(outHandles).toHaveLength(1);
    expect(outHandles[0].id).toBe("audio-out:out1");
  });

  it("caps both sides of an EQ node at one slot", () => {
    const node = makeProcessingNode({
      kind: { kind: "eq5band", eq_sub: 0, eq_bass: 0, eq_mid: 0, eq_treble: 0, eq_air: 0, output_gain: 0 },
      inputs: [{ index: 0, connected_id: "src1" }],
      outputs: [{ index: 0, connected_id: "out1" }],
    });
    const handles = handlesForProcessingNode(node);
    expect(handles.filter((h) => h.portType === "audio-in")).toHaveLength(1);
    expect(handles.filter((h) => h.portType === "audio-out")).toHaveLength(1);
  });
});

describe("graphEntityExists", () => {
  it("recognizes a processing node id when the processing-nodes list is passed", () => {
    const node = makeProcessingNode({ id: "proc-1" });
    expect(graphEntityExists([], [], "proc-1", [node])).toBe(true);
    expect(graphEntityExists([], [], "proc-1")).toBe(false);
  });
});
