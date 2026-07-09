# Backlog

## Baseline Gate

Every backlog item should answer yes to:

- Does this make Linux audio easier to understand and manage?

## Phase 2: Foundation Runtime and Core Flows

Work in this order. Earlier slices unblock later ones.

**Status:** Complete for milestone gate (2026-07-09). See `docs/product/Roadmap.md` for acceptance criteria and carry-over items.

### 2.1 Scaffold

- [x] Initialize Tauri 2 + Vue 3 + TypeScript project structure
- [x] Set up Rust core engine and Tauri command boundary
- [x] Define domain model types (Rust + TypeScript)
- [x] Wire Tauri IPC event stream for core → UI updates
- [x] Boot app on Linux with empty dashboard shell

### 2.2 Enumeration (Read-Only)

- [x] PipeWire adapter: enumerate via `pw-dump` (live session data)
- [x] Normalize nodes, ports, links into runtime graph model
- [x] Stable internal IDs for devices and streams
- [x] `get_runtime_graph` Tauri command
- [x] Dashboard view: list devices, streams, and active links
- [x] Live refresh when PipeWire state changes (poll every 1s via `pw-dump`)
- [ ] Replace `pw-dump` polling with native pipewire-rs event subscription

### 2.3 Profiles (File-First YAML)

- [x] Config store: load/save `config.yaml` and profile files
- [x] Profile validator with schema versioning
- [x] Profile save: capture current state → write YAML
- [x] Profile swapper: load → validate → apply → update active pointer
- [x] Rollback on failed profile apply
- [x] Export/import profile files (copy or simple archive)

### 2.4 Routing and Mixer

- [x] Per-application routing (apply routing intents)
- [x] Device-to-device routing (virtual sink → output or virtual mic)
- [x] Stream → virtual mic routing (feed sink + internal link)
- [x] Persist last-chosen routes in `config.yaml` (`routing_rules`)
- [x] Undo/rollback action for routing edits
- [x] Basic mixer panel with level state and mute control
- [x] Create virtual input
- [x] Create virtual output
- [x] Remove virtual devices
- [x] Rename devices (aliases sync to feed sink labels for virtual mics)
- [ ] Device icons and categories
- [ ] Multi-output routing
- [ ] Monitor paths
- [x] Contextual notifications

### 2.5 UX Polish

- [x] Routing connection lines in dashboard matrix
- [ ] Visual drag/connect routing editor
- [ ] First-run wizard
- [ ] Search

### 2.6 Packaging Baseline

- [x] Binary build via `cargo tauri build`
- [x] `.deb` package artifact (Tauri bundle target)
- [x] `.rpm` package artifact (Tauri bundle target)
- [x] Flatpak manifest and build pipeline
- [x] CI build matrix with install smoke tests
- [x] Document runtime dependencies per distro

## Phase 3: Rules and Advanced Routing UX

**Complete for milestone gate (2026-07-09).** See `docs/product/Roadmap.md` for acceptance criteria and carry-over items.

- [x] Match by executable
- [x] Match by app name
- [x] Match by window class (best-effort from PipeWire metadata)
- [x] Default categories
- [x] Define portable rule serialization format (`rules[]` in `config.yaml`)
- [x] Add rule simulation mode before apply
- [ ] Add rule conflict/fallback test matrix
- [ ] Add UI wireframe references
- [ ] Add interaction timing targets
- [ ] Define onboarding checklist and first-run helper behavior

Carry-over to later phases:

- [ ] Visual drag/connect routing editor
- [ ] Rule edit UI (create/delete/enable exist; edit deferred)
- [ ] Tray / system quick controls

## Phase 4: Persistence and Background Management

- [ ] Optional daemon for restore/background behavior
- [ ] Restore on login/session start
- [ ] Persistent virtual device lifecycle
- [ ] Add sequence diagrams for route change, profile restore, and auto-map workflows
- [ ] Add boundary-level test strategy (unit, integration, simulated PipeWire events)
- [ ] Add event lifecycle states and reconciliation strategy
- [ ] Define deterministic conflict resolution for competing mapping candidates
- [ ] Prototype and document safe-mode behavior for first-run environments
- [ ] Add config compatibility tests
- [ ] Production packaging hardening (repos, systemd, desktop integration)

## Phase 5: Ecosystem and Integrations

- [ ] OBS integration
- [ ] EasyEffects integration
- [ ] Plugin SDK
- [ ] Remote API/automation entrypoints
- [ ] Define plugin manifest schema
- [ ] Publish starter plugin template
- [ ] Add plugin review checklist for community maintainers

## Documentation and Project Follow-Ups

### Product and Planning

- [x] Phase 2 scaffold and packaging specs
- [x] File-first YAML config and profile swap contract
- [ ] Convert product requirements into versioned milestones
- [ ] Add measurable UX benchmarks (task completion time, misroute recovery time)
- [ ] Link requirements to implementation epics

### Project Process

- [x] Add Makefile as canonical development interface
- [ ] Add issue and PR templates
- [ ] Add architecture decision record (ADR) process
- [ ] Add release documentation workflow

## Open Questions / Ideas

- Which PipeWire Rust bindings to use (pipewire-rs vs alternatives)?
- What plugin signing and trust model should be required for default distribution?
- What constraints and review rules should govern plugin-contributed UI surfaces?
- Which PipeWire metadata fields are sufficiently stable across distributions for deterministic mapping?
- What confidence threshold should permit auto-apply versus suggest-only mapping behavior?
- Which config fields are immutable versus user-editable by policy?
- What minimum telemetry policy, if any, is acceptable while preserving user trust and privacy expectations?
- When (if ever) does SQLite become justified over file-first YAML?
