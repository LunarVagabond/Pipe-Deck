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
- Baseline packaging for binary, `.deb`, and `.rpm`.

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
- Packaging pipeline producing binary, `.deb`, and `.rpm` artifacts.

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

- Native PipeWire event subscription (still polling `pw-dump` at 1s) → **Phase 6**
- Multi-output routing → **delivered Phase 5**
- Monitor paths (visualization and dedicated Sources workflows) → **Phase 8**
- First-run wizard, search → **Phase 6**
- Full visual drag/connect routing editor (lines + dropdowns exist today) → **Phase 8**
- Rule engine UI, explainability, simulation → **delivered Phase 3**

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

### Phase 3 Status (2026-07-09)

**Complete for milestone purposes.** Core acceptance criteria above are met.

Delivered:

- Full `rules[]` model with priority, enable/disable, CRUD, and one-time migration from `routing_rules`
- Deterministic evaluator with explainability trace (`RouteExplanation` on each stream)
- Dashboard explainability panel (collapsed summary + expanded detail)
- Rules view: full-width table, **+ New Rule** modal, simulation preview
- Matchers: `app_name`, `executable`, `media_name`, `direction`, `category`, `regex`, `window_class` (best-effort from PipeWire metadata)
- Session manual-override tracking (cleared when user picks the same target the rule would apply)
- Dry-run `simulate_rules` command and UI
- Rule **edit** flow in Rules view (modal reuse for rename, conditions, and target)

Explicit carry-over (not Phase 3 blockers):

- Visual drag/connect routing graph editor → **Phase 8**
- Tray / system quick controls → **Phase 6**
- `safeguards.fallback_policy` enforcement in evaluation → **Phase 6**
- Dedicated conflict-resolution UI beyond skipped-candidate explanations → **Phase 8**

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

### Phase 4 Status (2026-07-09)

**Complete for milestone purposes.** Acceptance criteria above are met in the current codebase.

Delivered:

- `virtual_devices[]` in `config.yaml` with persist-on-create/remove and startup reconciliation
- `restore.rs` shared restore core (GUI + daemon): idempotent device recreate, orphan cleanup, route re-apply
- Profile `device_assumptions` captured on save; ordered virtual-device restore on profile swap
- `pipe-deck-daemon` binary, systemd user unit, Settings UI for background restore management
- Daemon status file at `~/.local/state/pipe-deck/daemon.json`
- Packaging: runtime deps, AppStream/desktop files, CI smoke + bundle jobs

Explicit carry-over (not Phase 4 blockers):

- apt/rpm repository publishing
- Native PipeWire event subscription (Phase 2 carry-over) → **Phase 6**

**Phases 1–5 milestone gates passed. Phase 6 is active.**

## Phase 5: Ecosystem and Integrations

### Scope

- Plugin ecosystem and extension capabilities.
- External API/CLI surfaces for automation.
- First-party audio features (effects, multi-output routing).

### Deliverables

- Isolated plugin runtime model with explicit capability controls.
- Stable extension and integration contracts.
- Contributor documentation for extension lifecycle and compatibility.
- `pipe-deck` CLI for scripting.
- Bundled first-party Effects plugin.

### Acceptance Criteria

- Third-party extensions cannot compromise core routing stability.
- Extension behavior is transparent and permission-scoped.
- Multi-output routing works for playback streams (undo + profile restore).

### Phase 5 Status (2026-07-09)

**Scaffold complete for milestone purposes.** Plugin host, CLI, and multi-output routing are production-usable. First-party effects are **not** product-complete yet.

Delivered:

- Plugin host: manifest discovery, JSON-RPC stdio, capability gate, audit log, crash isolation
- `plugins:` config block with enablement and granted capabilities
- Settings UI for plugin management
- `pipe-deck` CLI binary with JSON output
- Bundled `pipe-deck-effects` plugin (reference example of the plugin API contract; the Effects nav entry itself is first-party and no longer depends on plugin registration — see #22)
- Core multi-output routing via virtual sink fan-out + `pw-link` monitor paths
- `filter_chain.rs` backend hook (3-band EQ + compressor via PipeWire `module-filter-chain`, `pipe-deck-*` only)
- `plugins/template`, `docs/Plugins.md`, `Plugin_Review_Checklist.md`

Explicit out of scope (PD-016):

- OBS / EasyEffects external integrations

Explicit carry-over (see Phase 6–7):

- Wire Effects UI to `filter_chain.rs` (Effects v0)
- Full first-party processing suite (EQ, balance, dynamics, per-device chains on sinks and inputs)

**Phase 6 is the active delivery phase.**

## Phase 6: Consolidation and Core Polish

### Scope

- Make the current product trustworthy for daily use before large new subsystems.
- Incremental Dashboard and mixer improvements without over-scoping a graph rewrite.
- One bounded effects vertical slice on virtual devices.
- Keep disabled sidebar destinations visible as a north-star map (Routing, Mixer, Sources, Effects); do not remove them until each view ships.

### Implementation sequence

1. **Stabilize** — routing, multi-sink fan-out, virtual device naming, mute/volume sync, profile save vs apply clarity.
2. **Dashboard polish** — matrix grouping, wire clarity, collapsible sections; defer force-directed / Obsidian-style graph to Phase 8.
3. **Mixer expansion** — dedicated **Mixer** view (extract dashboard strip, more room for channels); per-stream controls and meters later.
4. **Effects v0** — per virtual device: 3-band EQ + compressor toggle via `filter_chain.rs`; minimal Effects panel UI.
5. **Infrastructure** — replace 1s `pw-dump` polling with native PipeWire event subscription when ready.

### Deliverables

- Dashboard reliable enough for daily routing and level control.
- Dedicated Mixer page (sidebar item enabled when shipped).
- Effects v0 on `pipe-deck-*` virtual devices with profile-persisted chain config.
- Native PipeWire subscription (or documented fallback if blocked).

### Acceptance criteria

- Multi-output virtual sinks route, mute, and unmute correctly end-to-end.
- Virtual device display names with spaces work in UI and system audio lists.
- User can set volume via slider or typed percent; mute state matches PipeWire.
- Effects v0 applies and removes a filter chain on at least one virtual device without breaking routing.
- Disabled nav items remain visible but inert until their view ships.

### Carry-over from earlier phases (scheduled here)

- Native PipeWire event subscription (Phase 2)
- Device icons and categories (Phase 2)
- Search and first-run wizard (Phase 2)
- Tray / system quick controls (Phase 3)
- `safeguards.fallback_policy` enforcement (Phase 3)
- apt/rpm repository publishing (Phase 4)

## Phase 7: First-Party Processing

### Scope

- Full in-house effects stack (PD-015, PD-016): balancers, EQ, dynamics, faders, layered chains.
- Attach processing to virtual devices first; expand to physical sinks and capture inputs.
- No dependency on EasyEffects or other third-party audio control apps.
- PipeWire `filter-chain` and/or bundled DSP; minimize reliance on ad-hoc host LADSPA where practical.

### Deliverables

- Per-device effect chain editor (order, bypass, reset).
- Balance / pan and multi-band EQ beyond Effects v0.
- Dynamics (compressor, limiter) with sensible defaults.
- Profile persistence for device chains.
- Effects sidebar view fully enabled with first-party UI (not plugin stub only).

### Acceptance criteria

- User can build a chain on a virtual output or input without leaving Pipe Deck.
- Chains survive profile swap and virtual device restore.
- Processing failures degrade gracefully; core routing keeps working.

## Phase 8: Advanced Routing UX

### Scope

- Dedicated views for routing patterns that outgrow the Dashboard matrix.
- Optional advanced graph layout (including force-directed / free-position node graph).
- Capture-focused **Sources** workflow.

### Deliverables

- **Routing** view: drag/connect or advanced graph editor between apps, virtual sinks, and endpoints.
- **Sources** view: capture devices, per-app mic routing, monitor paths.
- Optional progressive disclosure: simplified Dashboard default, advanced graph mode on demand.
- Rule conflict UI beyond skipped-candidate explanations.

### Acceptance criteria

- User can complete common routing tasks from Dashboard; power users can use Routing view for complex topologies.
- Graph interactions are reversible and explainable (undo + route explanation).

## Strategic Direction

- Automatic mapping should progressively reduce manual sink/source setup.
- Automation must remain safe, explainable, and reversible.
- Dashboard remains the default hub; dedicated views add depth without replacing it.

## Navigation model

- **Enabled today:** Dashboard, Profiles, Rules, Settings; Effects when a plugin registers a panel.
- **Visible, disabled (north star):** Routing, Mixer, Sources — intentional placeholders showing where the product is headed; enable each when its view ships (Phase 6–8).
- See `docs/UI_Spec.md` for per-view intent.

## Decisions

- Phase 2 follows scaffold → enumeration → profiles → routing/mixer → packaging baseline.
- Persistence is file-first YAML (no SQLite in Phase 2); SQLite remains a future option if indexing or daemon needs justify it (PD-009).
- Optional `pipe-deck-daemon` ships in Phase 4 for login-time restore; GUI-only restore remains the default path.
- Earliest implementation milestone focuses on device enumeration, routing, mixer, and profile save/load/swap.
- Rule engine and advanced automation are post-initial milestone work.
- Automatic mapping rollout is gated by explainability, safety checks, and rollback readiness (PD-007).
- Incremental Dashboard matrix polish precedes a full Obsidian-style graph (Phase 8 optional).
- First-party effects expand in Phase 7 after Effects v0 proves the `filter-chain` path in Phase 6.
