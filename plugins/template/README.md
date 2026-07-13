# Plugin template

Minimal starter for Pipe Deck community plugins.

## Layout

```
my-plugin/
  plugin.yaml
  bin/my-plugin-entry
```

## Develop

1. Copy this directory to `~/.config/pipe-deck/plugins/<id>/`
2. Implement JSON-RPC 2.0 over stdin/stdout (see `docs/Plugin_API.md`)
3. Enable the plugin in Pipe Deck Settings and approve capabilities

## Test locally

```bash
PIPE_DECK_BUNDLED_PLUGINS=/path/to/plugins PIPE_DECK_USE_MOCK=1 pipe-deck plugins list
```
