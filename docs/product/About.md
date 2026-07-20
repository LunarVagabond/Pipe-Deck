# About Pipe Deck

## Why I built this

I came to Linux audio from Windows, where I'd gotten used to SteelSeries Sonar handling routing, mixing, and per-app effects from one place. When I moved to Linux full-time, I went looking for something with that same feel — one control center instead of a handful of overlapping tools — and didn't find it. PipeWire itself is genuinely capable; what was missing was a front end that made routing, mixing, virtual devices, and automation feel like they belonged to the same app.

I'm a tinkerer by nature, so "nobody's built it yet" reads less like a dead end and more like an invitation. Pipe Deck started as a personal itch: I wanted to stop juggling `pavucontrol`, `qpwgraph`, and shell scripts just to get audio routed the way I wanted, every time I rebooted or switched games. Building it myself meant I could shape it around how I actually work, rather than adapting my workflow to whatever tool happened to exist.

## What that means for the project

Because Pipe Deck grew out of a real, specific frustration, the guiding question for every feature is still the practical one: **does this make Linux audio easier to understand and manage?** Not "is this technically possible with PipeWire," but "would this have saved me a headache." That's also why the project is open source from day one — if this solved a problem for me, it's a safe bet it solves the same problem for other people who came to Linux expecting audio control to be this straightforward and found it wasn't, yet. Longer term, that's the bigger goal: Linux audio shouldn't require being a tinkerer to get right — it should work for people who just want their game, stream, or music to sound correct without learning PipeWire internals first.

## Where it's going

Pipe Deck is under active development toward a v0.5.0 beta. See the [Roadmap](../product/Roadmap.md) for what's planned and the [Decisions](../architecture/Decisions.md) log for the architectural choices behind it. Contributions, bug reports, and feature ideas are welcome — see [Contributing](../../.github/CONTRIBUTING.md).
