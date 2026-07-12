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
- When `preferences.auto_apply_rules` is true (default), newly seen stream identities are routed on graph refresh without clearing session overrides.

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

### User-facing labels (implemented)

| `RouteSource` | Display label |
|---------------|---------------|
| `authored_rule` | Rule name (`rules[].name`) |
| `persisted_rule` | **Manual route** (dashboard-saved `routing_rules`) |
| `manual_override` | **Manual choice this session** |
| `no_rule` | No matching auto-route rule |

Internal candidate keys are not shown in the dashboard summary.

### Known metadata limitations

`window_class` is best-effort: it's derived from whichever of `window.x11.class`,
`application.id`, or `application.icon-name` PipeWire reports for a stream's node
(see `parse_window_class` in `core/stream_identity.rs`). Some Wayland compositors
never populate any of these properties, so a rule that conditions on
`window_class` (directly or via a `Regex` condition on the `window_class` field)
cannot be evaluated for those streams. When that happens and no other rule
matches, the miss is surfaced in `skipped_candidates` with a reason explaining
that the required metadata wasn't reported — this is distinct from the rule
simply not matching the stream's actual window class.

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

## Test Scenario Matrix

Deterministic unit tests in `src-tauri/src/core/rules/` (`mod tests`):

| Scenario | Test name |
|----------|-----------|
| Persisted rule matches executable | `persisted_rule_matches_executable_only` |
| Persisted rule requires all specified fields | `persisted_rule_requires_all_specified_fields` |
| Category rule matches games | `authored_category_rule_matches_games` |
| Matching rule target is not manual override | `matching_rule_target_is_not_manual_override` |
| Manual override blocks auto-apply | `manual_override_blocks_auto_apply_explanation` |
| External manual override detected | `detect_external_manual_override_when_system_differs_from_rule` |
| Regex condition matches app name | `regex_condition_matches_app_name` |
| Identity matches app name or executable | `identity_matches_app_name_or_executable` |
| `keep_current` skips when target missing | `keep_current_skips_when_rule_target_missing` |
| `safe_default` falls back to physical output | `safe_default_falls_back_to_physical_output` |
| Authored rule beats persisted rule on priority | `authored_rule_beats_persisted_rule_on_priority` |
| Disabled authored rule skipped | `disabled_authored_rule_is_skipped` |
| Multiple authored rules — highest priority wins | `multiple_authored_rules_highest_priority_wins` |
| Capture stream matches direction rule | `capture_stream_matches_direction_rule` |
| Device rule mismatch tracks manual override | `device_rule_mismatch_tracks_manual_override` |

Run via `make test`.

## Legacy Examples (Absorbed)

- IF executable == discord THEN output = Chat AND input = Discord Mic
- IF executable == spotify THEN output = Music
- IF category == Game THEN output = Game
