# Security Policy

## Supported Versions

Pipe Deck does not yet have a stable release line with long-term security
support. Security fixes are applied to the latest code on `main`.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Instead, report it privately by emailing **cconlon@dcorps.dev** with:

- A description of the vulnerability and its potential impact.
- Steps to reproduce (proof-of-concept code or commands are helpful).
- The Pipe Deck version/commit and platform you tested against.

We'll acknowledge your report as soon as we can and follow up with next
steps. Once a fix is available, we'll coordinate on disclosure timing and
credit you in the release notes if you'd like.

## Scope

Pipe Deck shells out to `pactl`, `pw-link`, and `pw-dump` rather than
linking against PipeWire directly, and stores config/profiles as local
YAML under `~/.config/pipe-deck/`. Reports involving command construction,
config/profile parsing, or the plugin JSON-RPC host (`src-tauri/src/plugins/`)
are especially relevant.
