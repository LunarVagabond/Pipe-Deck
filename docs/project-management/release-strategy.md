# Release Strategy

Cadence and policy for when Pipe Deck ships. For how a release is actually cut (version bumps, tagging, CI, signing, publishing), see [`../project/Release.md`](../project/Release.md) — that page covers mechanics; this one covers timing.

## Cadence

Target a stable release roughly **every two weeks**.

## Skip rule

If a cycle would only include documentation updates or otherwise insignificant changes, it's fine to skip that release rather than cutting one just to hit the cadence. There's no value in shipping an empty or near-empty release on schedule — the cadence is a target, not a hard deadline. Whether a given cycle's changes are "insignificant" is a maintainer judgment call, not an automated gate, consistent with how the rest of this project's process runs.

## Hotfix policy

A hotfix — a critical bug, regression, or security issue — ships **immediately, outside the normal cadence**, rather than waiting for the next scheduled release. Use the existing hotfix tag-slug convention already documented in [`../project/Release.md`](../project/Release.md#tag-format) (e.g. `v0.2.0-hotfix-title`) — no separate process is needed beyond cutting the release as soon as the fix is ready.

## Milestones and cadence

Each release milestone should reflect what's realistically landing in that cycle. See [Milestones & Releases](milestones-and-releases.md) for how milestones are scoped and named.
