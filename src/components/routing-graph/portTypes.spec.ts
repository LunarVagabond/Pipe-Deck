import { describe, expect, it } from "vitest";
import { canConnectPorts, isHandleFillable } from "./portTypes";

describe("isHandleFillable", () => {
  it("treats stream handles (no colon) as always fillable", () => {
    expect(isHandleFillable("audio-out")).toBe(true);
    expect(isHandleFillable("audio-in")).toBe(true);
  });

  it("treats an :empty device handle as fillable", () => {
    expect(isHandleFillable("audio-in:empty")).toBe(true);
  });

  it("treats an already-occupied device handle as not fillable", () => {
    expect(isHandleFillable("audio-in:some-device-id")).toBe(false);
  });
});

describe("canConnectPorts", () => {
  it("rejects a missing source or target handle", () => {
    expect(canConnectPorts(null, "audio-in:empty")).toBe(false);
    expect(canConnectPorts("audio-out", undefined)).toBe(false);
  });

  it("rejects an input connecting to an input", () => {
    expect(canConnectPorts("audio-in", "audio-in:empty")).toBe(false);
  });

  it("rejects an output connecting to an occupied input slot by default", () => {
    expect(canConnectPorts("audio-out", "audio-in:other")).toBe(false);
  });

  it("allows an output connecting to an open input slot", () => {
    expect(canConnectPorts("audio-out", "audio-in:empty")).toBe(true);
    expect(canConnectPorts("audio-out", "audio-in")).toBe(true);
  });

  it("skips the empty-slot requirement when re-validating an existing edge", () => {
    expect(canConnectPorts("audio-out:device-a", "audio-in:device-b", false)).toBe(true);
  });
});
