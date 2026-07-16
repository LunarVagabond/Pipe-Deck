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

See `docs/Contributing.md` for the contributor-facing layout and rules.

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

## Dashboard Layout

The default dashboard uses a four-column routing matrix:

- **Applications** — active streams with per-stream **Route to** dropdown
- **Routing** — virtual sinks (including third-party sinks like Soundux) with **Route to** for device chains
- **Outputs** — hardware and virtual playback endpoints
- **Inputs** — hardware and virtual capture endpoints

Connection lines draw between linked nodes. Authored policies are managed in the **Rules** view; dashboard dropdown changes also persist lightweight `routing_rules` at lower priority.

## Rules View (Phase 3)

- Full-width table of authored rules (name, conditions, target, status, actions).
- **+ New Rule** opens a centered modal for name, priority, target selection, and conditions.
- **Edit** reopens the same modal for rename, condition, and target changes.
- **Simulate** runs a dry-run preview without applying routes.
- Collapsible identity reference table helps fill condition values from active streams.

## Settings View (Phase 4)

- **Restore on startup:** Off/On toggle — recreate virtual devices and reapply routes when the app opens (default on).
- **Background restore:** Off/On toggle — install and enable `pipe-deck-daemon` user systemd service for login-time restore (default off).
- **Background service status:** enabled state, last run, devices restored, and last error from `daemon.json`.
- Flatpak installs: background restore may be unavailable; in-app restore remains supported.

## Route Explanation Labels

| Source | Dashboard summary |
|--------|-------------------|
| Authored rule | `Routed by {Rule Name} → {device}` |
| Dashboard-saved route (`routing_rules`) | `Routed manually → {device}` |
| Session manual override | `Manual choice this session` |
| No match | `No matching auto-route rule` |

## Progressive Disclosure

- Default mode: simplified labels, guided actions, safe defaults.
- Advanced mode: detailed graph, explicit node/link controls, deeper diagnostics.

## Virtual Devices as Busses

A virtual output/input is presented to the user as a **bus**, not merely "another sink/source you create": something that groups sounds, can carry an effect chain (see "Effects as Attachments" below), and feeds onward to another bus or a final physical output — the same mental model as a submix on a mixing desk (see "Bus"/"Submix" in `Audio_Terminology.md`). Concretely:

- **Virtual outputs can chain into other virtual outputs** (e.g. a "Game Audio" submix feeding a "Master Mix" bus), not just into a physical output or a virtual mic. The Routing graph's `targetsForVirtualSink` allows this.
- **Virtual inputs stay leaves.** A virtual input (virtual mic) merges sources via `mix_sources`, but is never itself a routing source — this asymmetry is intentional, not an oversight: a virtual mic's job is to be the thing apps *consume*, not something that feeds further downstream.
- Under the hood both kinds are still the same `module-null-sink` primitive, differing only in exposed `media.class` (PD-020/PD-024) — the bus framing is a UI/mental-model choice, not a new data type. `DeviceKind`/`DeviceDirection` are unchanged.

## Effects as Attachments

Effects (PD-020, PD-024, PD-025) are presented to the user as **attachments** on a node, not as separate objects in the graph and not as a settings page you configure ahead of time. The mental model: pick a node, attach an effect to it, adjust it in place, detach it when you don't want it anymore — the same shape as attaching a file to a message, not provisioning infrastructure.

Concretely, this means:

- **Attach where you are.** The primary attach point is a right-click on the node itself (Routing graph) or a per-channel disclosure (Mixer) — never a separate "effects setup" step you have to complete before the node does anything. `Effects.vue` remains as a flat-list alternative for users who'd rather not hunt across the graph, but it drives the same underlying attachments, not a separate configuration path.
- **Attaching is immediate, not provisional.** There's no "enable" step after attaching — the deliberate act of attaching an effect is itself confirmation enough (PD-025). A one-time toast informs, it doesn't gate.
- **Detaching is a single action, fully reversible, with nothing left behind.** No orphaned config, no separate "disable" vs "remove" distinction to reason about.
- **An attachment never becomes its own node.** The underlying mechanism may create additional PipeWire plumbing (e.g. the `effect_output.*`/`effect_input.*` swap-by-identity nodes), but that's implementation detail — it must never surface as a second visible node the user has to route around. If it ever does, that's a bug (see the `effect_output.pipe-deck-mixer` graph-visibility fix logged alongside PD-025).
- **Volume is not an attachment.** It's the node's own permanent, pinned property (PD-020) — attachments render below it, addable/removable/reorderable among themselves, never displacing Volume.

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
- Primary navigation model: Dashboard, Routing, Mixer, Sources, Effects, Profiles, Settings.
- **North-star navigation:** Routing, Mixer, and Sources appear in the sidebar before their dedicated views ship. They stay visible but disabled so the product direction is obvious; enable each item when its view reaches acceptance criteria (see `docs/Roadmap.md` Phase 6–8).

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
