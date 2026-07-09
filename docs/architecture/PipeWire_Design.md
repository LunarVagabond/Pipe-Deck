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

