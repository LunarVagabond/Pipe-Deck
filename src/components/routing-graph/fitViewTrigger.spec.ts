import { describe, expect, it } from "vitest";
import { detectNewlyAddedNodes } from "./fitViewTrigger";

describe("detectNewlyAddedNodes", () => {
  it("reports no additions on the very first call and seeds known state", () => {
    const result = detectNewlyAddedNodes([{ id: "a" }, { id: "b" }], null, null);
    expect(result.addedIds).toEqual([]);
    expect(result.nodeIds).toEqual(new Set(["a", "b"]));
  });

  it("reports a genuinely new node id", () => {
    const result = detectNewlyAddedNodes(
      [{ id: "a" }, { id: "b" }],
      new Set(["a"]),
      new Set(),
    );
    expect(result.addedIds).toEqual(["b"]);
  });

  it("does not report a node whose id changed but whose identity key was already known", () => {
    const result = detectNewlyAddedNodes(
      [{ id: "a" }, { id: "b-new-node-id", identityKey: "discord" }],
      new Set(["a", "b-old-node-id"]),
      new Set(["discord"]),
    );
    expect(result.addedIds).toEqual([]);
  });

  it("reports a node with a new id and a genuinely new identity key", () => {
    const result = detectNewlyAddedNodes(
      [{ id: "a" }, { id: "c", identityKey: "firefox" }],
      new Set(["a"]),
      new Set(["discord"]),
    );
    expect(result.addedIds).toEqual(["c"]);
  });

  it("reports a node with no identity key (e.g. a device) purely by id", () => {
    const result = detectNewlyAddedNodes([{ id: "a" }, { id: "device-1" }], new Set(["a"]), new Set());
    expect(result.addedIds).toEqual(["device-1"]);
  });

  it("returns the current id/identity-key sets for the caller to persist as the next known state", () => {
    const result = detectNewlyAddedNodes(
      [{ id: "a" }, { id: "b", identityKey: "discord" }],
      new Set(["a"]),
      new Set(),
    );
    expect(result.nodeIds).toEqual(new Set(["a", "b"]));
    expect(result.identityKeys).toEqual(new Set(["discord"]));
  });
});
