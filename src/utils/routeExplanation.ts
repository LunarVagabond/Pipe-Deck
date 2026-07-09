import type { RouteExplanation, RouteSource } from "../types/graph";

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
