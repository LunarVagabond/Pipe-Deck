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

- Status: Accepted (implemented Phase 4)
- Decision: Daemon remains optional until persistence and restore workflows require background ownership.
- Phase 4 implementation: `pipe-deck-daemon` binary, user systemd unit, Settings UI toggle; disabled by default.
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

### PD-011 Lightweight Route Persistence (Phase 2+)

- Status: Accepted (updated Phase 3)
- Decision: Last-chosen routes from the dashboard matrix are saved in `config.yaml` under `routing_rules` and re-applied on graph refresh. Authored `rules[]` (Phase 3) take precedence by priority when both match.
- Constraints:
  - Stream rules key on composite identity (`app_name` + optional `executable` + optional `media_name`); device rules key on virtual sink `system_name`.
  - Authored rules are managed in the Rules view; dashboard explainability shows why each stream routed.
  - Full rule engine (priority, explainability, simulation) is implemented per `Rule_Engine_Spec.md`.
- Rationale: Users need routes to stick when ephemeral streams disappear without learning a separate rules vocabulary, while power users can author explicit policies that override implicit dashboard saves.

### PD-012 Phase 4 Restore Model

- Status: Accepted
- Decision: Virtual device definitions persist in `config.yaml` (`virtual_devices[]`); restore runs on app open by default and optionally at login via daemon.
- Constraints:
  - GUI and daemon share `restore.rs` and the same YAML contract.
  - Daemon safe mode: corrupt or missing config logs status and exits without creating devices.
  - Dashboard-saved routes display as **Manual route** in explainability; authored rules show rule name.
- Rationale: Survive reboots without forcing always-on services; keep automation labels user-readable.

### PD-014 Plugin API Transport

- Status: Accepted (Phase 5)
- Decision: Plugin host communicates via JSON-RPC 2.0 over stdin/stdout with newline-delimited messages.
- Constraints:
  - Request timeout 5 seconds; hung plugins are killed without blocking core routing.
  - `api_version: 1` in manifest must match host support.
- Rationale: Simple, language-agnostic, debuggable transport for subprocess isolation.

### PD-015 First-Party Effects

- Status: Accepted (Phase 5)
- Decision: Audio effects (EQ, compressor) ship as a first-party bundled plugin using PipeWire `filter-chain`; no EasyEffects dependency.
- Constraints:
  - Effects apply only to Pipe Deck-owned virtual devices (`pipe-deck-*`) in v1.
  - Plugin ships enabled by default; maintained in-tree.
- Rationale: Pipe Deck owns the audio stack; effects are core product value, not an external tool integration.

### PD-016 First-Party Audio Ownership

- Status: Accepted (Phase 5)
- Decision: Pipe Deck owns routing, effects, and virtual devices; external audio tool integrations (EasyEffects, OBS) are out of product scope.
- Constraints:
  - Community connector plugins may exist post-Phase 5 but never replace first-party paths.
  - Multi-output routing is a core engine feature (fan-out virtual sink + `pw-link`).
- Rationale: Pipe Deck is the Linux Audio Control Center, not a launcher for other audio tools.

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
