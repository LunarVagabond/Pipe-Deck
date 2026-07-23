import { describe, expect, it } from "vitest";
import type { RuleCondition, Stream } from "../types/graph";
import {
  conditionTypeLabel,
  conditionTypeMeta,
  conditionValue,
  formatConditionSummary,
  inferStreamCategory,
  liveSuggestionsForType,
  setConditionType,
  setConditionValue,
  streamFieldValue,
} from "./ruleConditions";

function makeStream(overrides: Partial<Stream> = {}): Stream {
  return {
    id: "stream-1",
    app_name: "Test App",
    direction: "playback",
    ...overrides,
  };
}

describe("conditionTypeLabel / conditionTypeMeta", () => {
  it("returns the known label and metadata for a recognized type", () => {
    expect(conditionTypeLabel("executable")).toBe("Executable");
    expect(conditionTypeMeta("executable").example).toBe("discord");
  });

  it("falls back to the raw type when unrecognized", () => {
    expect(conditionTypeLabel("bogus" as RuleCondition["type"])).toBe("bogus");
    expect(conditionTypeMeta("bogus" as RuleCondition["type"])).toMatchObject({
      type: "bogus",
      label: "bogus",
      description: "",
    });
  });
});

describe("streamFieldValue", () => {
  it("prefers executable over app_name for identity", () => {
    const stream = makeStream({ executable: "firefox", app_name: "Firefox" });
    expect(streamFieldValue(stream, "identity")).toBe("firefox");
  });

  it("falls back to app_name for identity when executable is missing", () => {
    const stream = makeStream({ executable: undefined, app_name: "Firefox" });
    expect(streamFieldValue(stream, "identity")).toBe("Firefox");
  });

  it("reads direct fields for app_name, executable, media_name, window_class, direction", () => {
    const stream = makeStream({
      app_name: "Discord",
      executable: "discord",
      media_name: "voice",
      window_class: "discord.Discord",
      direction: "capture",
    });
    expect(streamFieldValue(stream, "app_name")).toBe("Discord");
    expect(streamFieldValue(stream, "executable")).toBe("discord");
    expect(streamFieldValue(stream, "media_name")).toBe("voice");
    expect(streamFieldValue(stream, "window_class")).toBe("discord.Discord");
    expect(streamFieldValue(stream, "direction")).toBe("capture");
  });

  it("delegates category to inferStreamCategory", () => {
    const stream = makeStream({ app_name: "Spotify" });
    expect(streamFieldValue(stream, "category")).toBe("Music");
  });

  it("returns undefined for regex, which has no single field", () => {
    expect(streamFieldValue(makeStream(), "regex")).toBeUndefined();
  });
});

describe("inferStreamCategory", () => {
  it.each([
    ["steam", "Game"],
    ["spotify", "Music"],
    ["discord", "Chat"],
    ["firefox", "Browser"],
    ["chromium", "Browser"],
    ["obs", "Streaming"],
  ])("classifies %s as %s", (needle, category) => {
    expect(inferStreamCategory(makeStream({ app_name: needle }))).toBe(category);
  });

  it("matches on executable when app_name doesn't hint at a category", () => {
    expect(
      inferStreamCategory(makeStream({ app_name: "MyApp", executable: "steam" })),
    ).toBe("Game");
  });

  it("returns undefined when nothing matches", () => {
    expect(inferStreamCategory(makeStream({ app_name: "Unknown Thing" }))).toBeUndefined();
  });
});

describe("liveSuggestionsForType", () => {
  it("collects unique, sorted, non-empty values for the given field", () => {
    const streams = [
      makeStream({ app_name: "Zeta" }),
      makeStream({ app_name: "Alpha" }),
      makeStream({ app_name: "Alpha" }),
      makeStream({ app_name: "" }),
    ];
    expect(liveSuggestionsForType(streams, "app_name")).toEqual(["Alpha", "Zeta"]);
  });

  it("returns an empty list when no stream has the field", () => {
    expect(liveSuggestionsForType([makeStream({ media_name: undefined })], "media_name")).toEqual([]);
  });
});

describe("conditionValue / setConditionValue", () => {
  it("reads and writes pattern for regex conditions", () => {
    const condition: RuleCondition = { type: "regex", field: "app_name", pattern: "foo" };
    expect(conditionValue(condition)).toBe("foo");
    setConditionValue(condition, "bar");
    expect(condition.pattern).toBe("bar");
  });

  it("reads and writes value for non-regex conditions", () => {
    const condition: RuleCondition = { type: "identity", value: "foo" };
    expect(conditionValue(condition)).toBe("foo");
    setConditionValue(condition, "bar");
    expect(condition.value).toBe("bar");
  });
});

describe("setConditionType", () => {
  it("switching to regex sets a default field and empty pattern", () => {
    const condition: RuleCondition = { type: "identity", value: "old" };
    setConditionType(condition, "regex");
    expect(condition).toMatchObject({ type: "regex", field: "app_name", pattern: "" });
  });

  it("switching to direction defaults to playback", () => {
    const condition: RuleCondition = { type: "identity", value: "old" };
    setConditionType(condition, "direction");
    expect(condition).toEqual({ type: "direction", value: "playback" });
  });

  it("switching to category defaults to Game", () => {
    const condition: RuleCondition = { type: "identity", value: "old" };
    setConditionType(condition, "category");
    expect(condition).toEqual({ type: "category", value: "Game" });
  });

  it("switching to a plain value type clears the value", () => {
    const condition: RuleCondition = { type: "regex", field: "app_name", pattern: "old" };
    setConditionType(condition, "executable");
    expect(condition).toMatchObject({ type: "executable", value: "" });
  });
});

describe("formatConditionSummary", () => {
  it("formats a regex condition with its field label and pattern", () => {
    const condition: RuleCondition = { type: "regex", field: "window_class", pattern: "Disc.*" };
    expect(formatConditionSummary(condition)).toBe("Window Class matches /Disc.*/");
  });

  it("falls back to the raw field value when it isn't a known regex field option", () => {
    const condition: RuleCondition = { type: "regex", field: "unknown_field", pattern: "x" };
    expect(formatConditionSummary(condition)).toBe("unknown_field matches /x/");
  });

  it("formats a direction condition as Capture or Playback", () => {
    expect(formatConditionSummary({ type: "direction", value: "capture" })).toBe("Capture");
    expect(formatConditionSummary({ type: "direction", value: "playback" })).toBe("Playback");
  });

  it("formats identity and other value-based conditions as the raw value", () => {
    expect(formatConditionSummary({ type: "identity", value: "firefox" })).toBe("firefox");
    expect(formatConditionSummary({ type: "category", value: "Game" })).toBe("Game");
  });
});
