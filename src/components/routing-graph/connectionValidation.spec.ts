import { describe, expect, it } from "vitest";
import { resolveConnectionValidity } from "./connectionValidation";
import type { PendingEdgeUpdate } from "./connectionValidation";
import type { Connection } from "@vue-flow/core";

function makeConnection(
  overrides: Partial<Connection> & { id?: string } = {},
): Connection {
  return {
    source: "a",
    target: "b",
    sourceHandle: "audio-out",
    targetHandle: "audio-in:empty",
    ...overrides,
  } as Connection;
}

describe("resolveConnectionValidity", () => {
  it("requires an empty slot for a fresh connect drag with no pending edge update", () => {
    const connection = makeConnection({ targetHandle: "audio-in:occupied-id" });
    expect(resolveConnectionValidity(connection, null)).toBe(false);
  });

  it("allows a fresh connect drag onto a genuinely empty slot", () => {
    const connection = makeConnection({ targetHandle: "audio-in:empty" });
    expect(resolveConnectionValidity(connection, null)).toBe(true);
  });

  it("does not require an empty slot when re-validating an already-persisted edge (carries an id)", () => {
    const connection = makeConnection({
      id: "edge-1",
      targetHandle: "audio-in:occupied-id",
    });
    expect(resolveConnectionValidity(connection, null)).toBe(true);
  });

  it("allows the unmoved end of an edge-update drag through even though it's still occupied", () => {
    const pendingEdge = {
      sourceHandle: "audio-out",
      targetHandle: "audio-in:occupied-id",
    } as PendingEdgeUpdate;
    const connection = makeConnection({ targetHandle: "audio-in:occupied-id" });
    expect(resolveConnectionValidity(connection, pendingEdge)).toBe(true);
  });

  it("still rejects a handle unrelated to the pending edge update", () => {
    const pendingEdge = {
      sourceHandle: "audio-out",
      targetHandle: "audio-in:some-other-occupied-id",
    } as PendingEdgeUpdate;
    const connection = makeConnection({
      targetHandle: "audio-in:different-occupied-id",
    });
    expect(resolveConnectionValidity(connection, pendingEdge)).toBe(false);
  });

  it("rejects mismatched port types regardless of edge state", () => {
    const connection = makeConnection({
      sourceHandle: "audio-in",
      targetHandle: "audio-in:empty",
    });
    expect(resolveConnectionValidity(connection, null)).toBe(false);
  });
});
