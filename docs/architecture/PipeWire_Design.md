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

Near-term direction:

- Create virtual devices only when explicitly requested.
- Mark ownership metadata to support cleanup and restore logic.

Future direction:

- Managed virtual device sets tied to profiles.
- Optional daemon-managed persistence lifecycle.

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
- Graph refresh polls every 1 second (native PipeWire subscription deferred).

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

