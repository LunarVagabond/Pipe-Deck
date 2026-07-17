# Plugin template

Starter kit for Pipe Deck community plugins. Two things live here:

- **`plugin.yaml` + `bin/echo-plugin`** — the minimal scaffold: it completes the
  `initialize`/`shutdown` handshake and acknowledges everything else. Copy this
  when you want the smallest possible starting point.
- **`examples/graph-reader/`** — a worked example that goes one step further and
  actually *uses* a capability. Read this when you want to see a real RPC method
  handled meaningfully. Walkthrough below.

Plugins speak [JSON-RPC 2.0](../../docs/specs/Plugin_API.md) over stdin/stdout,
one JSON object per line, each in its own subprocess.

## Layout

```
my-plugin/
  plugin.yaml          # manifest: id, version, entry, requested capabilities
  bin/my-plugin-entry  # executable that speaks JSON-RPC over stdio
```

## Develop

1. Copy `plugins/template/` (or `plugins/template/examples/graph-reader/`) to
   `~/.config/pipe-deck/plugins/<id>/`
2. Edit `plugin.yaml` — unique `id`, requested `capabilities`, optional
   `developer`/`repo` (shown in the Plugins table and detail view)
3. Implement the entry binary (Python, Rust, or any language with stdio JSON)
4. Enable the plugin in **Settings → Plugins** and approve capabilities

## Worked example: `graph-reader`

`examples/graph-reader/` reads the runtime graph snapshot the host pushes and
logs a short routing summary. It is deliberately tiny — a teaching artifact,
not production routing logic.

- **RPC method used:** `graph.updated` (host → plugin notification carrying the
  full runtime graph as `params`). The plugin parses it and, for each playback
  stream, logs where that stream is currently routed.
- **Capability:** `graph.read` only. That is the single capability the method
  needs — no more, no less — which is exactly the minimal set the
  [Plugin Review Checklist](../../docs/specs/Plugin_Review_Checklist.md) asks for.
- **Two rules the example demonstrates:**
  - **stdout is for JSON-RPC only.** The host parses every stdout line, so all
    human-facing logging goes to **stderr** (which the host captures as a
    diagnostic tail). Printing prose to stdout would corrupt the channel.
  - **Notifications have no `id` and get no reply.** `graph.updated` is a
    notification; the plugin reads it and stays silent, unlike a request such as
    `initialize`, which must be answered with the same `id`.

See `examples/graph-reader/bin/graph-reader` — it is heavily commented
line-by-line.

## Test locally

The graph snapshot the plugin receives is just JSON, so you can exercise the
worked example headlessly by piping the same messages the host would send —
no PipeWire and no GUI required:

```bash
cd plugins/template/examples/graph-reader
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"api_version":1}}' \
  '{"jsonrpc":"2.0","method":"graph.updated","params":{"data_source":"mock","devices":[],"streams":[{"id":"stream-discord","app_name":"Discord","direction":"playback","current_target":"sink-chat"}],"links":[]}}' \
  '{"jsonrpc":"2.0","id":2,"method":"shutdown"}' \
  | ./bin/graph-reader
```

stdout shows only the two JSON-RPC replies; the graph summary appears on stderr.

To run it under the real host with Pipe Deck's mock audio backend instead:

```bash
PIPE_DECK_BUNDLED_PLUGINS=/path/to/pipe-deck/plugins/template/examples \
PIPE_DECK_USE_MOCK=1 pipe-deck plugins list
```

## Before you distribute

Complete the [Plugin Review Checklist](../../docs/specs/Plugin_Review_Checklist.md).
