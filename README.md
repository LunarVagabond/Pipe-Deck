# Pipe Deck

Pipe Deck is the Linux Audio Control Center.

Project documentation is in `docs/`.

## Development

Prerequisites: Rust (stable), Node.js 20+, Linux with PipeWire.

Use the Makefile as the primary development interface:

```bash
make install   # first-time setup
make start     # run desktop app in dev mode
make build     # production bundles
make help      # list all commands
```

See `docs/project/Contributing.md` for contributor workflow and Makefile conventions.

## Start Here

- `docs/README.md`
- `docs/architecture/Phase2_Scaffold.md`
- `docs/product/Product_Requirements.md`
- `Backlog.md`

## Documentation Structure

- `docs/product/` - product direction and roadmap
- `docs/architecture/` - system and PipeWire design
- `docs/specs/` - behavior and technical specs
- `docs/project/` - contributor process
- `Backlog.md` - prioritized implementation and documentation backlog

## Contributor Feature Filter

If you are proposing or building a feature, use this as your baseline gate:

- Can I clearly explain how this makes Linux audio easier to understand and manage?

If the answer is no, refine the idea before opening or implementing it.
