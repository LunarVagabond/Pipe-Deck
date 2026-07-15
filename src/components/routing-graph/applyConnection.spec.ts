import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Connection } from "@vue-flow/core";
import { makeDevice, makeGraph, makeStream } from "../../test/graphFixtures";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const { applyEdgeDisconnect, applyRoutingConnection } = await import("./applyConnection");

function connection(overrides: Partial<Connection>): Connection {
  return {
    source: null,
    target: null,
    sourceHandle: null,
    targetHandle: null,
    ...overrides,
  } as Connection;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("applyRoutingConnection", () => {
  it("reports the validation error and never calls invoke when the drag is invalid", async () => {
    const graph = makeGraph();
    const onResult = vi.fn();

    const applied = await applyRoutingConnection(graph, connection({}), onResult);

    expect(applied).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(onResult).toHaveBeenCalledWith(
      { success: false, message: "Drag needs both a source and a target port." },
      "",
    );
  });

  it("invokes set_stream_target for a valid stream-to-device drag", async () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    invokeMock.mockResolvedValue({ success: true });
    const onResult = vi.fn();

    const applied = await applyRoutingConnection(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
      onResult,
    );

    expect(applied).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith("set_stream_target", {
      streamId: "s1",
      targetDeviceId: "d1",
    });
    expect(onResult).toHaveBeenCalledWith({ success: true }, "Routing updated");
  });

  it("invokes set_device_targets for a multi-sink fan-out add", async () => {
    const sink = makeDevice({
      id: "sink1",
      kind: "virtual",
      direction: "output",
      current_targets: ["out1"],
    });
    const out1 = makeDevice({ id: "out1", direction: "output" });
    const out2 = makeDevice({ id: "out2", direction: "output" });
    const graph = makeGraph([sink, out1, out2]);
    invokeMock.mockResolvedValue({ success: true });
    const onResult = vi.fn();

    await applyRoutingConnection(
      graph,
      connection({
        source: "device:sink1",
        target: "device:out2",
        sourceHandle: "audio-out:empty",
        targetHandle: "audio-in:empty",
      }),
      onResult,
    );

    expect(invokeMock).toHaveBeenCalledWith("set_device_targets", {
      sourceDeviceId: "sink1",
      targetDeviceIds: ["out1", "out2"],
    });
    expect(onResult).toHaveBeenCalledWith({ success: true }, "Sink routing updated");
  });

  it("wraps a thrown invoke error with context instead of showing it raw", async () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    invokeMock.mockRejectedValue(new Error("backend exploded"));
    const onResult = vi.fn();

    const applied = await applyRoutingConnection(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
      onResult,
    );

    expect(applied).toBe(false);
    expect(onResult).toHaveBeenCalledWith(
      { success: false, message: "Couldn't update routing: backend exploded" },
      "",
    );
  });

  it("passes through a backend-reported failure without invoke throwing", async () => {
    const stream = makeStream({ id: "s1", direction: "playback" });
    const device = makeDevice({ id: "d1", direction: "output" });
    const graph = makeGraph([device], [stream]);
    invokeMock.mockResolvedValue({ success: false, message: "device is busy" });
    const onResult = vi.fn();

    await applyRoutingConnection(
      graph,
      connection({
        source: "stream:s1",
        target: "device:d1",
        sourceHandle: "audio-out",
        targetHandle: "audio-in:empty",
      }),
      onResult,
    );

    expect(onResult).toHaveBeenCalledWith({ success: false, message: "device is busy" }, "Routing updated");
  });
});

describe("applyEdgeDisconnect", () => {
  it("invokes clear_stream_target for a matching stream-device edge", async () => {
    const stream = makeStream({ id: "s1", current_target: "d1" });
    const device = makeDevice({ id: "d1" });
    const graph = makeGraph([device], [stream]);
    invokeMock.mockResolvedValue({ success: true });
    const onResult = vi.fn();

    await applyEdgeDisconnect(graph, { source: "stream:s1", target: "device:d1" }, onResult);

    expect(invokeMock).toHaveBeenCalledWith("clear_stream_target", {
      streamId: "s1",
      previousTargetDeviceId: "d1",
    });
    expect(onResult).toHaveBeenCalledWith({ success: true }, "Routing cleared");
  });
});
