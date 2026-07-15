import { describe, expect, it } from "vitest";
import { deviceNodeId, parseGraphNodeId, streamNodeId } from "./nodeIds";

describe("nodeIds", () => {
  it("round-trips a stream id", () => {
    const id = streamNodeId("s1");
    expect(id).toBe("stream:s1");
    expect(parseGraphNodeId(id)).toEqual({ kind: "stream", id: "s1" });
  });

  it("round-trips a device id", () => {
    const id = deviceNodeId("d1");
    expect(id).toBe("device:d1");
    expect(parseGraphNodeId(id)).toEqual({ kind: "device", id: "d1" });
  });

  it("preserves colons within the underlying id", () => {
    const id = deviceNodeId("pactl:sink:5");
    expect(parseGraphNodeId(id)).toEqual({ kind: "device", id: "pactl:sink:5" });
  });

  it("returns null for an unknown kind prefix", () => {
    expect(parseGraphNodeId("widget:foo")).toBeNull();
  });

  it("returns null when there is no id segment", () => {
    expect(parseGraphNodeId("stream")).toBeNull();
  });
});
