import { describe, expect, it } from "vitest";
import { formatMismatch } from "./formatMismatch";

describe("formatMismatch", () => {
  it("has no mismatch when rate and channels match", () => {
    const result = formatMismatch(
      { sample_rate: 48000, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(result.mismatch).toBe(false);
    expect(result.title).toBeUndefined();
  });

  it("flags a mismatch when sample rate differs", () => {
    const result = formatMismatch(
      { sample_rate: 44100, channels: 2 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(result.mismatch).toBe(true);
    expect(result.title).toContain("44100 Hz → 48000 Hz");
  });

  it("flags a mismatch when channel count differs", () => {
    const result = formatMismatch(
      { sample_rate: 48000, channels: 1 },
      { sample_rate: 48000, channels: 2 },
    );
    expect(result.mismatch).toBe(true);
    expect(result.title).toContain("1ch → 2ch");
  });

  it("does not flag a mismatch when either side's value is unknown", () => {
    expect(formatMismatch({ channels: 2 }, { channels: 2 }).mismatch).toBe(false);
    expect(formatMismatch({ sample_rate: 44100 }, {}).mismatch).toBe(false);
    expect(formatMismatch({}, { sample_rate: 48000, channels: 2 }).mismatch).toBe(false);
  });
});
