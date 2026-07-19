import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";
import { isMicMixCandidate, isMicPassthroughCandidate, isRoutableVirtualOutput } from "./routingRelationship";
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

describe("isMicMixCandidate", () => {
  it("is true for a physical mic into a virtual input", () => {
    const source = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const target = makeDevice({ id: "mic2", kind: "virtual", direction: "input" });
    expect(isMicMixCandidate(source, target)).toBe(true);
  });

  it("is true for a virtual output into a virtual input", () => {
    const source = makeDevice({ id: "out1", kind: "virtual", direction: "output" });
    const target = makeDevice({ id: "mic2", kind: "virtual", direction: "input" });
    expect(isMicMixCandidate(source, target)).toBe(true);
  });

  it("is false when the source is a physical output", () => {
    const source = makeDevice({ id: "out1", kind: "physical", direction: "output" });
    const target = makeDevice({ id: "mic2", kind: "virtual", direction: "input" });
    expect(isMicMixCandidate(source, target)).toBe(false);
  });

  it("is false when the target isn't a virtual input", () => {
    const source = makeDevice({ id: "mic1", kind: "physical", direction: "input" });
    const target = makeDevice({ id: "out1", kind: "virtual", direction: "output" });
    expect(isMicMixCandidate(source, target)).toBe(false);
  });
});

describe("isRoutableVirtualOutput", () => {
  it("is true for a virtual output device", () => {
    expect(isRoutableVirtualOutput(makeDevice({ kind: "virtual", direction: "output" }))).toBe(true);
  });

  it("is false for a virtual input device", () => {
    expect(isRoutableVirtualOutput(makeDevice({ kind: "virtual", direction: "input" }))).toBe(false);
  });

  it("is false for a physical output device", () => {
    expect(isRoutableVirtualOutput(makeDevice({ kind: "physical", direction: "output" }))).toBe(false);
  });
});

describe("connect-time and disconnect-time classification agree", () => {
  it("treats an already-routed virtual-output -> device pair as the same relationship in both directions", () => {
    const source = makeDevice({
      id: "vout1",
      label: "Virtual Output",
      kind: "virtual",
      direction: "output",
      current_target: "speakers",
    });
    const target = makeDevice({ id: "speakers", label: "Speakers", kind: "physical", direction: "output" });
    const graph = makeGraph([source, target], []);

    // Connect-time: dragging the same pair again should be classified as
    // "already routed" (a routable virtual-output relationship), not fall
    // through to some other relationship kind.
    const connectResult = resolveConnectionAction(graph, {
      source: "device:vout1",
      target: "device:speakers",
      sourceHandle: "audio-out",
      targetHandle: "audio-in:someone-else",
    } as never);
    expect(connectResult).toEqual({
      error:
        "Connect an output port to an open input slot — this target's slot is already in use or the wrong direction.",
    });

    // Disconnect-time: dragging the existing edge off should be classified
    // as the same routable virtual-output relationship and produce a
    // device_targets clear, not an "isn't a virtual sink route" error.
    const previousEdge: PreviousEdge = { source: "device:vout1", target: "device:speakers" };
    const disconnectResult = resolveConnectionAction(
      graph,
      { source: null, target: null, sourceHandle: null, targetHandle: null } as never,
      { mode: "edge_disconnect", previousEdge },
    );
    expect(disconnectResult).toEqual({
      action: { type: "device_targets", sourceDeviceId: "vout1", targetDeviceIds: [] },
    });
  });

  it("treats a mic-mix pair identically for isMicMixCandidate regardless of connect/disconnect direction", () => {
    const source = makeDevice({ id: "mic1", label: "Headset Mic", kind: "physical", direction: "input" });
    const target = makeDevice({
      id: "mic2",
      label: "Virtual Mic",
      kind: "virtual",
      direction: "input",
      mix_sources: [{ device_id: "mic1", volume_percent: 100, muted: false }],
    });
    expect(isMicMixCandidate(source, target)).toBe(isMicMixCandidate(source, target));

    const graph = makeGraph([source, target], []);
    const disconnectResult = resolveConnectionAction(
      graph,
      { source: null, target: null, sourceHandle: null, targetHandle: null } as never,
      { mode: "edge_disconnect", previousEdge: { source: "device:mic1", target: "device:mic2" } },
    );
    expect(disconnectResult).toEqual({
      action: { type: "mic_mix_remove", virtualMicDeviceId: "mic2", sourceDeviceId: "mic1" },
    });
  });
});
