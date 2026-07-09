# Product Decisions

## Purpose

Centralized record of accepted product and architecture decisions for Pipe Deck.

## Decision Log

### PD-001 Default Landing View

- Status: Accepted
- Decision: Dashboard is the default first-launch landing page.
- Rationale: Fast orientation around current audio state improves confidence for new users.

### PD-002 Routing Apply Model

- Status: Accepted
- Decision: Routing edits apply immediately by default.
- Constraint: Undo/rollback is required for all routing edits.
- Rationale: Immediate feedback reduces friction and keeps workflows fast.

### PD-003 Profile Storage Model

- Status: Accepted
- Decision: Profiles are stored as separate YAML files by default.
- Constraint: Main config may maintain a lightweight profile index and active profile pointer.
- Rationale: Better portability, backup, and sharing behavior for user setups.

### PD-004 Plugin Runtime Isolation

- Status: Accepted
- Decision: Plugins run in isolated subprocesses by default.
- Constraints:
  - Capabilities are explicit and denied by default until approved.
  - Plugin failures must not crash or block core routing operations.
- Rationale: Safety and fault isolation are mandatory for extension support.

### PD-005 Rule Engine Precedence and Debug Minimum

- Status: Accepted
- Decisions:
  - Global deterministic conflict policy in MVP.
  - Manual user overrides take precedence for the active session.
  - Minimum debug detail includes matched rule ID, match reason, chosen action, and fallback behavior.
- Rationale: Predictable outcomes and explainability are core product requirements.

### PD-006 Daemon Requirement Boundary

- Status: Accepted
- Decision: Daemon remains optional until persistence and restore workflows require background ownership.
- Rationale: Avoid early operational complexity while preserving future extensibility.

### PD-007 Automatic Mapping Rollout Policy

- Status: Accepted
- Decisions:
  - Initial automatic mapping operates in suggest-first mode.
  - Auto-apply requires confidence and safety checks.
  - Disconnect handling uses grace window followed by last known-good fallback.
- Rationale: Automation should reduce effort without hidden or unsafe behavior.

### PD-008 Diagnostics Direction

- Status: Accepted
- Decision: Diagnostics prioritize local explainability and troubleshooting over mandatory telemetry.
- Rationale: Transparency and user trust are more important than early telemetry breadth.

### PD-009 Phase 2 Persistence Model

- Status: Accepted
- Decision: File-first YAML for config and profiles in Phase 2; no SQLite or database layer.
- Constraints:
  - Main config (`config.yaml`) holds preferences, active profile pointer, and profile index.
  - Profiles stored as separate YAML files under `profiles/`.
  - Export/import via file copy or simple archive.
  - SQLite may be introduced later only if indexing, concurrent writes, or daemon reconciliation justify it; YAML remains the portable contract.
- Rationale: Simpler to implement, debug, and share; matches PD-003; sufficient for Phase 2 complexity.

### PD-010 Frontend Styling Model

- Status: Accepted
- Decision: Frontend styles use SCSS partials under `src/styles/`; Vue components must not contain `<style>` blocks.
- Constraints:
  - `src/styles/main.scss` is the single entry imported by `src/main.ts`.
  - View/component styles namespace under a root class (for example `.dashboard`, `.routing-matrix`).
  - Shared theme tokens live in `src/styles/_variables.scss`.
- Rationale: Keeps presentation separate from component logic, makes styles easier to scan and reuse, and avoids scattered scoped blocks.

## Related Documents

- `docs/product/Product_Requirements.md`
- `docs/product/Roadmap.md`
- `docs/architecture/System_Architecture.md`
- `docs/architecture/PipeWire_Design.md`
- `docs/specs/UI_Spec.md`
- `docs/specs/Config_Spec.md`
- `docs/specs/Plugin_API.md`
- `docs/architecture/Phase2_Scaffold.md`
- `docs/project/Packaging.md`
