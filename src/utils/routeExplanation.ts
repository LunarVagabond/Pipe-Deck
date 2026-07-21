import type { ActionStatus, RouteExplanation, RouteSource } from "../types/graph";

export function formatRuleLabel(
  ruleKey: string,
  source?: RouteSource,
  matchedRuleId?: string,
): string {
  if (source === "persisted_rule" || ruleKey.startsWith("persisted:")) {
    return "Manual route";
  }
  if (source === "manual_override") {
    return "Manual choice";
  }
  if (source === "authored_rule") {
    return ruleKey || matchedRuleId || "Rule";
  }
  return ruleKey || matchedRuleId || "Rule";
}

export function routeExplanationSummary(
  explanation: RouteExplanation,
  targetLabel?: string,
): string {
  if (explanation.source === "manual_override") {
    return "Manual choice this session";
  }

  if (explanation.source === "no_rule") {
    return "No matching auto-route rule";
  }

  const ruleLabel = formatRuleLabel(
    explanation.matched_rule_key ?? "",
    explanation.source,
    explanation.matched_rule_id,
  );

  if (targetLabel) {
    if (explanation.source === "persisted_rule") {
      return `Routed manually → ${targetLabel}`;
    }
    return `Routed by ${ruleLabel} → ${targetLabel}`;
  }

  if (explanation.source === "persisted_rule") {
    return "Saved manual route";
  }

  return `Matched ${ruleLabel}`;
}

export function actionStatusLabel(status: ActionStatus | undefined): string {
  switch (status) {
    case "applied":
      return "Applied";
    case "blocked":
      return "Blocked";
    case "skipped_manual_override":
      return "Skipped (manual override)";
    case "target_unavailable":
      return "Target unavailable";
    case "simulated":
      return "Would apply";
    default:
      return "No action";
  }
}

/**
 * `blocked` and `skipped_manual_override` both mean "a rule matched but
 * nothing was applied" and read the same on the graph; `target_unavailable`
 * ("the destination doesn't exist") is a distinct, more severe case and gets
 * its own color.
 */
export function routeWarningLevel(
  explanation: RouteExplanation | undefined,
): "blocked" | "unavailable" | null {
  switch (explanation?.action_status) {
    case "blocked":
    case "skipped_manual_override":
      return "blocked";
    case "target_unavailable":
      return "unavailable";
    default:
      return null;
  }
}
