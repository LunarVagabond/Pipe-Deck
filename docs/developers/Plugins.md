# Plugins

Contributor guide for Pipe Deck extension development.

## Overview

Plugins run in isolated subprocesses and communicate via JSON-RPC 2.0 over stdin/stdout. See [Plugin API](../specs/Plugin_API.md) for the full contract.

## Quick start

1. Copy `plugins/template/` to `~/.config/pipe-deck/plugins/my-plugin/`
2. Edit `plugin.yaml` (unique `id`, requested `capabilities`, and optionally `developer`/`repo` — these show up in the Plugins table and detail view)
3. Implement the entry binary (Python, Rust, or any language with stdio JSON)
4. Enable the plugin in **Settings → Plugins** and approve capabilities

New to the plugin API? The [template walkthrough](../../plugins/template/README.md) explains the minimal scaffold and a heavily-commented worked example (`graph-reader`) that reads the runtime graph snapshot via the `graph.updated` method — with a headless test command you can run without the app.

## Capability reference (v1)

| Capability | Use | Enforced today |
|------------|-----|-----------------|
| `graph.read` | Receive `graph.updated` notifications | Yes |
| `routing.suggest` | Send route suggestions to the host (no apply) | Yes |
| `profile.read` | Receive `profile.updated` notifications | Yes |
| `effects.manage` | Manage filter chains on `pipe-deck-*` devices | No |
| `ui.panel.register` | Register a nav panel in the host UI | Yes |

`effects.manage` can still be requested/granted even though nothing enforces it yet — see [Plugin API](../specs/Plugin_API.md) for what "not enforced" means in practice.

## Lifecycle

1. Host discovers `plugin.yaml` in bundled and user plugin directories (re-run on demand via the "Rescan plugin directories" button in Settings, no app restart needed)
2. User enables plugin and approves capabilities in Settings
3. Host spawns subprocess, sends `initialize` RPC, then pushes the active profile's metadata if `profile.read` is granted
4. Plugin may register UI panels, respond to `graph.updated`/`profile.updated`, and send `routing.suggest` notifications
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

Before distributing a plugin, complete [Plugin Review Checklist](../specs/Plugin_Review_Checklist.md).
