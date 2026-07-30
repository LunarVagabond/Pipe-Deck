import { describe, expect, it } from "vitest";
import {
  defaultLabelForProcessingNodeType,
  isProcessingNodeSystemName,
  isProcessingNodeType,
} from "./graphActions";

describe("isProcessingNodeType", () => {
  it("recognizes every processing-node menu type", () => {
    for (const type of ["fan_out", "mixer", "eq5band", "delay", "limiter", "hpf", "reverb", "widener", "pan"]) {
      expect(isProcessingNodeType(type)).toBe(true);
    }
  });

  it("rejects the device-dialog types (output/input) and unknown strings", () => {
    expect(isProcessingNodeType("output")).toBe(false);
    expect(isProcessingNodeType("input")).toBe(false);
    expect(isProcessingNodeType("not-a-real-type")).toBe(false);
  });
});

describe("defaultLabelForProcessingNodeType", () => {
  it("returns a human-readable label for each type", () => {
    expect(defaultLabelForProcessingNodeType("fan_out")).toBe("Fan-Out");
    expect(defaultLabelForProcessingNodeType("eq5band")).toBe("5-Band EQ");
    expect(defaultLabelForProcessingNodeType("hpf")).toBe("High-Pass Filter");
    expect(defaultLabelForProcessingNodeType("pan")).toBe("Balance/Pan");
  });
});

describe("isProcessingNodeSystemName", () => {
  it("recognizes a processing-node system name", () => {
    expect(isProcessingNodeSystemName("pipe-deck-proc-fan_out-1")).toBe(true);
  });

  it("rejects a virtual device system name", () => {
    expect(isProcessingNodeSystemName("pipe-deck-sink-chat")).toBe(false);
  });
});
