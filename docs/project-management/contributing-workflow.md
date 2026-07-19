# Contributing Workflow: Finding and Organizing Work

This page is about how issues, epics, and milestones fit together when you're picking up or filing work. For branch naming, commit/PR title format, and the `[noissue]` restriction, see [`.github/CONTRIBUTING.md`](../../.github/CONTRIBUTING.md) — nothing here duplicates that.

## Finding work

- **Browse initiatives**: `gh issue list --label epic --state open` lists current Epics — each one's Sub-issues panel shows what's left.
- **Browse near-term, release-scoped work**: `gh api repos/{owner}/{repo}/milestones --jq '.[] | select(.state=="open") | "\(.number)\t\(.title)"'` lists open (release) milestones; `gh issue list --milestone "<title>"` lists what's in one.
- **Good first issue** / **help wanted** labels still work as before, independent of the epic/milestone split.

## Organizing a new issue

When filing or triaging an issue, decide independently:

1. **Does it belong to an existing initiative?** If so, it should become a sub-issue of that `[Epic]`. Attaching a sub-issue currently requires either the web UI's "Add sub-issue" button on the Epic, or a `gh api graphql` call — see [Issue Workflow](issue-workflow.md#how-a-new-issue-gets-organized) for the exact commands. There's no `gh issue` subcommand for this in the CLI version this repo uses (2.45.0).
2. **Is it scoped to a specific upcoming release?** If so, set the milestone to that release. If not, leave the milestone unset — don't guess a release just to fill the field.

These two decisions are independent; an issue can have both, one, or neither. See [Issue Workflow](issue-workflow.md#how-a-new-issue-gets-organized) for a worked example.

## Real dependencies between issues

If picking up an issue reveals it's genuinely blocked by another, use GitHub's native Blocked-by/Blocks relationship rather than a comment — see [Issue Workflow](issue-workflow.md#issue-relationships-blocks--blocked-by).
