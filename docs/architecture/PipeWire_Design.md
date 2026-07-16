# PipeWire Design

## Purpose

Describe how Pipe Deck models and interacts with PipeWire while keeping operations understandable and safe for users.

## In Scope

- Discovery model for nodes, streams, devices, and links.
- Virtual device strategy.
- Automatic mapping direction and safety mechanisms.

## Out of Scope

- Exact Rust type definitions.
- Final PipeWire library bindings decision.
- Production-grade heuristics tuning values.

## Core Design Goals

- Make PipeWire state visible in user-friendly terms.
- Reduce manual sink/source setup for common workflows.
- Preserve explicit user control with easy rollback.

## Discovery Model

Pipe Deck should maintain a normalized runtime graph built from PipeWire updates:

- Physical devices (headset, HDMI, USB interface)
- Virtual devices (app-defined sinks/sources)
- Application streams (playback/capture clients)
- Active links and policy metadata

Normalization requirements:

- Stable internal IDs for UI and rules.
- Device capability flags (input/output/duplex).
- Human-readable labels with fallback naming strategy.

## Virtual Device Strategy

Implemented (Phase 4):

- Virtual devices persist in `config.yaml` (`virtual_devices[]`) on create/remove.
- Startup and optional daemon restore recreate `pipe-deck-{slug}` modules idempotently.
- Profile save captures `device_assumptions` for virtual devices; profile swap restores them first.

Near-term direction:

- Create virtual devices only when explicitly requested.
- Mark ownership metadata to support cleanup and restore logic.

Future direction:

- Managed virtual device sets tied to profiles beyond current assumption IDs.

## Phase 2 Routing Mechanics

Pipe Deck uses three routing hops depending on the target:

| User action | Mechanism | Notes |
|-------------|-----------|-------|
| App → sink | `pactl move-sink-input` | Standard per-application output routing |
| App → virtual mic | Hidden feed sink + `pw-link` | Feed sink `pipe-deck-feed-{slug}` is internal plumbing; labeled `{mic name} (Pipe Deck route)` in other apps |
| Virtual sink → output/mic | `pw-link` monitor→playback/input | e.g. Soundux Sink → headphones or virtual mic |

### Feed sinks

- Created on demand when routing playback to a virtual input.
- Hidden from the Pipe Deck UI; garbage-collected when idle or when the virtual mic is removed.
- Renaming a virtual mic updates the feed label when safe (no active stream on the feed).

### Discovery

- Primary graph from `pw-dump`; supplemented with `pactl` for stream targets and levels.
- Pipe Deck-owned virtual devices use stable `virtual-*` IDs from the in-app registry, not raw `node-*` IDs from `pw-dump`.
- Live refresh subscribes to `pw-dump -m` and pushes a `graph-updated` event to the frontend on change (`pipewire/live.rs`); falls back to a 1-second poll only if that monitor process can't be started.
- **Refresh coalescing policy** (#57): under high stream churn, `pw-dump -m` can emit many events in a burst. Rather than debouncing per-event (which still fires one full graph refresh per line, never letting the graph settle), the monitor drains events on a dedicated reader thread and the refresh loop waits for a 200ms quiet gap before pushing a single coalesced `graph-updated` — bounded by a 400ms hard cap so sustained churn still surfaces changes promptly rather than starving entirely. This keeps the UI update rate bounded while routing/mixer actions still reflect well within 500ms.
- **Frontend-side throttling** (#57): the backend's own coalescing bounds emission to roughly 2-5Hz worst case, but the frontend previously applied every `graph-updated` event unconditionally with a full reactive replacement (`useRuntimeGraph` in `src/stores/runtimeGraph.ts`). It now runs incoming events through a trailing-edge debounce with a max-wait ceiling (100ms wait / 150ms max-wait, `src/composables/useThrottledGraphUpdates.ts`) before writing to reactive state, bounding the resulting Vue Flow rebuild rate independent of how bursty the underlying event stream gets. Debounce+max-wait was chosen over `requestAnimationFrame` batching: it gives a fixed, deterministic budget (unit-testable with fake timers) rather than tying the update rate to paint cadence. Combined with the backend's own worst case, this still lands well inside the 500ms "routing apply reflects promptly" budget. See PD-023 in `Decisions.md`.

## Automatic Mapping (Long-Term Direction)

Objective: reduce manual sink/source mapping for common setups.

Directional heuristic sources:

- Device type and naming conventions.
- Application identity/category hints.
- Previously successful user-approved mappings.

Safety constraints:

- Never silently break active critical paths (e.g., default mic unexpectedly removed).
- Every automatic mapping should be explainable in UI.
- One-click revert to last known-good state.

## Failure and Recovery Model

- Failed link/create operations return explicit reasons.
- Partial operations should fail safely and preserve prior stable routes.
- Last known-good profile remains recoverable.

## Decisions

- Initial automatic mapping operates in suggest-first mode before broad auto-apply behavior.
- Auto-apply is allowed only after confidence and safety checks pass.
- Device disconnect handling uses a grace window and then falls back to last known-good route.

## Traceability to User Value

- Graph normalization -> less confusion in device lists.
- Managed virtual devices -> fewer repetitive setup steps.
- Explainable auto-mapping -> confidence without hidden behavior.

