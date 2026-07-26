import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";
import { isMicPassthroughCandidate } from "./routingRelationship";
import { resolveConnectionAction, type PreviousEdge } from "./connectionRules";

describe("isMicPassthroughCandidate", () => {
  it("is true for a playback stream dropped onto a virtual input", () => {
    const stream = makeStream({ direction: "playback" });
    const device = makeDevice({ kind: "virtual", direction: "input" });
    expect(isMicPassthroughCandidate(stream, device)).toBe(true);
  });

  it("is false for a capture stream", () => {
    const stream = makeStream({ direction: "capture" });
    const device = makeDevice({ kind: "virtual", direction: "input" });
    expect(isMicPassthroughCandidate(stream, device)).toBe(false);
  });

  it("is false when the target isn't a virtual input", () => {
    const stream = makeStream({ direction: "playback" });
    const device = makeDevice({ kind: "physical", direction: "output" });
    expect(isMicPassthroughCandidate(stream, device)).toBe(false);
  });
});

describe("device-to-device routing (retired, #293)", () => {
  it("rejects dragging a virtual output device directly onto another device", () => {
    const source = makeDevice({
      id: "vout1",
      label: "Virtual Output",
      kind: "virtual",
      direction: "output",
    });
    const target = makeDevice({ id: "speakers", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([source, target], []);

    // A virtual output device has no output handle at all now (nodePorts.ts),
    // so a real drag could never produce this connection in the first place
    // — this exercises resolveConnectionAction's own fallback rejection
    // directly, bypassing the handle-presence check the UI would otherwise
    // enforce first.
    const connectResult = resolveConnectionAction(graph, {
      source: "device:vout1",
      target: "device:speakers",
      sourceHandle: "audio-out",
      targetHandle: "audio-in:empty",
    } as never);
    expect(connectResult).toEqual({
      error:
        "\"Virtual Output\" can't be routed directly to \"Speakers\" — plain devices no longer route onward to each other. Use a Mixer or Fan-Out node, or drag an application stream instead.",
    });
  });

  it("has nothing to disconnect for a device-to-device edge, since none can exist", () => {
    const source = makeDevice({ id: "vout1", label: "Virtual Output", kind: "virtual", direction: "output" });
    const target = makeDevice({ id: "speakers", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([source, target], []);

    const previousEdge: PreviousEdge = { source: "device:vout1", target: "device:speakers" };
    const disconnectResult = resolveConnectionAction(
      graph,
      { source: null, target: null, sourceHandle: null, targetHandle: null } as never,
      { mode: "edge_disconnect", previousEdge },
    );
    expect(disconnectResult).toEqual({ error: "Nothing to disconnect." });
  });
});
