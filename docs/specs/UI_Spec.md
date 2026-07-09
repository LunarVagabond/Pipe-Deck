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
- Theme tokens are defined as CSS custom properties in `src/styles/_variables.scss`.

See `docs/project/Contributing.md` for the contributor-facing layout and rules.

## UX Goal

Users should understand current audio state quickly and complete common routing tasks in a few steps.

## Decisions

- Default first-launch landing page is Dashboard.
- Routing edits apply immediately by default.
- Undo/rollback is a required safety mechanism for all routing edits.

## Primary Views

- Overview Dashboard: current outputs, inputs, active streams, quick actions.
- Devices: create/manage virtual inputs and outputs.
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
- Phase 3 adds rule/manual override explanations and jump-to-fix actions.

## Dashboard Layout (Phase 2)

The default dashboard uses a four-column routing matrix:

- **Applications** — active streams with per-stream **Route to** dropdown
- **Routing** — virtual sinks (including third-party sinks like Soundux) with **Route to** for device chains
- **Outputs** — hardware and virtual playback endpoints
- **Inputs** — hardware and virtual capture endpoints

Connection lines draw between linked nodes. There is no separate "saved routes" panel — persistence is implicit when the user changes a dropdown.

## Progressive Disclosure

- Default mode: simplified labels, guided actions, safe defaults.
- Advanced mode: detailed graph, explicit node/link controls, deeper diagnostics.

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

### Candidate Color Schemes

- Deep Indigo (default candidate): `#0B0F14`, `#131820`, `#1C2330`, `#7C5CFF`, `#26C3A3`, `#FFB020`, `#E6E9EF`, `#9AA4B2`
- Ocean Teal: `#0A0F12`, `#11171D`, `#182228`, `#00B4D8`, `#2ECC71`, `#F59E0B`, `#E6E9EF`, `#93A1AF`
- Carbon Purple: `#0C0E13`, `#151820`, `#1F2430`, `#8A55F7`, `#22D3EE`, `#FB7185`, `#E6E9EF`, `#A1A8B3`

### Page-Level Ideas Derived From Mockup

- Dashboard: application-to-routing matrix plus quick status bars for key sinks/sources.
- Routing: node-link visual editor with direct drag/connect behavior.
- Mixer: per-channel sliders with meters and mute/solo controls.
- Profiles: one-click profile switch with summary of included virtual sinks/sources and rules.
- Settings: compact toggles for startup behavior, tray mode, updates, and language.

## Traceability to User Value

- Overview clarity -> faster diagnosis of broken audio paths.
- Routing explanation panel -> easier trust and debugging.
- Profile workflows -> fewer repetitive setup tasks.

## Rules Explanation Detail

- Default UI shows concise explanation text.
- Expanded detail is available on demand for debugging and advanced workflows.
