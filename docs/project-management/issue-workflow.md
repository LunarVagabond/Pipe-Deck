# Issue Workflow: Epics, Sub-Issues, and Relationships

## What an Epic is

An Epic is a normal GitHub issue with the `epic` label, titled `[Epic] <name>` (e.g. `[Epic] Quality & Platform`). It represents a large initiative that's bigger than one release and typically spans several — the kind of work that used to live in a themed milestone ("Phase 7 — Processing", "Ecosystem & Packaging", and so on).

An Epic's body carries a description of the initiative. Its actual scope and progress live in its **Sub-issues** panel (GitHub's native sub-issue feature), not in a manual checklist in the body — a checklist drifts out of sync with reality; the native panel can't, because it's the same open/closed state as the issues themselves.

List current epics:
```bash
gh issue list --label epic --state open --json number,title
```

## What a Sub-Issue is

A Sub-Issue is any issue attached underneath an Epic (or, occasionally, underneath another issue that itself functions as a grouping issue — a two-level hierarchy is fine when a chunk of an Epic's work has its own natural sub-grouping). GitHub renders "Tracked by #N" on a sub-issue and shows a live progress roll-up on its parent.

A sub-issue can have exactly **one** parent. If you try to attach an issue that's already a sub-issue of something else, GitHub will reject it — that's a sign the issue already has a home; don't work around it by detaching and reattaching without checking why it was where it was first.

## When to create a new Epic

Open a new `[Epic]` issue when a body of work:
- is clearly bigger than a single release, or
- doesn't yet have an obvious release target but is worth tracking as a standing initiative, or
- groups several existing/planned issues that don't already sit under an Epic.

Don't create an Epic for a single feature or bug that fits in one issue — that's just an issue. Don't create a new Epic without checking `gh issue list --label epic` first for one that already fits; ask before assuming a new one is needed if it's ambiguous.

## How a new issue gets organized

Milestone (release) and Epic (initiative) are **independent axes** — an issue can carry both, either, or neither:

- A bug filed against the current release cycle with no clear larger initiative: milestone only (e.g. `v0.5.0`), no Epic.
- A feature that's part of a larger initiative but not yet scheduled for a specific release: sub-issue of an Epic, no milestone.
- A feature that's both part of a larger initiative *and* scheduled to ship in a specific release: sub-issue of the Epic **and** carries that release's milestone at the same time.

Example: an issue improving Flatpak packaging is a sub-issue of `[Epic] Ecosystem & Packaging` (the initiative) and, once it's actually scheduled, also gets the `v0.6.0` milestone (the release) — both at once, not one or the other.

Attaching a sub-issue: the gh CLI in this repo (2.45.0) has no dedicated subcommand for sub-issues, so it's either the web UI's "Add sub-issue" button on the Epic, or `gh api` directly:

```bash
# 1. Resolve both node IDs
gh api graphql -f query='
  query { repository(owner:"LunarVagabond", name:"Pipe-Deck") {
    epic: issue(number: <EPIC_NUM>) { id }
    child: issue(number: <NEW_ISSUE_NUM>) { id }
  } }'

# 2. Attach
gh api graphql -f query='
  mutation($epicId:ID!, $childId:ID!) {
    addSubIssue(input:{issueId:$epicId, subIssueId:$childId}) { subIssue { number } }
  }' -f epicId="<EPIC_NODE_ID>" -f childId="<CHILD_NODE_ID>"
```

## Issue relationships (Blocks / Blocked by)

Use GitHub's native Blocked-by/Blocks relationship (the "Blocked by" section in the issue sidebar — same place sub-issues show) when a real dependency exists between two issues, instead of describing it only as prose like "blocks #123" in the body. Native relationships show up as structured data on both issues and are visible without reading through comment history.

This is a **going-forward convention only**. Historical issues were not swept for dependency language and retroactively linked as part of the epic migration — that would have required interpreting free-text phrasing across ~140 issues, which is error-prone enough that it was deliberately left out of scope. If you notice a real dependency on an older issue while working on it, feel free to add the native relationship then.
