# Roadmap

## Purpose

Define phased delivery goals and acceptance criteria while keeping scope aligned with the core mission.

## Mission Anchor

Pipe Deck is the Linux Audio Control Center.

All roadmap items must improve Linux audio clarity, control, or reliability for users.

## Phase 1: Documentation Foundation

- Canonical documentation under `docs/`.
- Product, architecture, and specs aligned.
- Contributor-friendly structure for OSS onboarding.

## Phase 2: Foundation Runtime and Core Flows (Initial Implementation)

### Scope

- Scaffold Tauri + Vue (TypeScript) desktop application.
- Enumerate PipeWire devices and streams (read-only first).
- Route applications to chosen targets.
- Mixer controls for core channels.
- Save, load, and swap YAML profile files.
- Temporary virtual device workflows.
- Dashboard-first UX with immediate apply and rollback.
- Baseline packaging for binary, `.deb`, `.rpm`, and Flatpak.

### Implementation Sequence

1. **Scaffold** — Tauri app shell, Vue TypeScript frontend, Rust core engine boundary.
2. **Enumeration** — PipeWire discovery pipeline and normalized runtime graph; read-only dashboard UI.
3. **Profiles** — YAML file-based desired state, profile swapper, save/load, export/import.
4. **Routing and mixer** — Apply routing intents, basic mixer panel, undo/rollback.
5. **Packaging baseline** — Installable dev/beta artifacts per target family. See [Packaging](../project/Packaging.md).

### Deliverables

- Tauri + Vue TypeScript app that boots on Linux.
- Stable runtime graph for physical devices, virtual devices, and application streams.
- Read-only dashboard showing live enumerated PipeWire state.
- Working routing UI for per-application target selection.
- Basic mixer panel with visible level state and mute control.
- Profile create/load/swap flow backed by separate YAML profile files.
- Error and recovery messaging for failed routing operations.
- Packaging pipeline producing binary, `.deb`, `.rpm`, and Flatpak artifacts.

### Acceptance Criteria

- App boots and shows live enumerated PipeWire entities without manual config edits.
- Route changes apply successfully and are reversible.
- Profile swap reloads desired state from YAML and re-renders UI; failed apply rolls back with actionable errors.
- Export/import works by copying profile files or a simple archive.
- Packaging produces at least one testable artifact per target family.
- Failures surface actionable messages rather than silent errors.

### Phase 2 Status (2026-07-09)

**Complete for milestone purposes.** Acceptance criteria above are met in the current codebase.

Delivered beyond the original minimum:

- Device-to-device routing (virtual sink → hardware output or virtual mic via `pw-link`)
- Stream → virtual mic routing (hidden feed sink + auto link)
- Lightweight route persistence in `config.yaml` (`routing_rules`) re-applied when apps return
- Dashboard routing matrix with dropdown targets and connection lines

Explicit carry-over to later phases (not Phase 2 blockers):

- Native PipeWire event subscription (still polling `pw-dump` at 1s)
- Multi-output routing, monitor paths, first-run wizard, search
- Full visual drag/connect routing editor (lines + dropdowns exist today)
- Rule engine UI, explainability, simulation (see Phase 3)

**Phase 3 is ready to start.**

## Phase 3: Rules and Advanced Routing UX

### Scope

- Rule engine with deterministic evaluation and explainable outcomes.
- Visual routing interactions for advanced editing.
- Automatic routing by reliable identifiers.
- Optional tray/system quick controls.

### Deliverables

- Rule authoring/editing interface with priority and conflict handling.
- Explainability panel showing why a route was chosen.
- Visual graph editing for routing paths.
- Rule simulation path before applying high-impact changes.

### Acceptance Criteria

- Rule outcomes are deterministic and auditable.
- Manual override behavior is consistent with spec.
- Users can identify and resolve routing conflicts quickly.

## Phase 4: Persistence and Background Management

### Scope

- Persistent virtual device lifecycle.
- Optional daemon for restore/background behavior.
- Restore on login/session start.
- Packaging and distribution hardening (production-ready install paths).

### Deliverables

- Reliable device/profile restoration path across reboots and reconnects.
- Daemon boundary implemented only where persistence requires background ownership.
- Production packaging hardening for primary Linux distribution targets.

### Acceptance Criteria

- Persistent routes survive expected restart scenarios.
- Background behavior is observable and failure-tolerant.
- Packaging/install paths produce consistent startup behavior across distributions.

## Phase 5: Ecosystem and Integrations

### Scope

- Plugin ecosystem and extension capabilities.
- External API/CLI surfaces for automation.
- Integration work (for example OBS/EasyEffects) based on clear user demand.

### Deliverables

- Isolated plugin runtime model with explicit capability controls.
- Stable extension and integration contracts.
- Contributor documentation for extension lifecycle and compatibility.

### Acceptance Criteria

- Third-party extensions cannot compromise core routing stability.
- Extension behavior is transparent and permission-scoped.
- Integration features map to verified user workflows.

## Strategic Direction

- Automatic mapping should progressively reduce manual sink/source setup.
- Automation must remain safe, explainable, and reversible.

## Decisions

- Phase 2 follows scaffold → enumeration → profiles → routing/mixer → packaging baseline.
- Persistence is file-first YAML (no SQLite in Phase 2); SQLite remains a future option if indexing or daemon needs justify it.
- Daemon remains deferred until Phase 4 persistence requirements.
- Earliest implementation milestone focuses on device enumeration, routing, mixer, and profile save/load/swap.
- Rule engine and advanced automation are post-initial milestone work.
- Automatic mapping rollout is gated by explainability, safety checks, and rollback readiness.
