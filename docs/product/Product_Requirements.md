# Product Requirements

## Product Identity

Pipe Deck is the **Linux audio mixer and control center** for PipeWire — the open-source desktop app for routing, mixing, profiles, virtual devices, and automation on Linux.

Pipe Deck exists to help everyday users and power users alike better understand and manage their Linux audio, and to help the community build and maintain the tools that make that possible.

## Mission

Help users route, organize, and control Linux audio without learning PipeWire internals.

## Feature Acceptance Filter

Every feature must pass this question:

- Does this help users better understand and manage their audio, or help the community build and maintain the tools that make that possible?

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
- A complete first-party DSP processing suite as a day-one deliverable — full EQ/dynamics/balance processing is ongoing work, tracked as [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182).
- Platform parity beyond Linux is not a near-term commitment. Linux is, and remains, the only fully-supported platform today. A macOS/Windows port is tracked as a long-term, unscheduled direction (see [Epic #185](https://github.com/LunarVagabond/Pipe-Deck/issues/185) and [PD-019](../architecture/Decisions.md) for the target APIs and parity scope) — not a promise against a delivery date.

## Delivery Status and Implementation Tracking

This document defines *what* Pipe Deck is and *why*; it deliberately does not track *when* things ship, since that detail has drifted stale here in the past.

Concrete delivery status lives on GitHub, not in a docs file: **milestones** track what ships in a specific release, and **epics** (`epic`-labeled issues with native sub-issues) track large, multi-release initiatives. See [Project Management](../project-management/README.md) for the full model. [Roadmap](../product/Roadmap.md) covers strategic direction and delivery history, including a lookup table from the old numbered "Phase 6+" names to their current epics.

## Long-Term Goal: Automatic Mapping

Long-term, Pipe Deck should reduce or eliminate manual sink/source assignment for common setups.

Directional intent:

- Detect available PipeWire nodes and known usage patterns.
- Apply safe default mappings for common scenarios.
- Allow clear override and rollback when heuristics are wrong.

This is a strategic direction, not an initial/day-one deliverable.

## Success Criteria

- New contributors can understand product scope in under 15 minutes.
- Core flows map directly to real Linux audio pain points.
- Every planned feature can be justified through the feature acceptance filter.

## Decisions

- First-run experience centers on Dashboard as the default landing page.
- Core defaults include practical audio categories; user-defined labels remain supported.
- Diagnostics prioritize local explainability and troubleshooting over mandatory telemetry.
