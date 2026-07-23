import { describe, expect, it } from "vitest";
import type { Connection } from "@vue-flow/core";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";
import { resolveConnectionAction } from "./connectionRules";

function connection(overrides: Partial<Connection>): Connection {
  return {
    source: null,
    target: null,
    sourceHandle: null,
    targetHandle: null,
    ...overrides,
  } as Connection;
}

describe("resolveConnectionAction — connect mode", () => {
  it("errors when source or target is missing", () => {
    const graph = makeGraph();
    const result = resolveConnectionAction(graph, connection({ source: "stream:s1" }));
    expect(result).toEqual({ error: "Drag needs both a source and a target port." });
  });

  it("errors when the target handle is not an open slot", () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:someone-else",
      }),
    );
    expect(result).toEqual({
      error:
        "Connect an output port to an open input slot — this target's slot is already in use or the wrong direction.",
    });
  });

  it("errors when a node id can't be parsed", () => {
    const stream = makeStream({ id: "s1" });
    const graph = makeGraph([], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "widget:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error: "Could not identify one end of this connection — try refreshing the routing view.",
    });
  });

  it("errors when connecting two streams directly", () => {
    const streamA = makeStream({ id: "s1", app_name: "App A" });
    const streamB = makeStream({ id: "s2", app_name: "App B" });
    const graph = makeGraph([], [streamA, streamB]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "stream:s2",
        sourceHandle: "audio-out",
        targetHandle: "audio-in",
      }),
    );
    expect(result).toEqual({
      error: '"App A" and "App B" are both application streams — connect a stream to a device instead.',
    });
  });

  it("resolves a valid playback stream to output device connection", () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({ action: { type: "stream_target", streamId: "s1", targetDeviceId: "d1" } });
  });

  it("resolves the same drag reversed (device onto stream)", () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:d1",
        target: "stream:s1",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in",
      }),
    );
    expect(result).toEqual({ action: { type: "stream_target", streamId: "s1", targetDeviceId: "d1" } });
  });

  it("errors when the target doesn't accept the stream's direction", () => {
    const stream = makeStream({ id: "s1", app_name: "Capture App", direction: "capture" });
    const device = makeDevice({ id: "d1", label: "Speakers", direction: "output" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error:
        '"Capture App" is a capture stream — "Speakers" doesn\'t accept that direction. Pick a capture input instead.',
    });
  });

  it("adds a mic passthrough when a playback stream is dropped on a virtual mic", () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const mic = makeDevice({ id: "mic1", label: "Virtual Mic", kind: "virtual", direction: "input" });
    const graph = makeGraph([mic], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "device:mic1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      action: { type: "stream_mic_passthrough_add", streamId: "s1", micDeviceId: "mic1" },
    });
  });

  it("errors when the stream already passes through to that mic", () => {
    const mic = makeDevice({ id: "mic1", label: "Virtual Mic", kind: "virtual", direction: "input" });
    const stream = makeStream({ id: "s1", direction: "playback", current_target: "mic1" });
    const graph = makeGraph([mic], [stream]);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "stream:s1",
        target: "device:mic1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error: '"Test App" is already sending audio to "Virtual Mic".',
    });
  });

  it("adds a mic-mix source for a physical mic into a virtual input", () => {
    const physMic = makeDevice({ id: "mic1", label: "Headset Mic", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({ id: "mic2", label: "Virtual Mic", kind: "virtual", direction: "input" });
    const graph = makeGraph([physMic, virtualMic], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:mic1",
        target: "device:mic2",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      action: { type: "mic_mix_add", virtualMicDeviceId: "mic2", sourceDeviceId: "mic1" },
    });
  });

  it("errors when the mic is already mixed into the target", () => {
    const physMic = makeDevice({ id: "mic1", label: "Headset Mic", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      label: "Virtual Mic",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    const graph = makeGraph([physMic, virtualMic], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:mic1",
        target: "device:mic2",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error: '"Headset Mic" is already mixed into "Virtual Mic".',
    });
  });

  it("errors when a virtual sink targets something outside its allowed set", () => {
    const sink = makeDevice({ id: "sink1", label: "Virtual Sink", kind: "virtual", direction: "output" });
    const physicalMic = makeDevice({
      id: "mic1",
      label: "Headset Mic",
      kind: "physical",
      direction: "input",
    });
    const graph = makeGraph([sink, physicalMic], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:sink1",
        target: "device:mic1",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error:
        '"Virtual Sink" can only route to a physical output, another virtual output, or a virtual input — "Headset Mic" isn\'t one of those.',
    });
  });

  it("errors when the source isn't a virtual output sink", () => {
    const virtualInput = makeDevice({
      id: "vin1",
      label: "Virtual Input",
      kind: "virtual",
      direction: "input",
    });
    const physicalOut = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([virtualInput, physicalOut], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:vin1",
        target: "device:out1",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      error:
        '"Virtual Input" isn\'t a virtual output sink, so it can\'t be routed directly to another device. Drag an application stream instead.',
    });
  });

  it("errors when a multi-sink output is already connected to that target", () => {
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:sink1",
        target: "device:out1",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({ error: '"Virtual Sink" is already routed to "Speakers".' });
  });

  it("adds a new fan-out target for a multi-sink virtual output", () => {
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out1 = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", label: "Headphones", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1, out2], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:sink1",
        target: "device:out2",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
    );
    expect(result).toEqual({
      action: { type: "device_targets", sourceDeviceId: "sink1", targetDeviceIds: ["out1", "out2"] },
    });
  });

  it("re-targets a multi-sink output during an edge_update drag", () => {
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out1 = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", label: "Headphones", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1, out2], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:sink1",
        target: "device:out2",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
      { mode: "edge_update", previousEdge: { source: "device:sink1", target: "device:out1" } },
    );
    expect(result).toEqual({
      action: { type: "device_targets", sourceDeviceId: "sink1", targetDeviceIds: ["out2"] },
    });
  });

  it("re-targets during an edge_update drag when the unmoved source handle is still occupied", () => {
    // Reflects real drag behavior: only the dragged (target) end lands on an
    // empty slot — the unmoved source end still carries its original,
    // already-connected handle id, not a trailing `:empty` slot.
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out1 = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const out2 = makeDevice({ id: "out2", label: "Headphones", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1, out2], []);
    const result = resolveConnectionAction(
      graph,
      connection({
        source: "device:sink1",
        target: "device:out2",
        sourceHandle: "audio-out:out1",
        targetHandle: "audio-in:empty",
      }),
      {
        mode: "edge_update",
        previousEdge: {
          source: "device:sink1",
          target: "device:out1",
          sourceHandle: "audio-out:out1",
          targetHandle: "audio-in:empty",
        },
      },
    );
    expect(result).toEqual({
      action: { type: "device_targets", sourceDeviceId: "sink1", targetDeviceIds: ["out2"] },
    });
  });
});

describe("resolveConnectionAction — edge_disconnect mode", () => {
  it("errors when there is no previous edge", () => {
    const graph = makeGraph();
    const result = resolveConnectionAction(graph, connection({}), { mode: "edge_disconnect" });
    expect(result).toEqual({ error: "Nothing to disconnect." });
  });

  it("errors when the previous edge's node ids can't be parsed", () => {
    const graph = makeGraph();
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "widget:a", target: "widget:b" },
    });
    expect(result).toEqual({ error: "Unknown node type." });
  });

  it("clears a stream's target when the edge matches the current route", () => {
    const stream = makeStream({ id: "s1", current_target: "d1" });
    const device = makeDevice({ id: "d1" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "stream:s1", target: "device:d1" },
    });
    expect(result).toEqual({
      action: { type: "clear_stream_target", streamId: "s1", previousTargetDeviceId: "d1" },
    });
  });

  it("errors when a stream-device edge no longer matches the live route", () => {
    const stream = makeStream({ id: "s1", app_name: "App A", current_target: "other-device" });
    const device = makeDevice({ id: "d1", label: "Speakers" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "stream:s1", target: "device:d1" },
    });
    expect(result).toEqual({
      error: '"App A" isn\'t currently routed to "Speakers" — nothing to disconnect.',
    });
  });

  it("clears a stream's target for the device-onto-stream edge shape", () => {
    const stream = makeStream({ id: "s1", current_target: "d1" });
    const device = makeDevice({ id: "d1" });
    const graph = makeGraph([device], [stream]);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:d1", target: "stream:s1" },
    });
    expect(result).toEqual({
      action: { type: "clear_stream_target", streamId: "s1", previousTargetDeviceId: "d1" },
    });
  });

  it("has nothing to disconnect for a stream-stream edge shape", () => {
    const streamA = makeStream({ id: "s1" });
    const streamB = makeStream({ id: "s2" });
    const graph = makeGraph([], [streamA, streamB]);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "stream:s1", target: "stream:s2" },
    });
    expect(result).toEqual({ error: "Nothing to disconnect." });
  });

  it("removes a mic-mix source when the edge matches the current mix", () => {
    const physMic = makeDevice({ id: "mic1", label: "Headset Mic", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      label: "Virtual Mic",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    const graph = makeGraph([physMic, virtualMic], []);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:mic1", target: "device:mic2" },
    });
    expect(result).toEqual({
      action: { type: "mic_mix_remove", virtualMicDeviceId: "mic2", sourceDeviceId: "mic1" },
    });
  });

  it("errors when the mic-mix edge no longer matches the current mix", () => {
    const physMic = makeDevice({ id: "mic1", label: "Headset Mic", kind: "physical", direction: "input" });
    const virtualMic = makeDevice({
      id: "mic2",
      label: "Virtual Mic",
      kind: "virtual",
      direction: "input",
      mix_sources: [],
    });
    const graph = makeGraph([physMic, virtualMic], []);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:mic1", target: "device:mic2" },
    });
    expect(result).toEqual({
      error: '"Headset Mic" isn\'t currently mixed into "Virtual Mic" — nothing to disconnect.',
    });
  });

  it("errors when disconnecting a non-virtual-output device route", () => {
    const physOut = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const sink = makeDevice({ id: "sink1", label: "Virtual Sink", kind: "virtual", direction: "output" });
    const graph = makeGraph([physOut, sink], []);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:out1", target: "device:sink1" },
    });
    expect(result).toEqual({
      error:
        '"Speakers" isn\'t a virtual sink route — only virtual-output connections can be dragged off to disconnect them.',
    });
  });

  it("errors when the device-to-device edge no longer matches a live route", () => {
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out2"],
    });
    const out1 = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1], []);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:sink1", target: "device:out1" },
    });
    expect(result).toEqual({
      error: '"Virtual Sink" isn\'t currently routed to "Speakers" — nothing to disconnect.',
    });
  });

  it("removes a fan-out target from a multi-sink virtual output", () => {
    const sink = makeDevice({
      id: "sink1",
      label: "Virtual Sink",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1", "out2"],
    });
    const out1 = makeDevice({ id: "out1", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([sink, out1], []);
    const result = resolveConnectionAction(graph, connection({}), {
      mode: "edge_disconnect",
      previousEdge: { source: "device:sink1", target: "device:out1" },
    });
    expect(result).toEqual({
      action: { type: "device_targets", sourceDeviceId: "sink1", targetDeviceIds: ["out2"] },
    });
  });
});
