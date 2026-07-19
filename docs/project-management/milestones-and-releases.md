# Milestones & Releases

## What a milestone means now

A milestone answers exactly one question: **what ships in this release?** Milestones are named after the version they track (e.g. `v0.5.0 — Beta Liftoff`) and hold only the issues actually scoped to that release.

Milestones are **not** used for roadmap phases, themes, or long-running initiatives anymore — that's what an [Epic](issue-workflow.md#what-an-epic-is) is for. If you're tempted to create a milestone for something open-ended ("Quality work", "Someday"), it should be an Epic instead; leave issues that don't have a concrete release target unmilestoned rather than parking them in a vague milestone.

## When to create a milestone

Create a new milestone when a release is actually being scoped — i.e. there's a real next version number and a rough idea of what's going into it. Give it a due date once one is set; an open-ended milestone with no due date and no clear membership isn't doing its job.

## Retired milestones

The milestones that previously stood in for epics (`Phase 6 — Consolidation`, `Phase 7 — Processing`, `Phase 8 — Advanced Routing`, `Quality & Platform`, `Ecosystem & Packaging`, `Documentation & Process`, `Routing Pipeline Hardening`, `Routing Graph — At-a-Glance Polish`, `Stretch — Cross-Platform Port`) were **closed, not deleted**, as part of the 2026-07-18 restructuring. Closing (rather than deleting) keeps old links resolvable and keeps the milestone's original description intact as a historical record, while removing it from the active-milestone list. Each one's issues are now native sub-issues of the corresponding `[Epic]` issue instead.

## Release mechanics

This page is about what a milestone *means* and when to create one. For how a release actually gets cut — version bumps, tagging, CI, signing, publishing — see [`../project/Release.md`](../project/Release.md). For the cadence and policy around *when* releases happen, see [Release Strategy](release-strategy.md).
