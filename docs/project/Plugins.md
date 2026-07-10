# Plugins

Contributor guide for Pipe Deck extension development.

## Overview

Plugins run in isolated subprocesses and communicate via JSON-RPC 2.0 over stdin/stdout. See [Plugin API](../specs/Plugin_API) for the full contract.

## Quick start

1. Copy `plugins/template/` to `~/.config/pipe-deck/plugins/my-plugin/`
2. Edit `plugin.yaml` (unique `id`, requested `capabilities`)
3. Implement the entry binary (Python, Rust, or any language with stdio JSON)
4. Enable the plugin in **Settings → Plugins** and approve capabilities

## Capability reference (v1)

| Capability | Use |
|------------|-----|
| `graph.read` | Receive `graph.updated` notifications |
| `routing.suggest` | Return route suggestions (no apply) |
| `profile.read` | Read active profile metadata |
| `effects.manage` | Manage filter chains on `pipe-deck-*` devices |
| `ui.panel.register` | Register a nav panel in the host UI |

## Lifecycle

1. Host discovers `plugin.yaml` in bundled and user plugin directories
2. User enables plugin and approves capabilities in Settings
3. Host spawns subprocess, sends `initialize` RPC
4. Plugin may register UI panels and respond to `graph.updated`
5. On disable/shutdown: `shutdown` RPC → process exit

## Testing

```bash
export PIPE_DECK_USE_MOCK=1
export PIPE_DECK_BUNDLED_PLUGINS=/path/to/pipe-deck/plugins
make test
pipe-deck plugins list
```

## First-party plugins

`pipe-deck-effects` ships bundled and demonstrates the full v1 contract. Community plugins follow the same manifest format.

## Review

Before distributing a plugin, complete [Plugin Review Checklist](./Plugin_Review_Checklist).
