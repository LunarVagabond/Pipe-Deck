import { describe, expect, it } from "vitest";
import { streamIdentityKey } from "./streamIdentity";
import { makeStream } from "../test/graphFixtures";

describe("streamIdentityKey", () => {
  it("matches for two streams with the same app_name/executable/media_name", () => {
    const a = makeStream({
      id: "node-1",
      app_name: "Firefox",
      executable: "firefox",
    });
    const b = makeStream({
      id: "node-2",
      app_name: "Firefox",
      executable: "firefox",
    });
    expect(streamIdentityKey(a)).toBe(streamIdentityKey(b));
  });

  it("differs when executable differs", () => {
    const a = makeStream({
      id: "node-1",
      app_name: "Chrome",
      executable: "chrome",
    });
    const b = makeStream({
      id: "node-2",
      app_name: "Chrome",
      executable: "chromium",
    });
    expect(streamIdentityKey(a)).not.toBe(streamIdentityKey(b));
  });

  it("falls back to media_name distinguishing two streams of the same app", () => {
    const a = makeStream({
      id: "node-1",
      app_name: "Firefox",
      media_name: "tab-a",
    });
    const b = makeStream({
      id: "node-2",
      app_name: "Firefox",
      media_name: "tab-b",
    });
    expect(streamIdentityKey(a)).not.toBe(streamIdentityKey(b));
  });

  it("differs for distinct apps", () => {
    const a = makeStream({ id: "node-1", app_name: "Firefox" });
    const b = makeStream({ id: "node-2", app_name: "Spotify" });
    expect(streamIdentityKey(a)).not.toBe(streamIdentityKey(b));
  });
});
