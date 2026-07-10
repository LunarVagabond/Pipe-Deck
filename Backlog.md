# Backlog

## Baseline Gate

Every backlog item should answer yes to:

- Does this make Linux audio easier to understand and manage?

## Active: Phase 6 — Consolidation and Core Polish

Work in this order unless a stability bug forces a detour. See `docs/product/Roadmap.md` for acceptance criteria.

**Navigation:** Keep Routing, Mixer, Sources, and Effects sidebar entries visible; leave disabled until each view ships (north-star map).

### 6.1 Stabilize (in progress / ongoing)

- [x] Sink-centric routing model (streams → sink; multi-output via virtual sink + `pw-link`)
- [x] Live-only graph refresh (no rule re-apply on poll)
- [x] Multi-output fan-out via monitor paths + per-target link management
- [x] Capture routing sync (Chromium mic dropdown / `pactl` capture targets)
- [x] Virtual device multi-word names (separate pactl module args)
- [x] Multi-sink mute/volume sync (sink + monitor, pactl levels after virtual merge)
- [x] Dashboard mixer strip (vertical faders, editable %, uniform card heights)
- [x] Profiles Save vs Apply clarity
- [x] Rules Apply rules + app identity matching + recent stream cache
- [x] End-to-end dogfood pass: document known gaps before enabling Mixer page

### 6.2 Dashboard polish

- [x] Routing connection lines (colored, directional, bezier tangents)
- [ ] Column grouping / collapse for dense setups
- [ ] Matrix zoom/pan (incremental; defer force-directed graph to Phase 8)
- [ ] Device icons and categories
- [ ] Search (devices, streams, rules)
- [ ] First-run wizard / onboarding checklist

### 6.3 Dedicated Mixer view

- [x] Extract mixer strip into full **Mixer** page
- [x] Enable Mixer sidebar item when page ships
- [x] Per-stream volume/mute (stretch)
- [x] Level meters (stretch)

### 6.4 Effects v0 (first vertical slice)

- [x] Effects panel UI: pick virtual device, 3-band EQ + compressor toggle
- [x] Wire `pipe-deck-effects` / host commands to `filter_chain.rs`
- [x] Persist chain config in profile or `config.yaml`
- [x] Graceful degradation when `module-filter-chain` / LADSPA unavailable

### 6.5 Infrastructure

- [x] Replace `pw-dump` polling with native PipeWire event subscription
- [x] Tray / system quick controls (optional in Phase 6)
- [x] `safeguards.fallback_policy` enforcement in rule evaluation

### 6.6 Distribution carry-over

- [ ] apt/rpm repository publishing
- [ ] Full Flatpak build in CI (manifest validation only today)

---

## Phase 2: Foundation Runtime and Core Flows

**Status:** Complete for milestone gate (2026-07-09).

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
- [ ] Replace `pw-dump` polling with native pipewire-rs event subscription → **Phase 6.5** (`pw-dump -m` monitor shipped; full pipewire-rs deferred)

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
- [x] Create virtual multi-output sink
- [x] Remove virtual devices
- [x] Rename devices (aliases sync to feed sink labels for virtual mics)
- [x] Multi-output routing (fan-out virtual sink + `pw-link`)
- [x] Contextual notifications
- [ ] Device icons and categories → **Phase 6.2**

### 2.5 UX Polish

- [x] Routing connection lines in dashboard matrix
- [ ] Visual drag/connect routing editor → **Phase 8**
- [ ] First-run wizard → **Phase 6.2**
- [ ] Search → **Phase 6.2**

### 2.6 Packaging Baseline

- [x] Binary build via `cargo tauri build`
- [x] `.deb` package artifact (Tauri bundle target)
- [x] `.rpm` package artifact (Tauri bundle target)
- [x] Flatpak manifest and build pipeline
- [x] CI build matrix with install smoke tests (`make smoke` in CI; full Flatpak build deferred)
- [x] Document runtime dependencies per distro

## Phase 3: Rules and Advanced Routing UX

**Status:** Complete for milestone gate (2026-07-09).

- [x] Match by executable
- [x] Match by app name
- [x] Match by window class (best-effort from PipeWire metadata)
- [x] Default categories
- [x] Define portable rule serialization format (`rules[]` in `config.yaml`)
- [x] Add rule simulation mode before apply
- [x] Rule edit UI (create, edit, delete, enable/disable)
- [ ] Add rule conflict/fallback test matrix → **Phase 8**
- [ ] Add UI wireframe references → **docs backlog**
- [ ] Add interaction timing targets → **docs backlog**
- [ ] Define onboarding checklist and first-run helper behavior → **Phase 6.2**
- [ ] Visual drag/connect routing editor → **Phase 8**
- [ ] Tray / system quick controls → **Phase 6.5**

## Phase 4: Persistence and Background Management

**Status:** Complete for milestone purposes (2026-07-09).

- [x] Optional daemon for restore/background behavior
- [x] Restore on login/session start
- [x] Persistent virtual device lifecycle
- [x] Add sequence diagrams for route change, profile restore, and auto-map workflows
- [x] Add boundary-level test strategy (unit, integration, simulated PipeWire events)
- [x] Add event lifecycle states and reconciliation strategy
- [x] Add config compatibility tests
- [x] Production packaging hardening (systemd, desktop integration, CI smoke; repos deferred)
- [ ] apt/rpm repository publishing → **Phase 6.6**
- [ ] Full Flatpak build in CI → **Phase 6.6**
- [ ] Define deterministic conflict resolution for competing mapping candidates → **Phase 8**
- [ ] Extended first-run wizard beyond daemon safe-mode exit → **Phase 6.2**

## Phase 5: Ecosystem and Integrations

**Status:** Scaffold complete for milestone purposes (2026-07-09). Effects product work continues in Phase 6–7.

- [x] Plugin runtime (subprocess host, JSON-RPC stdio, capability gate, audit log)
- [x] Plugin manifest schema and config `plugins:` block
- [x] Settings UI for plugin enable/disable and capability approval
- [x] `pipe-deck` CLI (graph, route, profile, rules, plugins)
- [x] First-party bundled `pipe-deck-effects` plugin (scaffold + nav panel)
- [x] `filter_chain.rs` backend (EQ + compressor hook)
- [x] Multi-output routing (fan-out virtual sink + `pw-link`)
- [x] Starter plugin template (`plugins/template/`)
- [x] Contributor docs (`docs/project/Plugins.md`, `Plugin_Review_Checklist.md`)

Removed from scope (PD-016 first-party audio ownership):

- OBS integration — out of product scope
- EasyEffects integration — replaced by first-party effects plugin

## Phase 7: First-Party Processing (planned)

- [ ] Per-device effect chain editor (order, bypass, reset)
- [ ] Balance / pan controls
- [ ] Multi-band EQ beyond 3-band MVP
- [ ] Dynamics suite (compressor, limiter, gates)
- [ ] Chains on physical sinks and capture inputs (not only `pipe-deck-*`)
- [ ] Profile-persisted chains across swap/restore
- [ ] Reduce reliance on host LADSPA plugins where practical

## Phase 8: Advanced Routing UX (planned)

- [ ] Dedicated **Routing** view (enable sidebar when shipped)
- [ ] Drag/connect or force-directed node graph editor
- [ ] Dedicated **Sources** view (capture-focused)
- [ ] Monitor path visualization
- [ ] Dedicated rule conflict-resolution UI

---

## Documentation and Project Follow-Ups

### Product and Planning

- [x] Phase 2 scaffold and packaging specs
- [x] File-first YAML config and profile swap contract
- [x] Phase 6–8 roadmap (consolidation, effects, advanced routing)
- [ ] Convert product requirements into versioned milestones
- [ ] Add measurable UX benchmarks (task completion time, misroute recovery time)
- [ ] Link requirements to implementation epics

### Project Process

- [x] Add Makefile as canonical development interface
- [ ] Add issue and PR templates
- [ ] Add architecture decision record (ADR) process
- [ ] Add release documentation workflow

## Open Questions / Ideas

Items with existing decision coverage are noted; others remain open for later phases.

| Question | Status |
|----------|--------|
| Which PipeWire Rust bindings to use (`pipewire-rs` vs alternatives)? | **Open** — evaluate during Phase 6.5 native subscription work |
| Plugin signing and trust model for default distribution? | **Open** — distribution hardening; see `docs/project/Packaging.md` |
| Constraints for plugin-contributed UI surfaces? | **Partial** — see `Plugin_Review_Checklist.md`; formalize host policy in Phase 6+ |
| Which PipeWire metadata fields are stable across distros for deterministic mapping? | **Ongoing** — v1 uses `app_name`, `executable`, `media_name`, `window_class` (best-effort); document per-distro gaps as found |
| Confidence threshold for auto-apply vs suggest-only? | **Decided (PD-007)** — suggest-first; broad auto-apply only with confidence + safety checks; explicit **Apply rules** for authored policies today |
| Which config fields are immutable vs user-editable by policy? | **Partial** — schema version + validator in `profile_store`; full policy table deferred |
| Minimum telemetry policy? | **Decided (PD-008)** — local explainability first; no mandatory telemetry |
| When does SQLite justify replacing file-first YAML? | **Decided (PD-009)** — only if indexing, concurrent writes, or daemon reconciliation require it; YAML stays the portable contract |
