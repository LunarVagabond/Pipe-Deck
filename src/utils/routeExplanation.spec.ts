import { describe, expect, it } from "vitest";
import { actionStatusLabel, routeWarningLevel } from "./routeExplanation";
import type { ActionStatus, RouteExplanation } from "../types/graph";

function explanationFor(action_status: ActionStatus): RouteExplanation {
  return {
    source: "authored_rule",
    match_reasons: [],
    skipped_candidates: [],
    action_status,
    fallback_applied: false,
  };
}

describe("actionStatusLabel", () => {
  it.each<[ActionStatus | undefined, string]>([
    ["applied", "Applied"],
    ["blocked", "Blocked"],
    ["skipped_manual_override", "Skipped (manual override)"],
    ["target_unavailable", "Target unavailable"],
    ["simulated", "Would apply"],
    ["no_action", "No action"],
    [undefined, "No action"],
  ])("maps %s to %s", (status, label) => {
    expect(actionStatusLabel(status)).toBe(label);
  });
});

describe("routeWarningLevel", () => {
  it.each<[ActionStatus, "blocked" | "unavailable"]>([
    ["blocked", "blocked"],
    ["skipped_manual_override", "blocked"],
    ["target_unavailable", "unavailable"],
  ])("maps %s to %s", (status, expected) => {
    expect(routeWarningLevel(explanationFor(status))).toBe(expected);
  });

  it.each<ActionStatus>(["applied", "simulated", "no_action"])(
    "returns null for %s",
    (status) => {
      expect(routeWarningLevel(explanationFor(status))).toBeNull();
    },
  );

  it("returns null when there is no explanation", () => {
    expect(routeWarningLevel(undefined)).toBeNull();
  });
});
