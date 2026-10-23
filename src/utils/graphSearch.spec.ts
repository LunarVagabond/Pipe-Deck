import { describe, expect, it } from "vitest";
import { makeDevice, makeGraph, makeStream } from "../test/graphFixtures";
import type { Device, Stream } from "../types/graph";
import {
  applyGraphSearch,
  deviceMatchesSearch,
  streamMatchesSearch,
} from "./graphSearch";

describe("graph search", () => {
  it.each<{ field: keyof Device }>([
    { field: "label" },
    { field: "system_name" },
  ])(
    "matches device $field independently, ignoring case and padding",
    ({ field }) => {
      const device = makeDevice({ [field]: "Only NeEdLe here" });
      expect(deviceMatchesSearch(device, "  NEEDLE  ")).toBe(true);
      expect(deviceMatchesSearch(device, "absent")).toBe(false);
    },
  );

  it.each<{ field: keyof Stream }>([
    { field: "app_name" },
    { field: "executable" },
    { field: "media_name" },
    { field: "system_name" },
  ])(
    "matches stream $field independently, ignoring case and padding",
    ({ field }) => {
      const stream = makeStream({ [field]: "Only NeEdLe here" });
      expect(streamMatchesSearch(stream, "  NEEDLE  ")).toBe(true);
      expect(streamMatchesSearch(stream, "absent")).toBe(false);
    },
  );

  it("handles absent optional stream fields", () => {
    expect(streamMatchesSearch(makeStream(), "absent")).toBe(false);
  });

  it("excludes ids, window class, and direction", () => {
    const graph = makeGraph(
      [makeDevice({ id: "device-key" })],
      [makeStream({ id: "stream-key", window_class: "window-only" })],
    );
    for (const query of [
      "device-key",
      "stream-key",
      "window-only",
      "playback",
      "output",
    ]) {
      expect(applyGraphSearch(graph, query).devices).toEqual([]);
      expect(applyGraphSearch(graph, query).streams).toEqual([]);
    }
  });

  it("keeps distinct streams with the same app name", () => {
    const streams = [
      makeStream({ id: "music-1", app_name: "Music" }),
      makeStream({ id: "music-2", app_name: "Music" }),
      makeStream({ id: "other", app_name: "Browser" }),
    ];
    const result = applyGraphSearch(makeGraph([], streams), "music");
    expect(result.streams.map((stream) => stream.id)).toEqual([
      "music-1",
      "music-2",
    ]);
  });

  it("matches devices and streams independently within the same graph", () => {
    const graph = makeGraph(
      [makeDevice({ id: "speakers", label: "Speakers" })],
      [makeStream({ id: "music", app_name: "Music" })],
    );
    const streamOnly = applyGraphSearch(graph, "music");
    expect(streamOnly.devices).toEqual([]);
    expect(streamOnly.streams.map((stream) => stream.id)).toEqual(["music"]);
    const deviceOnly = applyGraphSearch(graph, "speakers");
    expect(deviceOnly.devices.map((device) => device.id)).toEqual(["speakers"]);
    expect(deviceOnly.streams).toEqual([]);
  });

  it.each(["", " \t\n "])(
    "preserves graph identity for an empty query %j",
    (query) => {
      const graph = makeGraph([makeDevice()], [makeStream()]);
      expect(applyGraphSearch(graph, query)).toBe(graph);
      expect(deviceMatchesSearch(graph.devices[0], query)).toBe(true);
      expect(streamMatchesSearch(graph.streams[0], query)).toBe(true);
    },
  );

  it("filters 120 nodes and restores every node on clear", () => {
    const devices = Array.from({ length: 60 }, (_, i) =>
      makeDevice({ id: `d-${i}`, label: i < 3 ? "Needle" : "Speakers" }),
    );
    const streams = Array.from({ length: 60 }, (_, i) =>
      makeStream({ id: `s-${i}`, app_name: i > 57 ? "Needle" : "Music" }),
    );
    const graph = makeGraph(devices, streams);
    const snapshot = structuredClone(graph);
    const result = applyGraphSearch(graph, "needle");
    expect(result.devices.map((device) => device.id)).toEqual([
      "d-0",
      "d-1",
      "d-2",
    ]);
    expect(result.streams.map((stream) => stream.id)).toEqual(["s-58", "s-59"]);
    for (const query of ["", "   "]) {
      const restored = applyGraphSearch(graph, query);
      expect(restored).toBe(graph);
      expect(restored.devices).toEqual(snapshot.devices);
      expect(restored.streams).toEqual(snapshot.streams);
    }
  });

  it("drops links with either endpoint removed without mutating the source", () => {
    const graph = {
      ...makeGraph(
        [
          makeDevice({ id: "kept-device", label: "Needle speakers" }),
          makeDevice({ id: "removed-device" }),
        ],
        [
          makeStream({ id: "kept-stream", app_name: "Needle app" }),
          makeStream({ id: "removed-stream" }),
        ],
        [
          { id: "keep", source_id: "kept-stream", target_id: "kept-device" },
          {
            id: "drop-device-source",
            source_id: "removed-device",
            target_id: "kept-stream",
          },
          {
            id: "drop-device-target",
            source_id: "kept-stream",
            target_id: "removed-device",
          },
          {
            id: "drop-stream-source",
            source_id: "removed-stream",
            target_id: "kept-device",
          },
          {
            id: "drop-stream-target",
            source_id: "kept-device",
            target_id: "removed-stream",
          },
        ],
      ),
      notice: "Keep graph metadata",
      data_source: "mock",
    };
    const snapshot = structuredClone(graph);
    const result = applyGraphSearch(graph, "needle");
    expect(result).not.toBe(graph);
    expect(result).toEqual({
      ...graph,
      devices: [graph.devices[0]],
      streams: [graph.streams[0]],
      links: [graph.links[0]],
    });
    expect(graph).toEqual(snapshot);
  });
});
