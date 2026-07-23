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
- Implementing advanced DSP processing in Phase 1.
- Platform parity beyond Linux is not a near-term commitment. Linux is, and remains, the only fully-supported platform today. A macOS/Windows port is tracked as a long-term, unscheduled direction (see the **Stretch — Cross-Platform Port** milestone and [PD-019](../architecture/Decisions.md) for the target APIs and parity scope) — not a promise against a delivery phase.

## Phase Boundaries and Implementation Status

Phase-by-phase scope, delivery status, and current active phase live in [Roadmap](../product/Roadmap.md) — that document is the single source of truth for delivery sequencing, kept current as phases complete. This document defines *what* Pipe Deck is and *why*; it deliberately does not duplicate *when* each phase ships, since that detail has drifted stale here in the past.

Open, in-flight, and backlog work beyond the roadmap phases is tracked as [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues) against the milestones listed on the project board (Phase 6–8, Documentation & Process, Ecosystem & Packaging, Quality & Platform, and the long-term Stretch — Cross-Platform Port milestone).

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
