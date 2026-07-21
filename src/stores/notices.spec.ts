import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

beforeEach(() => {
  vi.resetModules();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
});

describe("useNotices", () => {
  it("pushes notices newest-first with auto-incrementing ids", async () => {
    const { useNotices } = await import("./notices");
    const { notices, pushNotice } = useNotices();

    pushNotice("info", "first", 0);
    pushNotice("success", "second", 0);

    expect(notices.value.map((n) => n.message)).toEqual(["second", "first"]);
    expect(notices.value[1].id).toBeLessThan(notices.value[0].id);
  });

  it("truncates to the 4 most recent notices", async () => {
    const { useNotices } = await import("./notices");
    const { notices, pushNotice } = useNotices();

    for (let i = 1; i <= 5; i += 1) {
      pushNotice("info", `notice-${i}`, 0);
    }

    expect(notices.value).toHaveLength(4);
    expect(notices.value.map((n) => n.message)).toEqual(["notice-5", "notice-4", "notice-3", "notice-2"]);
  });

  describe("auto-dismiss timing", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("auto-dismisses a notice after its timeout elapses", async () => {
      const { useNotices } = await import("./notices");
      const { notices, pushNotice } = useNotices();

      pushNotice("info", "will expire", 5000);
      expect(notices.value).toHaveLength(1);

      await vi.advanceTimersByTimeAsync(5000);

      expect(notices.value).toHaveLength(0);
    });

    it("never schedules a dismiss when timeoutMs is not positive", async () => {
      const { useNotices } = await import("./notices");
      const { notices, pushNotice } = useNotices();

      pushNotice("info", "persistent", 0);
      await vi.advanceTimersByTimeAsync(1_000_000);

      expect(notices.value).toHaveLength(1);
    });
  });

  it("dismisses only the matching notice, and no-ops for an unknown id", async () => {
    const { useNotices } = await import("./notices");
    const { notices, pushNotice, dismissNotice } = useNotices();

    pushNotice("info", "keep", 0);
    pushNotice("info", "remove", 0);
    const [toRemove, toKeep] = [...notices.value];

    dismissNotice(toRemove.id);
    expect(notices.value.map((n) => n.id)).toEqual([toKeep.id]);

    dismissNotice(999_999);
    expect(notices.value.map((n) => n.id)).toEqual([toKeep.id]);
  });
});

describe("useNoticeSettings.initNoticeSettings", () => {
  it("sets the duration from config preferences on success", async () => {
    invokeMock.mockResolvedValue({ preferences: { notice_duration_ms: 8000 } });
    const { useNoticeSettings } = await import("./notices");
    const { noticeDurationMs, initNoticeSettings } = useNoticeSettings();

    await initNoticeSettings();

    expect(noticeDurationMs.value).toBe(8000);
  });

  it("falls back to the default when preferences/notice_duration_ms is missing", async () => {
    invokeMock.mockResolvedValue({});
    const { useNoticeSettings, DEFAULT_NOTICE_DURATION_MS } = await import("./notices");
    const { noticeDurationMs, initNoticeSettings } = useNoticeSettings();

    await initNoticeSettings();

    expect(noticeDurationMs.value).toBe(DEFAULT_NOTICE_DURATION_MS);
  });

  it("silently falls back to the default when the invoke call rejects", async () => {
    invokeMock.mockRejectedValue(new Error("config unavailable"));
    const { useNoticeSettings, DEFAULT_NOTICE_DURATION_MS } = await import("./notices");
    const { noticeDurationMs, initNoticeSettings } = useNoticeSettings();

    await expect(initNoticeSettings()).resolves.toBeUndefined();
    expect(noticeDurationMs.value).toBe(DEFAULT_NOTICE_DURATION_MS);
  });
});

describe("useNoticeSettings.setNoticeDuration", () => {
  it("optimistically applies the new duration and keeps it on success", async () => {
    const { useNoticeSettings } = await import("./notices");
    const { noticeDurationMs, setNoticeDuration } = useNoticeSettings();

    await setNoticeDuration(9000);

    expect(invokeMock).toHaveBeenCalledWith("set_notice_duration_ms", { ms: 9000 });
    expect(noticeDurationMs.value).toBe(9000);
  });

  it("rolls back and pushes an error notice when the invoke call fails", async () => {
    const { useNoticeSettings, useNotices, DEFAULT_NOTICE_DURATION_MS } = await import("./notices");
    const { noticeDurationMs, setNoticeDuration } = useNoticeSettings();
    const { notices } = useNotices();
    invokeMock.mockRejectedValue(new Error("save failed"));

    await setNoticeDuration(9000);

    expect(noticeDurationMs.value).toBe(DEFAULT_NOTICE_DURATION_MS);
    expect(notices.value[0]).toMatchObject({ kind: "error", message: "save failed" });
  });
});

describe("useApplyResult.handleApplyResult", () => {
  it("pushes a success notice and returns true on success", async () => {
    const { useApplyResult, useNotices } = await import("./notices");
    const { handleApplyResult } = useApplyResult();
    const { notices } = useNotices();

    const outcome = handleApplyResult({ success: true }, "Saved");

    expect(outcome).toBe(true);
    expect(notices.value[0]).toMatchObject({ kind: "success", message: "Saved" });
  });

  it("pushes an error notice with the message and returns false on failure", async () => {
    const { useApplyResult, useNotices } = await import("./notices");
    const { handleApplyResult } = useApplyResult();
    const { notices } = useNotices();

    const outcome = handleApplyResult({ success: false, message: "boom" }, "");

    expect(outcome).toBe(false);
    expect(notices.value[0]).toMatchObject({ kind: "error", message: "boom" });
  });

  it("falls back to a generic error message when none is provided", async () => {
    const { useApplyResult, useNotices } = await import("./notices");
    const { handleApplyResult } = useApplyResult();
    const { notices } = useNotices();

    handleApplyResult({ success: false }, "");

    expect(notices.value[0]).toMatchObject({ kind: "error", message: "Operation failed" });
  });
});
