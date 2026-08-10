import { describe, expect, it, vi, afterEach } from "vitest";
import {
  filterRecentlySeen,
  recentEntryLabel,
  recentEntryAgo,
} from "./recentStreams";
import type { RecentStreamIdentity } from "../types/graph";

function entry(
  overrides: Partial<RecentStreamIdentity> = {},
): RecentStreamIdentity {
  return {
    app_name: "Firefox",
    direction: "playback",
    last_seen_secs: 0,
    ...overrides,
  };
}

describe("filterRecentlySeen", () => {
  it("returns an empty array for undefined input", () => {
    expect(filterRecentlySeen(undefined)).toEqual([]);
  });

  it("drops entries that are currently live", () => {
    const entries = [
      entry({ app_name: "live" }),
      entry({ app_name: "gone", is_live: true }),
    ];
    expect(filterRecentlySeen(entries).map((e) => e.app_name)).toEqual([
      "live",
    ]);
  });

  it("drops entries flagged as system streams", () => {
    const entries = [
      entry({ app_name: "app" }),
      entry({ app_name: "sys", is_system: true }),
    ];
    expect(filterRecentlySeen(entries).map((e) => e.app_name)).toEqual(["app"]);
  });

  it("keeps entries that are neither live nor system", () => {
    const entries = [entry({ app_name: "keep" })];
    expect(filterRecentlySeen(entries)).toEqual(entries);
  });
});

describe("recentEntryLabel", () => {
  it("shows just the app name when media_name is absent", () => {
    expect(recentEntryLabel(entry({ app_name: "Firefox" }))).toBe("Firefox");
  });

  it("shows just the app name when media_name equals app_name", () => {
    expect(
      recentEntryLabel(entry({ app_name: "Firefox", media_name: "Firefox" })),
    ).toBe("Firefox");
  });

  it("appends media_name in parentheses when it differs from app_name", () => {
    expect(
      recentEntryLabel(
        entry({ app_name: "Firefox", media_name: "YouTube - Song Title" }),
      ),
    ).toBe("Firefox (YouTube - Song Title)");
  });
});

describe("recentEntryAgo", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("reports 'just now' for under a minute", () => {
    const now = Date.UTC(2026, 0, 1, 0, 0, 30);
    vi.useFakeTimers().setSystemTime(now);
    expect(
      recentEntryAgo(entry({ last_seen_secs: Math.floor(now / 1000) - 10 })),
    ).toBe("just now");
  });

  it("reports whole minutes for under an hour", () => {
    const now = Date.UTC(2026, 0, 1, 0, 30, 0);
    vi.useFakeTimers().setSystemTime(now);
    expect(
      recentEntryAgo(
        entry({ last_seen_secs: Math.floor(now / 1000) - 5 * 60 }),
      ),
    ).toBe("5m ago");
  });

  it("reports whole hours at an hour or more", () => {
    const now = Date.UTC(2026, 0, 1, 3, 0, 0);
    vi.useFakeTimers().setSystemTime(now);
    expect(
      recentEntryAgo(
        entry({ last_seen_secs: Math.floor(now / 1000) - 2 * 3600 }),
      ),
    ).toBe("2h ago");
  });

  it("clamps a future last_seen_secs to zero seconds instead of going negative", () => {
    const now = Date.UTC(2026, 0, 1, 0, 0, 0);
    vi.useFakeTimers().setSystemTime(now);
    expect(
      recentEntryAgo(entry({ last_seen_secs: Math.floor(now / 1000) + 500 })),
    ).toBe("just now");
  });
});
