# Project Management

How Pipe Deck organizes issues, epics, and releases on GitHub, and why.

## Why this exists

Milestones used to do two unrelated jobs at once: tracking what ships in a release (e.g. "v0.5.0") and tracking large, multi-release initiatives ("Phase 6 — Consolidation", "Quality & Platform", and similar roadmap-phase themes). Conflating the two made a simple question — "what ships next?" — hard to answer from the milestone list, since release-scoped and initiative-scoped issues sat side by side in the same bucket with no way to tell which was which without reading descriptions.

As of 2026-07-18, these are split along GitHub's native lines instead:

- **Milestones** answer "what ships in this release?" — nothing else.
- **Epics** (issues labeled `epic`) answer "what is this large initiative, and what's left to do?" — tracked via GitHub's native [sub-issues](https://docs.github.com/issues/tracking-your-work-with-issues/using-sub-issues) feature rather than a milestone or a manual checklist.
- **Issue relationships** (native Blocks/Blocked-by) capture real dependencies between issues, in place of prose like "blocks #123" in an issue body.

Every issue that was previously grouped under a roadmap-phase milestone has been re-parented as a native sub-issue of the corresponding Epic; those milestones are now closed (not deleted, so old links still resolve) and are no longer used.

## In this section

- [Issue Workflow](issue-workflow.md) — what an Epic and a Sub-Issue are, when to create a new Epic, how relationships work.
- [Milestones & Releases](milestones-and-releases.md) — what a milestone means now and when to create one.
- [Release Strategy](release-strategy.md) — cadence, skip rule, hotfix policy. See [`../developers/Release.md`](../developers/Release.md) for the mechanics of actually cutting a release.
- [Contributing Workflow](contributing-workflow.md) — how to find work and how new issues get organized. See [`.github/CONTRIBUTING.md`](../../.github/CONTRIBUTING.md) for branch/commit/PR conventions.
