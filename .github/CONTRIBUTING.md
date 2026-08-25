# Contributing

## Purpose

Contribution standards for Pipe Deck, with emphasis on clarity, safety, and mission alignment.

## Feature Proposal Gate

Each feature proposal or implementation should answer:

- Does this help users better understand and manage their audio, or help the community build and maintain the tools that make that possible?

If no, refine or drop the proposal.

## Questions before you file

For quick questions or a sanity check before opening an issue, drop into [Discord](https://discord.gg/cHtuCFkRRm) (server name: "Dev Syndicate"). For anything worth keeping searchable — design discussion, proposals — use [GitHub Discussions](https://github.com/LunarVagabond/Pipe-Deck/discussions) instead.

## If A Convention Gets In The Way

The branching model, commit format, and process rules below are a starting point, not a settled standard — assembled from what's worked elsewhere, not handed down from experience running an OSS project. Follow them as written. But if one is genuinely getting in the way of a contribution, doesn't fit a situation, or just seems off, raise it first — a [Discussion](https://github.com/LunarVagabond/Pipe-Deck/discussions) or a note in [Discord](https://discord.gg/cHtuCFkRRm) — before working around it. Same goes for friction in the tools, the codebase, or the workflow generally: surfacing it is always welcome. The goal is to talk it through and adjust the rule if it's wrong, not to greenlight quietly deviating from it.

## Contribution Principles

- User experience first.
- Keep PipeWire internals behind clear abstractions.
- Avoid breaking profile/config formats.
- Document public interfaces and behavior changes.
- Prefer simple, reversible behavior over clever complexity.

## Branching

- `main` — integration branch
- `<issue#>-short-description` — topic branches off `main`, named after the GitHub issue number (e.g. `42-submodule-detection`); no `feature/`, `bug/`, or similar prefix, the issue number is the lookup
- `noissue-short-description` — maintainer-only, mirroring the `[noissue]` commit/PR restriction below. If you see a branch like this, it's a maintainer quick fix, not a pattern open to other contributors
- `hotfix-short-description` — maintainer-only, mirroring the `[hotfix]` commit/PR restriction below. If you see a branch like this, it's a maintainer hotfix, not a pattern open to other contributors

## Work Tracking

Open work lives in [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues). Browse in the UI or list locally:

```bash
gh issue list
```

Product direction lives in [`docs/product/Roadmap.md`](../docs/product/Roadmap.md); acceptance criteria for specific initiatives live on their tracking issue/epic, not in a docs file. Completed history is in git; do not maintain a separate backlog file in the repo.

### Claiming An Issue

Before starting work, comment `/claim` on the issue — a bot assigns it to you automatically, which is what actually reserves it, so someone else doesn't start the same ticket in parallel. If an issue is already assigned, treat it as taken; comment to ask if it looks stalled instead of opening a competing PR. Epics don't work this way — find the specific sub-issue you want and `/claim` that instead.

*If it's a longer-running ticket, you don't have to post progress updates, but it's nice to leave one now and then so we know it's still moving — a claimed issue that's been quiet for 10 days gets an automatic ping, and is unassigned automatically 4 days after that if there's still no activity, so someone else can pick it up.*

A CI check (`claim-check.yml`) enforces this: it reads the issue number(s) your PR closes (via a closing keyword like `Closes #123` in the PR body) and fails the check if you aren't assigned to every one of them — whether that's because nothing was referenced, the referenced issue was never claimed, or it references a different issue than the one you actually claimed. `[noissue]`/`[hotfix]` titles skip this check, but only for PR authors with write access to the repo (the maintainer/named-core-dev list this format is already restricted to) — everyone else needs a real issue reference regardless of title.

## Commits And Pull Requests

Open an issue first when the work is non-trivial. The issue carries context (feature, bug, scope) — commits and PRs reference it by number.

### Commit Messages

Merges into `main` are **squash-only** — your branch's individual commits never appear in `main`'s history, only the squashed PR title does (see [Pull Request Titles](#pull-request-titles) below, which *is* strict). Because of that, commit messages on your branch are a suggested convention, not a requirement: write them however helps you work, `wip`/`fixup`/whatever included.

If you'd like to follow the convention anyway (it makes review easier, and CI leaves a non-blocking hint if a commit doesn't match), it's the same pattern as PR titles:

```
[#<issue>] - <short description>
```

**`[noissue]`, `[hotfix]`, and `[security]` are restricted.** All three exist only for the maintainer, a small, explicitly-named set of trusted core developers, and (for `[security]`/`[noissue]`) Dependabot. If you are not on that short list, use your issue number when you do tag commits. The tags mean different things:

- `[noissue]` — trivial, no ticket is warranted at all (typo, comment, one-line fix). Also what Dependabot's routine scheduled dependency bumps carry.
- `[hotfix]` — must be fixed now and there's a clear path to the fix, but there wasn't time to write up a ticket first. Reaching for this signals "this was a real bug/issue," not "there was nothing to file."
- `[security]` — a fix for a known vulnerability, most commonly Dependabot's security-triggered updates (automatically retitled from its default, unprefixed title into this — see `dependabot-security-title.yml`), occasionally a manual CVE/advisory fix.

```
[noissue] - <short description>
[hotfix] - <short description>
[security] - <short description>
```

Examples:

- `[#123] - Add bass slider to mixer panel`
- `[#123] - Wire bass slider to channel gain`
- `[noissue] - Fix typo in Contributing commit examples` (maintainer/core-only example)
- `[hotfix] - Guard against null device id crashing the mixer` (maintainer/core-only example)
- `[security] - Bump libwebkit2gtk to patch CVE-2026-XXXXX` (maintainer/core-only, or automated via Dependabot)

A CI job flags non-matching commit messages as a hint in the check output, but it never fails the PR — only the [PR title](#pull-request-titles) check is a blocking gate.

### Pull Request Titles

**This one is a hard requirement, unlike commit messages above.** Merges are squash-only, so the PR title becomes the actual commit message on `main` — it's the one place this format has to be right.

```
[#123] - Add bass slider to mixer panel
[noissue] - Fix typo in README quick start
[hotfix] - Guard against null device id crashing the mixer
[security] - Bump libwebkit2gtk to patch CVE-2026-XXXXX
```

`[noissue]`, `[hotfix]`, and `[security]` follow the same restriction as commit messages above — maintainer, named core developers, and Dependabot only. Everyone else opens an issue first and references it in the title. The PR body can go deeper on approach and testing.

A CI check enforces this title format — a malformed title fails the check rather than waiting on a reviewer to catch it by eye.

### AI-Assisted Contributions

AI coding assistants are welcome as a tool — this is not the same as "vibe coding" (accepting AI output wholesale without understanding or reviewing it). If an assistant materially helped with a commit, tag it with a trailer so it's easy to trace later, without cluttering the subject line:

```
git commit -m "[#42] - add submodule detection" --trailer "Co-Authored-By: Claude <noreply@anthropic.com>"
git commit -m "[#7] - correct pagination offset" --trailer "Co-Authored-By: GitHub Copilot <noreply@github.com>"
git commit -m "[#88] - simplify router registration" --trailer "Co-Authored-By: Cursor <noreply@cursor.com>"
```

This is optional and about being open, not a requirement — reviewers still hold the contributor responsible for understanding and standing behind the change either way.

#### If You Are An AI Agent Reading This

Follow the conventions in this file the same as any contributor would: `[#<issue>] - <short description>` commit and PR titles, one logical change per commit, docs updated alongside behavior changes. In addition:

- **Never use `[noissue]` or `[hotfix]`, and never use a `noissue-*` or `hotfix-*` branch name.** All are restricted to the maintainer and a small named set of core developers — every commit, PR, and branch you make needs a real issue number. If no issue exists yet for the work, that's a sign to open one first, not to reach for `[noissue]`/`[hotfix]`.
- Apply the `Co-Authored-By: <Tool> <email>` trailer above to every commit and PR you create or materially author.
- Don't add any other AI-attribution mention beyond that single trailer line (no extra notes in the commit body or PR description) unless explicitly asked to.
- If you're unsure whether the trailer applies in a given situation, ask rather than guessing.
- **Never reach for `eslint-disable`, `prettier-ignore`, `#[allow(clippy::...)]`, or `#[rustfmt::skip]` just to make a check pass.** See [Linting and Formatting](#linting-and-formatting) — a suppression without a genuine, specific justification comment is not an acceptable way to close out a lint/format failure; fix the underlying code instead, or ask if the rule itself seems wrong.

## Documentation-First Workflow

For major work:

1. Update the relevant file in `docs/` first.
2. Align implementation tasks with accepted docs.
3. Update docs and behavior together on changes.

## Development Interface (Makefile)

Use `make` as the canonical interface for local development and build tasks.
Python 3.11 or newer is required for the dependency target guard run by `make check`.

- Run `make help` to list available commands.
- Prefer adding new recurring CLI workflows as Makefile targets instead of documenting one-off shell commands.
- Wrap npm, cargo, and tauri commands in Make targets so contributors have one consistent entry point.

Current targets include:

| Command | Purpose |
|---------|---------|
| `make install` | Install frontend dependencies |
| `make start` / `make dev` | Run desktop app in development mode |
| `make dev-frontend` | Run Vite frontend only |
| `make build` | Production desktop bundles (.deb, .rpm, AppImage, binary) |
| `make build-frontend` | Type-check and build Vue frontend |
| `make build-daemon-dev` | Build the restore daemon binary (debug) |
| `make build-cli` | Build the `pipe-deck` CLI binary (debug) |
| `make build-rust` | Compile Rust backend (debug), via `build-daemon-dev` + `build-cli` |
| `make check` | Frontend + Rust checks, no bundles produced |
| `make test` | Rust tests |
| `make test-e2e` | Frontend Playwright component tests (`src/e2e/`; run `npx playwright install chromium` once first) |
| `make preview` | Preview the built frontend assets |
| `make smoke` | Run install and compile smoke checks |
| `make clean` | Remove build artifacts |
| `make release VER=<x.y.z>` | Maintainer-only: version bump + tag + release; not part of the standard contributor loop |
| `make help` | List every available target with its one-line description |

Set `PIPE_DECK_USE_MOCK=1` only when you need the static sample graph (e.g. UI work without PipeWire).

The Rust backend links `libpipewire` directly (native effects transport, see `docs/architecture/Decisions.md` PD-027) as of #149, so building/testing needs `libpipewire-0.3` dev headers installed (`pkg-config` finds them) in addition to the usual Tauri prerequisites — e.g. `libpipewire-0.3-dev` on Debian/Ubuntu, `pipewire-devel` on Fedora.

When introducing a new developer-facing command (for example lint, format, or packaging), add a documented Make target in the root `Makefile` and mention it here if it is part of the standard workflow.

### Known Dev-Environment Warnings

`Xlib: extension "DRI2" missing on display ":1"` — a WebKitGTK/X11 warning, not a Pipe Deck bug. It appears when the webview's GPU-accelerated compositing path probes for a DRI2 GLX extension the X server doesn't expose (common in VMs, nested/remote X sessions, or software-only graphics stacks). It's cosmetic stderr noise and doesn't affect functionality. If it's distracting, run with software compositing instead of disabling it repo-wide (which would degrade contributors with working GPU acceleration):

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 make start
```

## Linting and Formatting

`make check` runs `cargo clippy -D warnings`, `cargo fmt --check`, `npm run lint` (ESLint), and `npm run format:check` (Prettier) — all four are blocking in CI (`ci.yml`). None of `eslint-disable`, `prettier-ignore`, `#[allow(clippy::...)]`, or `#[rustfmt::skip]` are banned outright, but each one silences a check that exists for a reason, so **every suppression needs a comment on the line above (or immediately before the disabled block) explaining why the rule genuinely doesn't apply here** — not just that it was in the way. "this pattern is an intentional two-way binding into a parent's draft array" is a real justification; no comment, or a comment that just restates what the disable does, is not.

Reviewers should treat an unexplained suppression as a request to fix the underlying code, not to approve the bypass — including when the suppression was added by an AI coding assistant. A tool reaching for a disable comment to make a check pass faster is exactly the failure mode this rule exists to catch.

## Frontend Styling

Pipe Deck uses **SCSS stylesheets only** for frontend presentation.

- Do **not** add `<style>` blocks to Vue components (`.vue` files are template + script only).
- Put styles in `src/styles/`, mirroring the component/view layout where practical.
- Import styles once from `src/styles/main.scss`; `src/main.ts` loads that entry file.
- Use a root class per view/component (for example `.dashboard`, `.routing-matrix`) and nest selectors under it to avoid global leakage.
- Shared tokens live in `src/styles/_variables.scss` (CSS custom properties).
- Prefer SCSS nesting and partials over duplicated selectors.

Example layout:

```
src/styles/
  main.scss              # single entry; @use partials
  _variables.scss        # theme tokens
  _base.scss             # reset and global element rules
  app.scss                # app shell
  views/
    _dashboard.scss
  components/
    _routing-matrix.scss
    _mixer-strip.scss
```

When adding a new view or component with custom styling, create or extend the matching SCSS partial and wire it into `main.scss`.

## Where To Contribute

- New here? Start at [Getting Started](../docs/developers/Getting_Started.md) for prerequisites, clone, and first run.
- Codebase layout and dev workflow: [Development](../docs/developers/Development.md)
- Product direction: [Product Requirements](../docs/product/Product_Requirements.md), [Roadmap](../docs/product/Roadmap.md), [Decisions](../docs/architecture/Decisions.md)
- Architecture: [System Architecture](../docs/architecture/System_Architecture.md), [PipeWire Design](../docs/architecture/PipeWire_Design.md)
- Specifications: [UI Spec](../docs/specs/UI_Spec.md), [Plugin API](../docs/specs/Plugin_API.md), [Config Spec](../docs/specs/Config_Spec.md)
- Contributor process: this file, and the rest of [`docs/README.md`](../docs/README.md)

`docs/` is a normal, PR-able part of this repo, organized into `specs/`, `architecture/`, `product/`, and `developers/` subfolders — edit it the same way as any other change.

## Code Of Conduct

Participation in this project is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## OSS Onboarding Expectations

Contributions should include:

- Problem statement in plain language.
- Scope (in/out).
- Risks and rollback considerations.
- How this helps Linux audio become easier to understand/manage.
