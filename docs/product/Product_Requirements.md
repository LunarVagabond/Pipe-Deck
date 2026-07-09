# Product Requirements

## Product Identity

Pipe Deck is the Linux Audio Control Center.

Pipe Deck exists to make Linux audio easier to understand and manage for everyday users and power users alike.

## Mission

Help users route, organize, and control Linux audio without learning PipeWire internals.

## Feature Acceptance Filter

Every feature must pass this question:

- Does this make Linux audio easier to understand and manage?

If no, the proposal requires refinement before inclusion.

## Problem Statement

Linux audio is powerful but fragmented. Users often need multiple tools and low-level concepts to perform routine tasks such as:

- Routing app audio to desired outputs.
- Managing microphones and virtual devices.
- Restoring preferred layouts across sessions.

Pipe Deck should reduce setup time, reduce confusion, and increase confidence.

## Personas

- Casual Linux user: wants audio to work without deep setup.
- Gamer: needs fast routing and profile switching.
- Streamer/creator: needs repeatable multi-app routing.
- OSS power user: wants control, visibility, and scriptability.

## Core User Jobs

- Understand what audio devices and streams exist right now.
- Route applications to the right sink/source quickly.
- Save known-good setups and restore them reliably.
- Trust that changes are reversible and safe.

## Product Principles

- PipeWire-first.
- Linux-native UX.
- Sensible defaults.
- Safe and reversible actions.
- Visual-first workflows.
- Open source contributor friendliness.

## Non-Goals

- Replacing PipeWire.
- Implementing advanced DSP processing in Phase 1.
- Building platform parity beyond Linux.

## Phase Boundaries

## Phase 1: Documentation Foundation

- Define product and system specs.
- Align terminology and architecture boundaries.
- Prepare implementation-ready documentation.

No implementation commitments are part of this phase.

## Implementation Status (summary)

Phases 1–4 are complete for milestone purposes (see `docs/product/Roadmap.md`):

- **Phase 2:** Desktop app, routing, profiles, mixer, baseline packaging
- **Phase 3:** Rule engine, explainability, simulation, rule edit UI
- **Phase 4:** Virtual device persistence, optional daemon restore, packaging hardening

## Future Implementation Phases (Directional)

- Plugin ecosystem and external automation (Phase 5).
- Native PipeWire event subscription and advanced routing editor (carry-over).

## Long-Term Goal: Automatic Mapping

Long-term, Pipe Deck should reduce or eliminate manual sink/source assignment for common setups.

Directional intent:

- Detect available PipeWire nodes and known usage patterns.
- Apply safe default mappings for common scenarios.
- Allow clear override and rollback when heuristics are wrong.

This is a strategic direction, not a Phase 1 deliverable.

## Success Criteria

- New contributors can understand product scope in under 15 minutes.
- Core flows map directly to real Linux audio pain points.
- Every planned feature can be justified through the feature acceptance filter.

## Decisions

- First-run experience centers on Dashboard as the default landing page.
- Core defaults include practical audio categories; user-defined labels remain supported.
- Diagnostics prioritize local explainability and troubleshooting over mandatory telemetry.
