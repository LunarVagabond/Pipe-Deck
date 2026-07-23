# Roadmap

## Purpose

Describe Pipe Deck's strategic direction and where the product is headed. This is **not** a phase-by-phase delivery tracker — that job moved to GitHub (see below) so it stops drifting out of sync with reality.

## Mission Anchor

Pipe Deck is the Linux Audio Control Center.

All roadmap items must improve Linux audio clarity, control, or reliability for users.

## Where delivery tracking actually lives now

Concrete "what's shipping when" and "what's left in this initiative" live on GitHub, not here:

- **Milestones** (e.g. `v0.0.6-alpha`) track what ships in a specific release. See [open milestones](https://github.com/LunarVagabond/Pipe-Deck/milestones).
- **Epics** (`epic`-labeled issues, native sub-issues underneath) track large, multi-release initiatives. See [open epics](https://github.com/LunarVagabond/Pipe-Deck/issues?q=is%3Aissue+label%3Aepic).

This split happened 2026-07-18 (PD-028 in [Decisions](../architecture/Decisions.md)) specifically because this document's old phase-by-phase status sections stopped getting updated as work moved fast, and a reader had no way to tell a stale claim from a current one. See [Project Management](../project-management/README.md) for the full model.

## Delivery history

Pipe Deck's initial build-out ran through five numbered phases, all shipped:

| Phase | Delivered |
|-------|-----------|
| 1 — Documentation Foundation | Canonical `docs/` structure, contributor onboarding |
| 2 — Foundation Runtime and Core Flows | Tauri+Vue app shell, PipeWire enumeration, routing, mixer, YAML profiles, baseline packaging |
| 3 — Rules and Advanced Routing UX | Deterministic rule engine with explainability, simulation, Rules view |
| 4 — Persistence and Background Management | Virtual device restore across reboots, optional `pipe-deck-daemon`, packaging hardening |
| 5 — Ecosystem and Integrations | Plugin host, `pipe-deck` CLI, multi-output routing, first-party Effects v0 |

Phase numbers still show up as shorthand in some architecture/spec docs (e.g. "Phase 4" next to the daemon, "Phase 3+" next to rules) — that's just dating when a feature landed, not a claim about current planning. For the specifics of what shipped and why, see [Decisions](../architecture/Decisions.md).

What used to be "Phase 6" onward is where numbered phases stopped being used for anything live. Those later phases were converted into epics as part of the same 2026-07-18 restructuring:

| Former phase name | Now tracked as |
|--------------------|----------------|
| Phase 6 — Consolidation | [Epic #179](https://github.com/LunarVagabond/Pipe-Deck/issues/179) |
| Phase 7 — Processing | [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182) |
| Phase 8 — Advanced Routing | [Epic #184](https://github.com/LunarVagabond/Pipe-Deck/issues/184) |
| Routing Graph — At-a-Glance Polish | [Epic #178](https://github.com/LunarVagabond/Pipe-Deck/issues/178) |
| Routing Pipeline Hardening | [Epic #177](https://github.com/LunarVagabond/Pipe-Deck/issues/177) (closed — shipped) |
| Quality & Platform | [Epic #181](https://github.com/LunarVagabond/Pipe-Deck/issues/181) |
| Ecosystem & Packaging | [Epic #183](https://github.com/LunarVagabond/Pipe-Deck/issues/183) |
| Documentation & Process | [Epic #180](https://github.com/LunarVagabond/Pipe-Deck/issues/180) |
| Cross-Platform Port (stretch, unscheduled) | [Epic #185](https://github.com/LunarVagabond/Pipe-Deck/issues/185) |

## Strategic Direction

- Automatic mapping should progressively reduce manual sink/source setup.
- Automation must remain safe, explainable, and reversible.
- Dashboard remains the default hub; dedicated views add depth without replacing it.
- Full in-house effects processing (balance, EQ, dynamics) expands without depending on EasyEffects or other third-party audio control apps.
- Advanced routing UX (dedicated Routing/Sources views, optional graph layouts) grows once the Dashboard matrix outgrows what a shared hub can reasonably show.
- Platform parity beyond Linux is a long-term, unscheduled direction, not a near-term commitment — see [Epic #185](https://github.com/LunarVagabond/Pipe-Deck/issues/185) and [PD-019](../architecture/Decisions.md).

## Navigation model

- **Enabled today:** Dashboard, Profiles, Rules, Routing, Mixer, Sources, Effects, Settings — every primary sidebar destination has shipped.
- Disabled sidebar destinations were used earlier as a "north star" placeholder for unshipped views; that pattern is retired now that all of them have shipped. Any future large navigation addition should track through the epic covering that work rather than a Roadmap phase.
- See [UI Spec](../specs/UI_Spec.md) for per-view intent.

## Long-standing decisions

- Persistence is file-first YAML (no SQLite); SQLite remains a future option only if indexing or daemon needs justify it (PD-009).
- Optional `pipe-deck-daemon` handles login-time restore; GUI-only restore remains the default path.
- Automatic mapping rollout is gated by explainability, safety checks, and rollback readiness (PD-007).
- First-party effects processing is scoped to expand without introducing a dependency on third-party audio control apps (PD-015, PD-016) — see [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182).
