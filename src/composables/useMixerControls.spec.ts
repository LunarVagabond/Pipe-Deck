import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { useMixerControls } from "./useMixerControls";

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

describe("clampVolume", () => {
  it("clamps below zero up to zero", () => {
    const { clampVolume } = useMixerControls();
    expect(clampVolume(-10)).toBe(0);
  });

  it("clamps above 100 down to 100", () => {
    const { clampVolume } = useMixerControls();
    expect(clampVolume(150)).toBe(100);
  });

  it("rounds fractional values", () => {
    const { clampVolume } = useMixerControls();
    expect(clampVolume(42.6)).toBe(43);
  });

  it("passes an in-range integer through unchanged", () => {
    const { clampVolume } = useMixerControls();
    expect(clampVolume(50)).toBe(50);
  });
});

describe("setDeviceVolume / setStreamVolume / setDeviceMute / setStreamMute", () => {
  it("setDeviceVolume invokes set_device_volume with the right args", async () => {
    const { setDeviceVolume } = useMixerControls();
    await setDeviceVolume("dev-1", 80);
    expect(invokeMock).toHaveBeenCalledWith("set_device_volume", { deviceId: "dev-1", percent: 80 });
  });

  it("setStreamVolume invokes set_stream_volume with the right args", async () => {
    const { setStreamVolume } = useMixerControls();
    await setStreamVolume("stream-1", 40);
    expect(invokeMock).toHaveBeenCalledWith("set_stream_volume", { streamId: "stream-1", percent: 40 });
  });

  it("setDeviceMute invokes set_device_mute with the right args", async () => {
    const { setDeviceMute } = useMixerControls();
    await setDeviceMute("dev-1", true);
    expect(invokeMock).toHaveBeenCalledWith("set_device_mute", { deviceId: "dev-1", muted: true });
  });

  it("setStreamMute invokes set_stream_mute with the right args", async () => {
    const { setStreamMute } = useMixerControls();
    await setStreamMute("stream-1", false);
    expect(invokeMock).toHaveBeenCalledWith("set_stream_mute", { streamId: "stream-1", muted: false });
  });
});

describe("applyChannelVolume", () => {
  it("calls setStreamVolume for a stream channel", async () => {
    const { applyChannelVolume } = useMixerControls();
    await applyChannelVolume("stream", "s1", 60);
    expect(invokeMock).toHaveBeenCalledWith("set_stream_volume", { streamId: "s1", percent: 60 });
  });

  it("calls setDeviceVolume for a device channel", async () => {
    const { applyChannelVolume } = useMixerControls();
    await applyChannelVolume("device", "d1", 60);
    expect(invokeMock).toHaveBeenCalledWith("set_device_volume", { deviceId: "d1", percent: 60 });
  });

  it("routes a failure to the provided onError instead of the notice store", async () => {
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    const onError = vi.fn();
    const { applyChannelVolume } = useMixerControls();
    await applyChannelVolume("device", "d1", 60, onError);
    expect(onError).toHaveBeenCalledWith("boom");
    expect(handleApplyResultMock).not.toHaveBeenCalled();
  });

  it("falls back to the notice store when no onError is provided", async () => {
    invokeMock.mockRejectedValueOnce(new Error("boom"));
    const { applyChannelVolume } = useMixerControls();
    await applyChannelVolume("device", "d1", 60);
    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "boom" }, "");
  });
});

describe("toggleChannelMute", () => {
  it("un-mutes (invokes with the opposite of the current muted state) and reports success", async () => {
    const { toggleChannelMute } = useMixerControls();
    await toggleChannelMute("device", "d1", true);
    expect(invokeMock).toHaveBeenCalledWith("set_device_mute", { deviceId: "d1", muted: false });
    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: true }, "Unmuted");
  });

  it("mutes and reports success with the default message", async () => {
    const { toggleChannelMute } = useMixerControls();
    await toggleChannelMute("stream", "s1", false);
    expect(invokeMock).toHaveBeenCalledWith("set_stream_mute", { streamId: "s1", muted: true });
    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: true }, "Muted");
  });

  it("uses a custom success message when provided", async () => {
    const { toggleChannelMute } = useMixerControls();
    await toggleChannelMute("device", "d1", true, "Custom message");
    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: true }, "Custom message");
  });

  it("reports failure through the notice store on error", async () => {
    invokeMock.mockRejectedValueOnce(new Error("nope"));
    const { toggleChannelMute } = useMixerControls();
    await toggleChannelMute("device", "d1", true);
    expect(handleApplyResultMock).toHaveBeenCalledWith({ success: false, message: "nope" }, "");
  });
});

describe("scheduleChannelVolume", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("records an optimistic pending value immediately", () => {
    const { scheduleChannelVolume, pendingVolumes } = useMixerControls();
    scheduleChannelVolume("device", "d1", 55);
    expect(pendingVolumes.value["d1"]).toBe(55);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("debounces rapid calls into a single invoke with the latest value", async () => {
    const { scheduleChannelVolume } = useMixerControls();
    scheduleChannelVolume("device", "d1", 10);
    scheduleChannelVolume("device", "d1", 20);
    scheduleChannelVolume("device", "d1", 30);

    await vi.advanceTimersByTimeAsync(120);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("set_device_volume", { deviceId: "d1", percent: 30 });
  });

  it("clamps the scheduled value", async () => {
    const { scheduleChannelVolume } = useMixerControls();
    scheduleChannelVolume("stream", "s1", 999);

    await vi.advanceTimersByTimeAsync(120);

    expect(invokeMock).toHaveBeenCalledWith("set_stream_volume", { streamId: "s1", percent: 100 });
  });
});
