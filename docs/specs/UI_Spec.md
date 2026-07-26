# UI Spec

## Purpose

Define the user experience structure and interaction behavior that makes Linux audio routing understandable and manageable.

## In Scope

- Information architecture.
- Primary routing and profile workflows.
- Novice-to-power-user progression.

## Out of Scope

- Pixel-perfect visual design system.
- Framework-level component implementation details (except styling conventions below).

## Frontend Styling Convention

- Vue components contain template and script only; no `<style>` blocks.
- All presentation CSS lives in SCSS files under `src/styles/`.
- `src/styles/main.scss` is the single stylesheet entry imported by `src/main.ts`.
- Component/view styles use a root class namespace (for example `.mixer-strip`) with nested selectors.
- Theme tokens are defined as CSS custom properties in `src/styles/_variables.scss`. That file's `:root` block is the static pre-JS/failure-mode fallback only — the active color scheme overrides these same custom properties at runtime via `src/stores/theme.ts`. See [Theming](../specs/Theming.md) for the scheme system.

See [Contributing](../../.github/CONTRIBUTING.md) for the contributor-facing layout and rules.

## UX Goal

Users should understand current audio state quickly and complete common routing tasks in a few steps.

## Decisions

- Default first-launch landing page is Dashboard.
- Routing edits apply immediately by default.
- Undo/rollback is a required safety mechanism for all routing edits.

## Primary Views

- Overview Dashboard: current outputs, inputs, active streams, quick actions.
- Devices: create/manage virtual inputs and outputs — see "Virtual Devices as Busses" below for the mental model.
- Applications: per-app input/output assignment.
- Routing View: visual connection editor between apps and sinks/sources.
- Mixer View: per-device and per-stream levels/mute.
- Profiles View: save, load, compare, and restore known-good setups.
- Rules View: create and inspect auto-routing rules with explanations.
- Settings: global preferences and diagnostics controls.

## Primary Workflows

### Route an Application

1. Open Dashboard routing matrix.
2. Find the application stream in the **Applications** column.
3. Choose a target from **Route to** (output, virtual sink, or virtual mic).
4. Confirm the connection line and dropdown reflect the new route. Undo if needed.

### Save and Restore Profile

1. Capture current state.
2. Name profile and optional tags.
3. Restore on demand with conflict prompts when needed.

### Understand Why Audio Is Routed

- Current route is visible in the matrix (dropdown selection + connection line).
- Each application stream shows a collapsible **route explanation** panel: matched rule, match reasons, skipped candidates, and manual-override status.
- **Change route** in the panel focuses the stream's target dropdown.

### Recover an Off-Screen Node

- Right-click empty canvas → **Bring node here…** → pick a node from the list of every node currently on the board → the picked node relocates to the click point.
- Recovers a node dragged (or auto-laid-out) far outside the visible canvas, which otherwise has no other way to be located.

## Dashboard Layout

The default dashboard uses a four-column routing matrix:

- **Applications** — active streams with per-stream **Route to** dropdown
- **Routing** — virtual sinks (including third-party sinks like Soundux) with **Route to** for device chains
- **Outputs** — hardware and virtual playback endpoints
- **Inputs** — hardware and virtual capture endpoints

Connection lines draw between linked nodes. Authored policies are managed in the **Rules** view; dashboard dropdown changes also persist lightweight `routing_rules` at lower priority.

## Rules View (Phase 3)

Componentized under `src/components/rules/` (issue #277/#116 UX pass): `RuleListItem.vue` (one table row), `RuleConditionEditor.vue` (one condition row, reused inside the modal), and `RuleFormModal.vue` (the full create/edit dialog) — `Rules.vue` itself is the thin orchestrator (data loading, save/delete/toggle/reorder, search filtering).

- Full-width table of authored rules (name, conditions, target, live-match status, inline enable toggle, actions), sorted and displayed in priority order (highest first).
- A search box above the table filters by rule name, condition value, or target device name.
- Each row has an inline `ToggleSwitch` (reused from `components/ToggleSwitch.vue`) for enable/disable, and ▲/▼ priority-reorder buttons that swap the `priority` value with the adjacent rule and persist both — no drag-and-drop.
- A **Live match** badge per row ("Matching N now" / "No live match") is computed from `simulate_rules` results (`RouteExplanation.matched_rule_key === rule.name`), refreshed after every rule mutation and on load — a rule that never matches anything currently live is now visibly distinguishable from one that does, which is what makes the seen-set fix (PD-030) worth having a UI for.
- **+ New Rule** opens a centered modal for name, priority, target selection, and conditions.
- **Edit** reopens the same modal for rename, condition, and target changes.
- **Simulate** runs a dry-run preview without applying routes; its results also feed the per-row live-match badges above.
- Collapsible identity reference table helps fill condition values from active streams.

## Settings View (Phase 4)

- **Restore on startup:** Off/On toggle — recreate virtual devices and reapply routes when the app opens (default on).
- **Background restore:** Off/On toggle — install and enable `pipe-deck-daemon` user systemd service for login-time restore (default off).
- **Background service status:** enabled state, last run, devices restored, and last error from `daemon.json`.

## Route Explanation Labels

| Source | Dashboard summary |
|--------|-------------------|
| Authored rule | `Routed by {Rule Name} → {device}` |
| Dashboard-saved route (`routing_rules`) | `Routed manually → {device}` |
| Session manual override | `Manual choice this session` |
| No match | `No matching auto-route rule` |

A stream whose `action_status` is `blocked`, `skipped_manual_override`, or `target_unavailable` also gets a colored dot badge directly on its node in the Routing graph (not just inside the collapsible route-explanation panel), so a routing failure is visible without opening it — `blocked`/`skipped_manual_override` render as `--status-warning`, `target_unavailable` as the more severe `--status-danger`. The badge's tooltip reuses this same label table.

## Progressive Disclosure

- Default mode: simplified labels, guided actions, safe defaults.
- Advanced mode: detailed graph, explicit node/link controls, deeper diagnostics.

## Virtual Devices

Virtual output devices are a single, uniform kind now (PD-033, issue #293 — this supersedes PD-031's Bus/terminal-Output split described in earlier versions of this section). Virtual inputs are a separate, unaffected kind. Concretely:

- **A virtual output device is a true dead end**, symmetric with a physical output: apps/streams play into it, but it cannot route onward to anything, cannot host a device-attached effect, and renders with no output pin in the Routing graph — nothing can be dragged out of it. Combining multiple sources or fanning one signal out to several destinations is now exclusively the job of a dedicated processing node (see "Processing Nodes" below) — a Mixer node's or Fan-Out node's own output is what routes onward, not a plain device.
- **Virtual inputs stay leaves.** A virtual input (virtual mic) merges sources via `mix_sources`, but is never itself a routing source — this asymmetry is intentional, not an oversight: a virtual mic's job is to be the thing apps *consume*, not something that feeds further downstream. It can, however, still be fed by a Mixer node's output the same way any other destination can.
- **Creation is a two-way choice**: the "Create virtual device" dialog and the Routing graph's right-click "Add node" menu offer Output (virtual) and Input. Building a mix or a fan-out means adding a Mixer/Fan-Out processing node instead of choosing a device role.
- Under the hood, a virtual output is still the same `module-null-sink` primitive PipeWire always exposed with a monitor/output port — Pipe Deck's own permission layer (backend rejection plus the frontend never rendering the handle) is what makes it a dead end, not removing the underlying port. It does **not** change how the node is exposed to the rest of the system: another app can still pick it directly as a playback destination via PipeWire's own device list (tracked separately, issue #288).

## Processing Nodes

Mixer, Fan-out, 5-Band EQ, and a growing family of other effect kinds (PD-032, issue #293) are **dedicated, independently-wired graph nodes** — not attachments on an existing node. This section supersedes the "Effects as Attachments" framing this repo shipped with previously: that section's central rule, "an attachment never becomes its own node — if it ever does, that's a bug," is deliberately reversed here. See PD-032 for the full rationale, including why this doesn't reintroduce the per-connection failure mode PD-020 was written to avoid.

The mental model: a processing node is a first-class citizen of the routing graph, exactly like a device or a stream — you drop it onto the canvas, wire a source into its input, wire its output onward, and adjust its own parameters in place on the node itself. Nothing about "does this node exist" is implicit or hidden inside another node's disclosure panel.

Concretely, this means:

- **Adding a node is the primary entry point**, via the Routing graph's right-click "Add node" menu (mirroring how a virtual Output/Input is already added) — never a separate "effects setup" page you have to complete before anything routes. Mixer and Fan-out are fully functional; 5-Band EQ is fully functional real DSP; every other effect kind ships as a visibly-labeled **"Not implemented yet"** pass-through stub node — addable and wireable now, with real DSP landing per-kind in follow-up tickets.
- **Wiring is drag-and-drop, like any other node.** A Mixer's input count grows with each connection (one gain slider per connected source); a Fan-out's output count grows the same way. Both reject a would-be second single-slot connection (or an ambiguous removal) with a clear error rather than silently guessing — see PD-032's "ambiguous relink is rejected, never guessed" rule.
- **Adding a node is immediate, not provisional.** There's no separate "enable" step after adding — the deliberate act of adding the node (and, for a growable port, the deliberate act of connecting to it) is itself confirmation enough, the same PD-025 reasoning this framing inherits.
- **Removing a node is a single action.** Disconnecting all but at most one input and one output first is required (the ambiguous-relink guard above); once removable, nothing is left behind — no orphaned PipeWire object, no orphaned persisted config.
- **A plain device/stream's own Volume is still not a processing node.** It remains the node's own permanent, pinned property (`set_device_volume`/`set_stream_volume`, unchanged since PD-020) — a Mixer/EQ node's *own* parameters (per-input gain, EQ bands) live on the processing node itself, not layered onto some other node's Volume row.
- **Device-attached effects are now virtual-input (mic) only.** PD-033 retired device-attached effects on virtual output devices along with `VirtualRole::Bus` — the dedicated EQ5Band node is the only way to get EQ on an output-side signal now. The swap-by-identity attach/detach flow (`Device.effect_state`/`EffectChainConfig`) continues to work unchanged for virtual input devices. The pre-existing mic-mix mechanism (`Device.mix_sources`) is the one piece PD-032 explicitly generalizes into the Mixer Node — see PD-032 for its migration story.

## Usability Requirements

- Clear distinction between physical and virtual devices.
- Reversible actions for routing changes.
- No hidden automation without explanation.
- Fast visual feedback after each action.

## Accessibility and Clarity

- Keyboard-navigable core flows.
- Strong contrast and readable labels.
- Avoid jargon where plain language exists.

## Visual Reference (Initial Mockup)

- Reference asset: `docs/assets/mockups/InitialMockup.png`
- Visual language is dark-first with high-contrast accents and compact control density.
- Primary navigation model: Dashboard, Profiles, Rules, Routing, Mixer, Sources, Effects, Settings — all shipped and enabled (`src/App.vue`'s `navItems`). Earlier drafts of this doc described some of these as disabled "north-star" placeholders; that pattern is retired now that every primary view has shipped.

### Candidate Color Schemes

- Deep Indigo (default candidate): `#0B0F14`, `#131820`, `#1C2330`, `#7C5CFF`, `#26C3A3`, `#FFB020`, `#E6E9EF`, `#9AA4B2`
- Ocean Teal: `#0A0F12`, `#11171D`, `#182228`, `#00B4D8`, `#2ECC71`, `#F59E0B`, `#E6E9EF`, `#93A1AF`
- Carbon Purple: `#0C0E13`, `#151820`, `#1F2430`, `#8A55F7`, `#22D3EE`, `#FB7185`, `#E6E9EF`, `#A1A8B3`

### Page-Level Ideas Derived From Mockup

- Dashboard: application-to-routing matrix plus quick status bars for key sinks/sources.
- Routing: node-link visual editor with direct drag/connect behavior.
- Mixer: per-channel sliders with meters and mute/solo controls.
- Profiles: one-click profile switch with summary of included virtual sinks/sources and rules.
- Settings: restore-on-startup and optional background-restore toggles (Off/On), daemon status panel.

## Traceability to User Value

- Overview clarity -> faster diagnosis of broken audio paths.
- Routing explanation panel -> easier trust and debugging.
- Profile workflows -> fewer repetitive setup tasks.

## Rules Explanation Detail

- Default UI shows concise explanation text.
- Expanded detail is available on demand for debugging and advanced workflows.
