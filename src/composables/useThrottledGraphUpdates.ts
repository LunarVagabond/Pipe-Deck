export interface TrailingDebouncer<T> {
  (value: T): void;
  cancel(): void;
}

/**
 * Trailing-edge debounce with a max-wait ceiling: applies the latest value
 * after `wait` ms of quiet, but never lets sustained churn (each call
 * resetting the `wait` timer) delay an apply past `maxWait` ms from the
 * first call in the current burst. Chosen over a plain debounce (which can
 * starve updates indefinitely under continuous churn) and over
 * rAF-batching (ties the update rate to paint cadence rather than a fixed,
 * easily-testable budget).
 */
export function createTrailingDebouncer<T>(
  apply: (value: T) => void,
  opts: { wait: number; maxWait: number },
): TrailingDebouncer<T> {
  let waitTimer: ReturnType<typeof setTimeout> | null = null;
  let maxWaitTimer: ReturnType<typeof setTimeout> | null = null;
  let latestValue: T;

  function clearTimers() {
    if (waitTimer !== null) {
      clearTimeout(waitTimer);
      waitTimer = null;
    }
    if (maxWaitTimer !== null) {
      clearTimeout(maxWaitTimer);
      maxWaitTimer = null;
    }
  }

  function flush() {
    clearTimers();
    apply(latestValue);
  }

  const debouncer = ((value: T) => {
    latestValue = value;

    if (waitTimer !== null) {
      clearTimeout(waitTimer);
    }
    waitTimer = setTimeout(flush, opts.wait);

    if (maxWaitTimer === null) {
      maxWaitTimer = setTimeout(flush, opts.maxWait);
    }
  }) as TrailingDebouncer<T>;

  debouncer.cancel = clearTimers;

  return debouncer;
}
