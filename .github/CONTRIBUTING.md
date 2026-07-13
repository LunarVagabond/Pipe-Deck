# Contributing

## Purpose

Contribution standards for Pipe Deck, with emphasis on clarity, safety, and mission alignment.

## Feature Proposal Gate

Each feature proposal or implementation should answer:

- Does this make Linux audio easier to understand and manage?

If no, refine or drop the proposal.

## Contribution Principles

- User experience first.
- Keep PipeWire internals behind clear abstractions.
- Avoid breaking profile/config formats.
- Document public interfaces and behavior changes.
- Prefer simple, reversible behavior over clever complexity.

## Branching

- `main` — integration branch
- `feature/<name>` — topic branches off `main`

## Work tracking

Open work lives in [GitHub Issues](https://github.com/LunarVagabond/Pipe-Deck/issues). Browse in the UI or list locally:

```bash
gh issue list
```

Product direction and acceptance criteria remain in [`docs/product/Roadmap.md`](../docs/product/Roadmap.md). Completed history is in git; do not maintain a separate backlog file in the repo.

## Commits and pull requests

Open an issue first when the work is non-trivial. The issue carries context (feature, bug, scope) — commits and PRs reference it by number.

### Commit messages

```
[#<issue>] - <short description>
```

For small maintainer fixes with no issue (typo, comment, trivial cleanup):

```
[noissue] - <short description>
```

Examples:

- `[#123] - Add bass slider to mixer panel`
- `[#123] - Wire bass slider to channel gain`
- `[noissue] - Fix typo in Contributing commit examples`

Keep descriptions focused on **what changed** in that commit. Use the issue number from GitHub (`#123`) when one exists. One logical change per commit when practical.

### Pull request titles

Use the same pattern as commits:

```
[#123] - Add bass slider to mixer panel
[noissue] - Fix typo in README quick start
```

Reserve `[noissue]` for quick maintainer fixes that do not need a ticket. The PR body can go deeper on approach and testing; the title should tie back to the issue when there is one.

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
| `make build` | Production desktop bundles |
| `make build-frontend` | Type-check and build Vue frontend |
| `make build-rust` | Compile Rust backend (debug) |
| `make check` | Frontend + Rust checks |
| `make test` | Rust tests |
| `make clean` | Remove build artifacts |

Set `PIPE_DECK_USE_MOCK=1` only when you need the static sample graph (e.g. UI work without PipeWire).

When introducing a new developer-facing command (for example lint, format, or packaging), add a documented Make target in the root `Makefile` and mention it here if it is part of the standard workflow.

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

## Where to Contribute

- Product direction: [`docs/product/`](../docs/product/)
- Architecture: [`docs/architecture/`](../docs/architecture/)
- Specifications: [`docs/specs/`](../docs/specs/)
- Contributor process: this file, and background docs under [`docs/project/`](../docs/project/)

Documentation is migrating to the GitHub Wiki. When editing docs, keep [Home](../docs/Home.md), [`_Sidebar.md`](../docs/_Sidebar.md), and [`_Footer.md`](../docs/_Footer.md) in sync and follow link conventions in [Wiki publishing](../docs/project/Wiki.md).

## Code of Conduct

Participation in this project is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## OSS Onboarding Expectations

Contributions should include:

- Problem statement in plain language.
- Scope (in/out).
- Risks and rollback considerations.
- How this helps Linux audio become easier to understand/manage.
