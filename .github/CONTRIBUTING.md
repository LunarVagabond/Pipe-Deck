# Contributing

## Purpose

Contribution standards for Pipe Deck, with emphasis on clarity, safety, and mission alignment.

## Feature Proposal Gate

Each feature proposal or implementation should answer:

- Does this make Linux audio easier to understand and manage?

If no, refine or drop the proposal.

## Questions before you file

For quick questions or a sanity check before opening an issue, drop into [Discord](https://discord.gg/cHtuCFkRRm) (server name: "Dev Syndicate"). For anything worth keeping searchable — design discussion, proposals — use [GitHub Discussions](https://github.com/LunarVagabond/Pipe-Deck/discussions) instead.

## Contribution Principles

- User experience first.
- Keep PipeWire internals behind clear abstractions.
- Avoid breaking profile/config formats.
- Document public interfaces and behavior changes.
- Prefer simple, reversible behavior over clever complexity.

## Branching

- `main` — integration branch
- `<issue#>-short-description` — topic branches off `main`, named after the GitHub issue number (e.g. `42-submodule-detection`); no `feature/`, `bug/`, or similar prefix, the issue number is the lookup
- `noissue-short-description` — maintainer-only, mirroring the `[noissue]` commit/PR restriction below. If you see a branch like this, it's a maintainer hotfix, not a pattern open to other contributors

## Work Tracking

Open work lives in [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues). Browse in the UI or list locally:

```bash
gh issue list
```

Product direction and acceptance criteria remain in [`docs/product/Roadmap.md`](../docs/product/Roadmap.md). Completed history is in git; do not maintain a separate backlog file in the repo.

## Commits And Pull Requests

Open an issue first when the work is non-trivial. The issue carries context (feature, bug, scope) — commits and PRs reference it by number.

### Commit Messages

```
[#<issue>] - <short description>
```

**`[noissue]` is restricted.** It exists only for the maintainer and a small, explicitly-named set of trusted core developers to hotfix trivial things (typo, comment, one-line fix) without ticket overhead — it is the wrong way to handle most work, and is deliberately not available to general contributors or to AI agents. If you are not on that short list, every commit and PR needs a real issue number:

```
[noissue] - <short description>
```

Examples:

- `[#123] - Add bass slider to mixer panel`
- `[#123] - Wire bass slider to channel gain`
- `[noissue] - Fix typo in Contributing commit examples` (maintainer/core-only example)

Keep descriptions focused on **what changed** in that commit. Use the issue number from GitHub (`#123`) when one exists. One logical change per commit when practical.

### Pull Request Titles

Use the same pattern as commits:

```
[#123] - Add bass slider to mixer panel
[noissue] - Fix typo in README quick start
```

`[noissue]` follows the same restriction as commit messages above — maintainer and named core developers only. Everyone else opens an issue first and references it in the title. The PR body can go deeper on approach and testing.

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

- **Never use `[noissue]`, and never use a `noissue-*` branch name.** Both are restricted to the maintainer and a small named set of core developers — every commit, PR, and branch you make needs a real issue number. If no issue exists yet for the work, that's a sign to open one first, not to reach for `[noissue]`.
- Apply the `Co-Authored-By: <Tool> <email>` trailer above to every commit and PR you create or materially author.
- Don't add any other AI-attribution mention beyond that single trailer line (no extra notes in the commit body or PR description) unless explicitly asked to.
- If you're unsure whether the trailer applies in a given situation, ask rather than guessing.

## Documentation-First Workflow

For major work:

1. Update the relevant file in `docs/` first.
2. Align implementation tasks with accepted docs.
3. Update docs and behavior together on changes.

## Development Interface (Makefile)

Use `make` as the canonical interface for local development and build tasks.

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

- New here? Start at [Getting Started](../docs/project/Getting_Started.md) for prerequisites, clone, and first run.
- Codebase layout and dev workflow: [Development](../docs/project/Development.md)
- Product direction: [Product Requirements](../docs/product/Product_Requirements.md), [Roadmap](../docs/product/Roadmap.md), [Decisions](../docs/architecture/Decisions.md)
- Architecture: [System Architecture](../docs/architecture/System_Architecture.md), [PipeWire Design](../docs/architecture/PipeWire_Design.md)
- Specifications: [UI Spec](../docs/specs/UI_Spec.md), [Plugin API](../docs/specs/Plugin_API.md), [Config Spec](../docs/specs/Config_Spec.md)
- Contributor process: this file, and the rest of [`docs/README.md`](../docs/README.md)

`docs/` is a normal, PR-able part of this repo, organized into `specs/`, `architecture/`, `product/`, and `project/` subfolders — edit it the same way as any other change.

## Code Of Conduct

Participation in this project is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## OSS Onboarding Expectations

Contributions should include:

- Problem statement in plain language.
- Scope (in/out).
- Risks and rollback considerations.
- How this helps Linux audio become easier to understand/manage.
