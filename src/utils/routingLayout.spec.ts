import { describe, expect, it } from "vitest";
import { makeDevice } from "../test/graphFixtures";
import { deviceSubtitle, isMultiSink } from "./routingLayout";

describe("isMultiSink", () => {
  it("is false for a virtual output — #293, multi-target fan-out from a plain device is retired", () => {
    expect(
      isMultiSink(makeDevice({ kind: "virtual", direction: "output" })),
    ).toBe(false);
  });

  it("is false for a physical output", () => {
    expect(
      isMultiSink(makeDevice({ kind: "physical", direction: "output" })),
    ).toBe(false);
  });
});

describe("deviceSubtitle", () => {
  it("labels a virtual output device as Virtual Output", () => {
    expect(
      deviceSubtitle(makeDevice({ kind: "virtual", direction: "output" })),
    ).toBe("Virtual Output");
  });

  it("labels a physical output as Hardware Output", () => {
    expect(
      deviceSubtitle(makeDevice({ kind: "physical", direction: "output" })),
    ).toBe("Hardware Output");
  });
});
