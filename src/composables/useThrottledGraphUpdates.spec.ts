import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTrailingDebouncer } from "./useThrottledGraphUpdates";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("createTrailingDebouncer", () => {
  it("applies a single event after the wait window", () => {
    const apply = vi.fn();
    const debounced = createTrailingDebouncer(apply, { wait: 100, maxWait: 150 });

    debounced("a");
    expect(apply).not.toHaveBeenCalled();

    vi.advanceTimersByTime(99);
    expect(apply).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(apply).toHaveBeenCalledExactlyOnceWith("a");
  });

  it("never delays a sustained burst past maxWait from its first event", () => {
    const apply = vi.fn();
    const debounced = createTrailingDebouncer(apply, { wait: 100, maxWait: 150 });

    debounced("a");
    vi.advanceTimersByTime(60);
    debounced("b"); // resets the 100ms wait timer, but not the 150ms max-wait ceiling
    vi.advanceTimersByTime(60);
    debounced("c"); // 120ms since "a"; still under maxWait, wait timer resets again
    expect(apply).not.toHaveBeenCalled();

    // 150ms since the first event in the burst: maxWait should fire here,
    // pre-empting the (now 220ms-scheduled) wait timer from the "c" call.
    vi.advanceTimersByTime(30);
    expect(apply).toHaveBeenCalledExactlyOnceWith("c");
  });

  it("applies independently-spaced events as separate bursts", () => {
    const apply = vi.fn();
    const debounced = createTrailingDebouncer(apply, { wait: 100, maxWait: 150 });

    debounced("a");
    vi.advanceTimersByTime(100);
    expect(apply).toHaveBeenCalledExactlyOnceWith("a");

    debounced("b");
    vi.advanceTimersByTime(100);
    expect(apply).toHaveBeenCalledTimes(2);
    expect(apply).toHaveBeenLastCalledWith("b");
  });

  it("cancel stops a pending apply", () => {
    const apply = vi.fn();
    const debounced = createTrailingDebouncer(apply, { wait: 100, maxWait: 150 });

    debounced("a");
    debounced.cancel();
    vi.advanceTimersByTime(500);

    expect(apply).not.toHaveBeenCalled();
  });
});
