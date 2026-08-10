import { describe, expect, it } from "vitest";
import { filterRuntimeGraph } from "./filterGraph";
import type { RuntimeGraph, Stream, Link } from "../types/graph";

function stream(overrides: Partial<Stream> = {}): Stream {
  return {
    id: "s1",
    app_name: "app",
    direction: "playback",
    ...overrides,
  };
}

function link(overrides: Partial<Link> = {}): Link {
  return {
    id: "l1",
    source_id: "s1",
    target_id: "d1",
    is_monitor: false,
    ...overrides,
  };
}

function graph(overrides: Partial<RuntimeGraph> = {}): RuntimeGraph {
  return {
    devices: [],
    streams: [],
    links: [],
    ...overrides,
  };
}

describe("filterRuntimeGraph", () => {
  it("returns the graph unchanged when showSystemStreams is true", () => {
    const g = graph({ streams: [stream({ id: "sys", is_system: true })] });
    expect(filterRuntimeGraph(g, true)).toBe(g);
  });

  it("returns the same graph reference when there are no system streams to hide", () => {
    const g = graph({ streams: [stream({ id: "s1" })] });
    expect(filterRuntimeGraph(g, false)).toBe(g);
  });

  it("removes system streams when showSystemStreams is false", () => {
    const g = graph({
      streams: [
        stream({ id: "normal" }),
        stream({ id: "sys", is_system: true }),
      ],
    });
    const result = filterRuntimeGraph(g, false);
    expect(result.streams.map((s) => s.id)).toEqual(["normal"]);
  });

  it("removes links that reference a hidden system stream on either end", () => {
    const g = graph({
      streams: [
        stream({ id: "normal" }),
        stream({ id: "sys", is_system: true }),
      ],
      links: [
        link({ id: "keep", source_id: "normal", target_id: "device" }),
        link({ id: "drop-source", source_id: "sys", target_id: "device" }),
        link({ id: "drop-target", source_id: "device2", target_id: "sys" }),
      ],
    });
    const result = filterRuntimeGraph(g, false);
    expect(result.links.map((l) => l.id)).toEqual(["keep"]);
  });

  it("does not mutate the original graph", () => {
    const original = graph({
      streams: [
        stream({ id: "normal" }),
        stream({ id: "sys", is_system: true }),
      ],
      links: [link({ id: "l1", source_id: "sys", target_id: "device" })],
    });
    const originalStreamCount = original.streams.length;
    const originalLinkCount = original.links.length;
    filterRuntimeGraph(original, false);
    expect(original.streams.length).toBe(originalStreamCount);
    expect(original.links.length).toBe(originalLinkCount);
  });
});
