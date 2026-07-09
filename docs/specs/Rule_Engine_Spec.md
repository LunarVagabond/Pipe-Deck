# Rule Engine Spec

## Purpose

Define how Pipe Deck evaluates automatic routing rules in a way that is deterministic, explainable, and safe.

## In Scope

- Rule model and match conditions.
- Evaluation order and conflict resolution.
- Explainability and debug requirements.

## Out of Scope

- ML-based policy inference.
- Fully dynamic scripting language for Phase 1.

## Relationship to Phase 2 `routing_rules`

Phase 2 ships a **minimal** persistence layer in `config.yaml` (`routing_rules`): save on dropdown change, re-apply on refresh. Phase 3 extends this with the full rule engine while keeping `routing_rules` as implicit low-priority candidates.

**Implemented behavior (2026-07-09):**

- Authored `rules[]` entries are evaluated with explicit priority and explainability.
- Dashboard dropdown changes still append to `routing_rules.stream_rules` at implicit priority `-1000` (minus index), so authored rules win when both match.
- On first upgrade, existing `routing_rules.stream_rules` migrate once into `rules[]` when `rules` is empty.
- Session manual overrides block auto-apply until cleared (including when the user re-selects the rule's target).

See `docs/specs/Config_Spec.md` for serialization and precedence details.

## Rule Model

A rule contains:

- Metadata (id, name, enabled state, priority).
- Conditions (application, device, context signals).
- Action (route to target sink/source, set profile behavior, etc.).
- Safeguards (allow/deny when target unavailable, fallback policy).

## Condition Sources

- Application identity (name, executable, window class where available — best-effort from `window.x11.class`, `application.id`, or `application.icon-name`).
- Process name and application ID where available.
- Stream direction (playback/capture).
- Device type/category.
- Optional user-defined regex conditions.
- Session context (optional future: workspace/profile mode).

## Evaluation Semantics

- Deterministic order by priority, then creation order.
- First-match-wins by default.
- Optional merge behavior only for explicitly compatible actions.

## Conflict Resolution

When multiple rules apply:

- Prefer highest priority explicit matches.
- Log losing candidates for explainability.
- If conflict remains ambiguous, fall back to safe default route.

## Explainability Requirements

The system must answer:

- Which rule applied?
- Why it matched?
- Which candidates were skipped and why?
- Whether action was executed, partially executed, or blocked.

## Safety Requirements

- Never orphan critical streams without fallback.
- Never override explicit manual user route without clear policy.
- Preserve a revert path to prior known-good routing state.

## Decisions

- Rule conflict behavior is deterministic and global in MVP (no per-profile conflict policy initially).
- Manual user overrides take precedence for the active session.
- Minimum debug details always include matched rule ID, match reason, chosen action, and fallback behavior.

## Traceability to User Value

- Deterministic rules -> predictable behavior.
- Clear explanations -> easier troubleshooting and trust.
- Safe fallbacks -> fewer broken audio sessions.

## Legacy Examples (Absorbed)

- IF executable == discord THEN output = Chat AND input = Discord Mic
- IF executable == spotify THEN output = Music
- IF category == Game THEN output = Game
