import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";
import { computeDeviceConnections, handlesForDevice, handlesForStream } from "./nodePorts";

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

  it("gives a multi-sink virtual output one handle per target plus a trailing empty slot", () => {
    const device = makeDevice({ id: "sink1", kind: "virtual", direction: "output" });
    const handles = handlesForDevice(device, { in: [], out: ["out1", "out2"] });
    const outHandles = handles.filter((h) => h.portType === "audio-out");

    expect(outHandles.map((h) => h.id)).toEqual(["audio-out:out1", "audio-out:out2", "audio-out:empty"]);
  });

  it("gives a terminal Output (virtual) device zero output handles — #287, it's a true dead end", () => {
    const device = makeDevice({ id: "term1", kind: "virtual", direction: "output", virtual_role: "output" });
    const handles = handlesForDevice(device, { in: ["s1"], out: [] });

    expect(handles.some((h) => h.portType === "audio-out")).toBe(false);
    expect(handles.some((h) => h.portType === "audio-in")).toBe(true);
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
