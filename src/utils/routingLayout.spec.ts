import { describe, expect, it } from "vitest";
import { makeDevice } from "../test/graphFixtures";
import { deviceSubtitle, isMultiSink } from "./routingLayout";

describe("isMultiSink", () => {
  it("is true for a bus (today's virtual-output behavior)", () => {
    expect(isMultiSink(makeDevice({ kind: "virtual", direction: "output" }))).toBe(true);
  });

  it("is false for a terminal Output (virtual) — #287, it never fans out", () => {
    expect(isMultiSink(makeDevice({ kind: "virtual", direction: "output", virtual_role: "output" }))).toBe(
      false,
    );
  });

  it("is false for a physical output", () => {
    expect(isMultiSink(makeDevice({ kind: "physical", direction: "output" }))).toBe(false);
  });
});

describe("deviceSubtitle", () => {
  it("labels a bus device as Bus", () => {
    expect(deviceSubtitle(makeDevice({ kind: "virtual", direction: "output" }))).toBe("Bus");
  });

  it("labels a terminal Output (virtual) device as Virtual Output", () => {
    expect(deviceSubtitle(makeDevice({ kind: "virtual", direction: "output", virtual_role: "output" }))).toBe(
      "Virtual Output",
    );
  });

  it("labels a physical output as Hardware Output", () => {
    expect(deviceSubtitle(makeDevice({ kind: "physical", direction: "output" }))).toBe("Hardware Output");
  });
});
