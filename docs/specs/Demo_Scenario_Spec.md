# Demo Scenario Spec

## Purpose

Define a checked-in file format for a named, deterministic mock graph —
a "scenario" — that `MockAudioBackend` can load instead of its single
hardcoded `sample_graph()`. Scenarios are the foundation the rest of the
[Reproducible Demo Framework epic](https://github.com/LunarVagabond/Pipe-Deck/issues/365)
builds on: the same podcast/streaming/game-mix setup, loaded once, backs a
screenshot, a GIF, a release video, and eventually an end-to-end regression
test.

## In Scope

- Scenario file format, location, and versioning.
- What a scenario author writes vs. what the loader derives.
- How `MockAudioBackend` expands a scenario into a full `RuntimeGraph`.
- Validation expectations.

## Out of Scope

- The loading mechanism's env var / CLI surface (`PIPE_DECK_MOCK_SCENARIO=<path>`,
  covered by issue #368).
- The actual canonical scenario files (issue #370).
- Level-meter simulation, window orchestration, OBS control, and the demo
  runner (separate sub-issues under the epic) — a scenario file only
  describes a graph, nothing about how it's captured or recorded.

## Why not extend `Profile`?

`Profile` (see [Config Spec](Config_Spec.md)) is deliberately thin: it holds
`routing_intents`/`volume_state`/`device_assumptions` keyed against IDs in an
**already-existing** graph — it presupposes the devices and streams it
references are already live, it doesn't define them. A scenario has no
existing graph to presuppose; it has to originate the devices, streams, and
virtual-mic-mix wiring from nothing. That's a `RuntimeGraph`-shaped problem,
not a `Profile`-shaped one, so a scenario file is a new, separate schema —
not a `Profile` extension.

It also isn't a literal serialized `RuntimeGraph`, though. That struct
carries several fields that are *derived*, not authored — `current_target`/
`current_targets` on a `Device` are redundant with `links`, `is_monitor` is
computed at link-creation time, `data_source`/`notice` are backend-set. Hand
duplicating those in every scenario file would recreate exactly the
drift-prone duplication issue #366 just removed. A scenario file therefore
authors the minimal set of facts and leaves everything computable to the
loader.

## Format and Location

- **Format:** YAML, matching every other Pipe Deck config/profile file
  (`serde_yaml`, already a project dependency via `ProfileStore`).
- **Location:** `scenarios/*.yaml` at the repo root — a sibling of
  `scripts/`, not under `src-tauri/` (never compiled in) or `docs/` (data,
  not documentation). `PIPE_DECK_MOCK_SCENARIO` accepts any path, so a
  contributor can point at a scenario file anywhere; `scenarios/` is just
  where the canonical, checked-in ones live.
- **Schema version:** required `version: 1` at the top level, same
  fail-fast convention as `config.yaml`/profiles.

## Schema

```yaml
version: 1
id: podcast
name: Podcast recording
description: >
  Two hosts on a voice call, mixed to a single recording bus, with a
  separate filtered-mic monitor path back to headphones.

devices:
  - id: sink-headphones
    label: Headphones
    kind: physical
    direction: output
  - id: sink-record-mix
    label: Record Mix
    kind: virtual
    direction: output
    sink_mode: single
  - id: source-mic
    label: Microphone
    kind: physical
    direction: input
  - id: source-mic-filtered
    label: Mic (Filtered)
    kind: virtual
    direction: input
    mix_sources:
      - device_id: source-mic
        volume_percent: 100

streams:
  - id: stream-discord
    app_name: Discord
    executable: discord
    direction: playback
  - id: stream-obs
    app_name: OBS
    executable: obs
    direction: capture

routes:
  - from: stream-discord
    to: sink-record-mix
  - from: sink-record-mix
    to: sink-headphones
  - from: source-mic-filtered
    to: stream-obs
  - from: source-mic
    to: source-mic-filtered

# Optional — omit entirely for scenarios that don't need one (PD-032
# processing nodes: Mixer/Fan-out/EQ/etc). Ports are referenced by index,
# not position, same as ProcessingNodePort in core/models.rs.
processing_nodes: []
```

### Top-level fields

| Field | Required | Notes |
|-------|----------|-------|
| `version` | yes | Schema version; `1` today. |
| `id` | yes | Stable slug, used to derive `PIPE_DECK_MOCK_SCENARIO` file naming and future demo-runner `SCENARIO=<id>` lookups. |
| `name` | yes | Human-readable, shown anywhere a scenario is picked from a list. |
| `description` | no | Free text; narration/chapter-marker material for future GIF/video tooling (kept optional so a minimal scenario stays minimal). |
| `devices` | yes (may be empty) | See below. |
| `streams` | yes (may be empty) | See below. |
| `routes` | yes (may be empty) | See below. |
| `processing_nodes` | no | Same shape as `ProcessingNode` (`core/models.rs`) minus `id`/`system_name`, which the loader generates the same way `MockAudioBackend`'s existing virtual-device helpers do. |

### `devices[]`

Same fields as `Device` (`core/models.rs`), minus everything the loader
derives:

| Field | Required | Notes |
|-------|----------|-------|
| `id` | yes | Also used as `system_name` and as the id other entries reference in `routes`/`mix_sources`. |
| `label` | yes | |
| `kind` | yes | `physical` \| `virtual`. |
| `direction` | yes | `input` \| `output` \| `duplex`. |
| `sink_mode` | no | `single` \| `multi`; only meaningful for `virtual` outputs. |
| `mix_sources` | no | Same shape as `MixSource` (`device_id`, optional `volume_percent` defaulting to 100, optional `muted`). |
| `volume_percent`, `muted` | no | Default to `70` / `false` if omitted — matches `MockAudioBackend`'s existing sample-graph defaults. |

Not authored: `current_target`, `current_targets`, `sample_rate`,
`channels` — all derived or left unset.

### `streams[]`

| Field | Required | Notes |
|-------|----------|-------|
| `id` | yes | |
| `app_name` | yes | |
| `executable` | no | |
| `direction` | yes | `playback` \| `capture`. |
| `media_name`, `window_class` | no | |
| `volume_percent`, `muted` | no | Same defaults as devices. |

Not authored: `system_name` (defaults to `id` unless the scenario needs them
distinct), `current_target` (derived from `routes`), `is_system` (always
`false` for scenario-authored streams — real system streams aren't
something a demo scenario should need to fake), `route_explanation` (the
loader fills in a neutral `no_rule`/`no_action` explanation, matching how a
freshly-seen stream looks before any rule evaluates it).

### `routes[]`

Each entry is one edge: `{ from: <id>, to: <id> }`, where `<id>` is any
device or stream id declared above. The loader expands `routes` into:

- `RuntimeGraph.links`, generating each `Link.id` as `link-<from>-<to>` and
  setting `is_monitor` based on whether `from` is a virtual sink's monitor
  path (same rule `MockAudioBackend`'s current hand-written links follow).
- Each device's `current_target` (single) and `current_targets` (all
  targets it currently routes to), by collecting every route whose `from`
  matches that device's id.

A scenario author never writes `current_target`/`current_targets`/`links`
directly — this is the single mechanical step that removes the duplication
`Profile`/raw-`RuntimeGraph` authoring would otherwise require.

## Loading Semantics (for issue #368)

When `MockAudioBackend` loads a scenario file instead of its hardcoded
`sample_graph()`:

1. Parse and validate against this schema (see Validation below).
2. Expand `devices`/`streams`/`routes`/`processing_nodes` into a full
   `RuntimeGraph`, filling every derived field described above.
3. Set `data_source: "mock"` and a `notice` string naming the loaded
   scenario (e.g. `"Scenario: Podcast recording. Unset
   PIPE_DECK_MOCK_SCENARIO to use the default sample graph."`) — the demo
   runner and screenshot tooling that want a "clean" capture already strip
   `data_source`/`notice` before injecting graph data into their own shims
   (see issue #366), so this doesn't regress those.
4. Fall back to the existing hardcoded `sample_graph()` when
   `PIPE_DECK_MOCK_SCENARIO` is unset — existing tests/screenshots that
   assume the default graph are unaffected.

## Validation Requirements

- Reject a scenario whose `version` isn't a value this build understands,
  with an actionable error (matching `Config_Spec.md`'s config/profile
  validation convention).
- Reject a `routes[]` entry referencing an `id` not declared in `devices`/
  `streams` — a scenario with a dangling route is a bug in the scenario
  file, not something the loader should silently drop.
- Reject a `mix_sources[].device_id` that doesn't reference a declared
  device, for the same reason.
- No two `devices[]`/`streams[]` entries may share an `id` (they share one
  id namespace, since `routes[]` addresses both uniformly).

## Decisions

- Scenario schema is `RuntimeGraph`-shaped, not a `Profile` extension (see
  "Why not extend `Profile`?" above) — resolves the epic's open question #1.
- A scenario is an authoring-time format that the loader *expands*, not a
  literal serialized `RuntimeGraph` — avoids reintroducing the
  duplication/drift problem issue #366 fixed.
- Canonical scenario files live in a new top-level `scenarios/` directory,
  not under `src-tauri/` or `docs/`.

## Traceability to User Value

- A deterministic, checked-in scenario is what makes a demo video
  reproducible run to run, and what makes a screenshot/GIF asset regenerable
  without hand-editing fake data — directly serves the "get more people
  using Pipe Deck" goal behind this epic.
- Same schema doubles as future end-to-end regression test fixtures once
  issue #375's driving-the-real-app spike lands, without a second format to
  maintain.
