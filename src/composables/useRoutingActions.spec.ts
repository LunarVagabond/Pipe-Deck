import { beforeEach, describe, expect, it, vi } from "vitest";
import { useRoutingActions } from "./useRoutingActions";

const invokeMock = vi.hoisted(() => vi.fn());
const handleApplyResultMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("../stores/notices", () => ({
  useApplyResult: () => ({ handleApplyResult: handleApplyResultMock }),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  handleApplyResultMock.mockReset();
});

describe("refreshCanUndo", () => {
  it("stores the backend boolean", async () => {
    invokeMock.mockResolvedValueOnce(true);
    const { canUndo, refreshCanUndo } = useRoutingActions();

    await refreshCanUndo();

    expect(invokeMock).toHaveBeenCalledWith("can_undo_routing");
    expect(canUndo.value).toBe(true);
  });

  it.each([
    [new Error("backend unavailable"), "an Error"],
    ["plain rejection", "a non-Error rejection"],
  ])("resets false for %s", async (rejection, description) => {
    invokeMock.mockResolvedValueOnce(true);
    const { canUndo, refreshCanUndo } = useRoutingActions();
    await refreshCanUndo();

    invokeMock.mockRejectedValueOnce(rejection);
    await refreshCanUndo();

    expect(canUndo.value, description).toBe(false);
  });
});

describe("setStreamTarget", () => {
  it("invokes the exact command and arguments, returns the payload, and reports the default message", async () => {
    const result = { success: true, message: "backend details" };
    invokeMock.mockResolvedValueOnce(result);
    const { setStreamTarget } = useRoutingActions();

    await expect(setStreamTarget("stream-1", "device-1")).resolves.toEqual(
      result,
    );

    expect(invokeMock).toHaveBeenCalledWith("set_stream_target", {
      streamId: "stream-1",
      targetDeviceId: "device-1",
    });
    expect(handleApplyResultMock).toHaveBeenCalledWith(
      result,
      "Routing updated",
    );
  });

  it("uses a custom success message", async () => {
    const result = { success: true };
    invokeMock.mockResolvedValueOnce(result);
    const { setStreamTarget } = useRoutingActions();

    await setStreamTarget("stream-1", "device-1", "Route restored");

    expect(handleApplyResultMock).toHaveBeenCalledWith(
      result,
      "Route restored",
    );
  });

  it.each([
    [new Error("route failed"), "route failed"],
    ["plain route failure", "plain route failure"],
  ])(
    "normalizes %s rejection and notifies with an empty success message",
    async (rejection, message) => {
      invokeMock.mockRejectedValueOnce(rejection);
      const { setStreamTarget } = useRoutingActions();

      await expect(setStreamTarget("stream-1", "device-1")).resolves.toEqual({
        success: false,
        message,
      });

      expect(handleApplyResultMock).toHaveBeenCalledWith(
        { success: false, message },
        "",
      );
    },
  );
});

describe("clearStreamTarget", () => {
  it("invokes the exact command and arguments, returns the payload, and reports the default message", async () => {
    const result = { success: true };
    invokeMock.mockResolvedValueOnce(result);
    const { clearStreamTarget } = useRoutingActions();

    await expect(clearStreamTarget("stream-1", "device-1")).resolves.toEqual(
      result,
    );

    expect(invokeMock).toHaveBeenCalledWith("clear_stream_target", {
      streamId: "stream-1",
      previousTargetDeviceId: "device-1",
    });
    expect(handleApplyResultMock).toHaveBeenCalledWith(
      result,
      "Routing cleared",
    );
  });

  it("uses a custom success message", async () => {
    const result = { success: true, message: "cleared" };
    invokeMock.mockResolvedValueOnce(result);
    const { clearStreamTarget } = useRoutingActions();

    await clearStreamTarget("stream-1", "device-1", "Route removed");

    expect(handleApplyResultMock).toHaveBeenCalledWith(result, "Route removed");
  });

  it.each([
    [new Error("clear failed"), "clear failed"],
    ["plain clear failure", "plain clear failure"],
  ])(
    "normalizes %s rejection and notifies with an empty success message",
    async (rejection, message) => {
      invokeMock.mockRejectedValueOnce(rejection);
      const { clearStreamTarget } = useRoutingActions();

      await expect(clearStreamTarget("stream-1", "device-1")).resolves.toEqual({
        success: false,
        message,
      });

      expect(handleApplyResultMock).toHaveBeenCalledWith(
        { success: false, message },
        "",
      );
    },
  );
});

describe("undoLastRouting", () => {
  it("is a no-op while canUndo is false", async () => {
    const { undoLastRouting } = useRoutingActions();

    await undoLastRouting();

    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("undoes the last route and refreshes canUndo", async () => {
    const { canUndo, refreshCanUndo, undoLastRouting } = useRoutingActions();
    invokeMock.mockResolvedValueOnce(true);
    await refreshCanUndo();
    invokeMock.mockClear();
    invokeMock.mockResolvedValueOnce(undefined).mockResolvedValueOnce(false);

    await undoLastRouting();

    expect(invokeMock).toHaveBeenNthCalledWith(1, "undo_last_routing");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "can_undo_routing");
    expect(canUndo.value).toBe(false);
  });

  it("remains non-throwing when undo is rejected", async () => {
    const { refreshCanUndo, undoLastRouting } = useRoutingActions();
    invokeMock.mockResolvedValueOnce(true);
    await refreshCanUndo();
    invokeMock.mockRejectedValueOnce("undo failed");

    await expect(undoLastRouting()).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenLastCalledWith("undo_last_routing");
  });
});
