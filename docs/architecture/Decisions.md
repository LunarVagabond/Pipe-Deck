# Product Decisions

## Purpose

Centralized record of accepted product and architecture decisions for Pipe Deck.

## Decision Log

### PD-001 Default Landing View

- Status: Accepted
- Decision: Dashboard is the default first-launch landing page.
- Rationale: Fast orientation around current audio state improves confidence for new users.

### PD-002 Routing Apply Model

- Status: Accepted
- Decision: Routing edits apply immediately by default.
- Constraint: Undo/rollback is required for all routing edits.
- Rationale: Immediate feedback reduces friction and keeps workflows fast.

### PD-003 Profile Storage Model

- Status: Accepted
- Decision: Profiles are stored as separate YAML files by default.
- Constraint: Main config may maintain a lightweight profile index and active profile pointer.
- Rationale: Better portability, backup, and sharing behavior for user setups.

### PD-004 Plugin Runtime Isolation

- Status: Accepted
- Decision: Plugins run in isolated subprocesses by default.
- Constraints:
  - Capabilities are explicit and denied by default until approved.
  - Plugin failures must not crash or block core routing operations.
- Rationale: Safety and fault isolation are mandatory for extension support.

### PD-005 Rule Engine Precedence and Debug Minimum

- Status: Accepted
- Decisions:
  - Global deterministic conflict policy in MVP.
  - Manual user overrides take precedence for the active session.
  - Minimum debug detail includes matched rule ID, match reason, chosen action, and fallback behavior.
- Rationale: Predictable outcomes and explainability are core product requirements.

### PD-006 Daemon Requirement Boundary

- Status: Accepted (implemented Phase 4)
- Decision: Daemon remains optional until persistence and restore workflows require background ownership.
- Phase 4 implementation: `pipe-deck-daemon` binary, user systemd unit, Settings UI toggle; disabled by default.
- Rationale: Avoid early operational complexity while preserving future extensibility.

### PD-007 Automatic Mapping Rollout Policy

- Status: Accepted
- Decisions:
  - Initial automatic mapping operates in suggest-first mode.
  - Auto-apply requires confidence and safety checks.
  - Disconnect handling uses grace window followed by last known-good fallback.
- Rationale: Automation should reduce effort without hidden or unsafe behavior.

### PD-008 Diagnostics Direction

- Status: Accepted
- Decision: Diagnostics prioritize local explainability and troubleshooting over mandatory telemetry.
- Rationale: Transparency and user trust are more important than early telemetry breadth.

### PD-009 Phase 2 Persistence Model

- Status: Accepted
- Decision: File-first YAML for config and profiles in Phase 2; no SQLite or database layer.
- Constraints:
  - Main config (`config.yaml`) holds preferences, active profile pointer, and profile index.
  - Profiles stored as separate YAML files under `profiles/`.
  - Export/import via file copy or simple archive.
  - SQLite may be introduced later only if indexing, concurrent writes, or daemon reconciliation justify it; YAML remains the portable contract.
- Rationale: Simpler to implement, debug, and share; matches PD-003; sufficient for Phase 2 complexity.

### PD-010 Frontend Styling Model

- Status: Accepted
- Decision: Frontend styles use SCSS partials under `src/styles/`; Vue components must not contain `<style>` blocks.
- Constraints:
  - `src/styles/main.scss` is the single entry imported by `src/main.ts`.
  - View/component styles namespace under a root class (for example `.dashboard`, `.routing-matrix`).
  - Shared theme tokens live in `src/styles/_variables.scss`.
- Rationale: Keeps presentation separate from component logic, makes styles easier to scan and reuse, and avoids scattered scoped blocks.

### PD-011 Lightweight Route Persistence (Phase 2+)

- Status: Accepted (updated Phase 3)
- Decision: Last-chosen routes from the dashboard matrix are saved in `config.yaml` under `routing_rules` and re-applied on graph refresh. Authored `rules[]` (Phase 3) take precedence by priority when both match.
- Constraints:
  - Stream rules key on composite identity (`app_name` + optional `executable` + optional `media_name`); device rules key on virtual sink `system_name`.
  - Authored rules are managed in the Rules view; dashboard explainability shows why each stream routed.
  - Full rule engine (priority, explainability, simulation) is implemented per `Rule_Engine_Spec.md`.
- Rationale: Users need routes to stick when ephemeral streams disappear without learning a separate rules vocabulary, while power users can author explicit policies that override implicit dashboard saves.

### PD-012 Phase 4 Restore Model

- Status: Accepted
- Decision: Virtual device definitions persist in `config.yaml` (`virtual_devices[]`); restore runs on app open by default and optionally at login via daemon.
- Constraints:
  - GUI and daemon share `restore.rs` and the same YAML contract.
  - Daemon safe mode: corrupt or missing config logs status and exits without creating devices.
  - Dashboard-saved routes display as **Manual route** in explainability; authored rules show rule name.
- Rationale: Survive reboots without forcing always-on services; keep automation labels user-readable.

### PD-014 Plugin API Transport

- Status: Accepted (Phase 5)
- Decision: Plugin host communicates via JSON-RPC 2.0 over stdin/stdout with newline-delimited messages.
- Constraints:
  - Request timeout 5 seconds; hung plugins are killed without blocking core routing.
  - `api_version: 1` in manifest must match host support.
- Rationale: Simple, language-agnostic, debuggable transport for subprocess isolation.

### PD-015 First-Party Effects

- Status: Accepted (Phase 5)
- Decision: Audio effects (EQ, compressor) ship as a first-party bundled plugin using PipeWire `filter-chain`; no EasyEffects dependency.
- Constraints:
  - Effects apply only to Pipe Deck-owned virtual devices (`pipe-deck-*`) in v1.
  - Plugin ships enabled by default; maintained in-tree.
- Rationale: Pipe Deck owns the audio stack; effects are core product value, not an external tool integration.

### PD-016 First-Party Audio Ownership

- Status: Accepted (Phase 5)
- Decision: Pipe Deck owns routing, effects, and virtual devices; external audio tool integrations (EasyEffects, OBS) are out of product scope.
- Constraints:
  - Community connector plugins may exist post-Phase 5 but never replace first-party paths.
  - Multi-output routing is a core engine feature (fan-out virtual sink + `pw-link`).
- Rationale: Pipe Deck is the Linux Audio Control Center, not a launcher for other audio tools.

### PD-017 Live Effects Safety Contract

- Status: Accepted (Phase 7)
- Context: an early attempt at live effects processing wrote an unvalidated
  `pipewire.conf.d` drop-in containing an FFmpeg `acompressor` filter-chain
  node, then automatically restarted PipeWire to load it — this crashed the
  user's PipeWire session (issue #64). Earlier approaches were also tried and
  rejected: `pactl load-module module-filter-chain` (not available via the
  Pulse compat layer) and `pw-cli load-module` (loads into `pw-cli`'s own
  local instance only, never `pipewire-0`).
- Decision: live effects processing is re-enabled only under a "two-speed"
  model with a hard safety contract:
  1. Never restart PipeWire without an explicit, user-confirmed Apply action.
  2. Never write a filter-chain config drop-in without passing preflight
     validation first (`pipewire::fx_validate::preflight`).
  3. The v1 live filter graph is **builtin-only** — no FFmpeg nodes, no
     arbitrary third-party plugins (`fx_validate::render_conf` is covered by
     a regression test asserting the rendered config never contains
     `ffmpeg`/`acompressor`).
  4. The filter-chain module entry is loaded with `nofail`, and only the
     dedicated `filter-chain.service` is restarted — never the main PipeWire
     graph (`pipewire::pipewire_restart`).
  5. Any failure during Structural Apply rolls back to a plain sink with no
     conf write and routing left untouched (`apply_effect_chain_structural`).
  6. Startup always cleans up legacy `99-pipe-deck-*` drop-ins regardless of
     whether a live chain is later re-applied.
  - **Live params** (slider drags after a chain is live) push straight to the
    running filter-chain node via `pw-cli set-param` — no conf write, no
    restart, so day-to-day adjustment carries none of the restart risk above.
  - **Structural Apply** (the one-time, user-confirmed "Enable/Disable live
    effects" action) is the only path that writes a conf drop-in and restarts
    `filter-chain.service`.
- Rationale: the incident showed that automating a PipeWire restart around an
  unvalidated config is a session-destroying failure mode, not a cosmetic bug.
  Splitting "live params" (safe, frequent, no restart) from "structural apply"
  (rare, explicit, restart-carrying, preflighted, and rollback-safe) preserves
  a responsive slider UX without reintroducing that risk.

### PD-018 Theming Architecture

- Status: Accepted
- Decision: Color schemes are applied at runtime as CSS custom properties written onto the document root, not as build-time SCSS theme files. Custom-scheme YAML merging (partial overrides against a built-in base palette) happens Rust-side; the frontend always receives a fully-resolved 12-color palette (9 surface/text/accent tokens plus `status_success`/`status_warning`/`status_danger`, added after an initial pass shipped with only 9 — status colors turned out to be hardcoded in several places and are just as legitimately themeable, e.g. for colorblind-friendly custom schemes).
- Rationale: the app's SCSS already consumed a single hardcoded `:root` custom-property palette across every view/component, so runtime injection required no per-file rewrites. Doing the merge in Rust keeps the built-in palettes as the single source of truth and means any future plugin-facing theme API (see [Plugin API](../specs/Plugin_API.md)) also only ever sees resolved colors, not partial YAML.
- Constraint: the static `_variables.scss` `:root` block is kept as-is as the pre-JS/failure-mode fallback (Midnight Deck).
- Constraint: the native OS window title bar/decorations (minimize/maximize/close) are outside CSS's reach entirely — Pipe Deck uses native decorations, not a custom-drawn title bar. Best-effort theming is done via Tauri's cross-platform `Window.setTheme('light'|'dark')`, called whenever the resolved scheme's `kind` changes; this is wrapped defensively (try/catch, dynamic import) since it only makes sense on desktop platforms with a themeable native title bar. No explicit OS branching was added — `setTheme` is already a cross-platform Tauri abstraction (Windows/macOS/Linux), and on a future non-desktop port the call simply no-ops/throws-and-is-caught rather than needing a dedicated code path.

### PD-019 Cross-Platform Scope, Audio Backends, and Feature Parity

- Status: Accepted
- Context: Pipe Deck's core engine, restore workflows, and effects are built directly around Linux PipeWire (`pw-dump`, `pactl`, `pw-link`, systemd user restore). The Stretch — Cross-Platform Port milestone (#67-#75) is a long-term personal goal to bring Pipe Deck to macOS and Windows; before investing in backend or UI work for those platforms, this ADR fixes the target APIs, what "parity" means per platform, and what is explicitly out of scope.
- Decision:
  - **Linux remains the primary, fully-supported platform.** Nothing in this ADR changes near-term Linux behavior or priorities; the stretch milestone has no numbered release commitment.
  - **macOS target:** Core Audio for device discovery, routing, and mixer control. Virtual devices (Pipe Deck-owned sinks/sources equivalent to today's `pactl` null sinks) require a third-party virtual audio driver (e.g. BlackHole or an equivalent aggregate-device-based approach) — Pipe Deck does not bundle or install a kernel extension/driver itself; device creation is a discovery + configuration problem against a driver the user installs, not code Pipe Deck ships.
  - **Windows target:** WASAPI for discovery, routing, and mixer control. Virtual devices similarly require a user-installed virtual cable driver; Pipe Deck configures and routes to it rather than shipping a driver.
  - **Backend abstraction:** the pluggable `AudioBackend` boundary proposed in #68 is the mechanism for all of the above — one trait owning graph fetch/subscribe, link/create/remove, volume/mute, and virtual device lifecycle, with today's PipeWire code moving behind a `LinuxPipeWireBackend` implementation and the mock backend remaining available for tests on any host OS. Backend selection is compile-time (`#[cfg(target_os)]`) or an explicit factory — never a runtime plugin.
  - **Feature parity matrix** — what ships per platform in a v1 port:

    | Capability | Linux | macOS | Windows |
    |---|---|---|---|
    | Device/stream discovery | Full | Full | Full |
    | Per-app routing | Full | Full | Full |
    | Virtual devices | Full (native, `pactl` modules) | Requires user-installed driver | Requires user-installed driver |
    | Mixer (volume/mute) | Full | Full | Full |
    | Rules engine | Full | Full | Full |
    | Background restore (daemon/login) | Full (systemd user service) | Deferred — needs LaunchAgent equivalent (#71) | Deferred — needs Task Scheduler/service equivalent (#71) |
    | Tray quick controls | Full | Deferred (#71 scope) | Deferred (#71 scope) |
    | First-party effects (filter-chain/LADSPA-based) | Full | Deferred — needs a portable effects pipeline (#74) | Deferred — needs a portable effects pipeline (#74) |
    | Plugin host (JSON-RPC subprocess) | Full | Full (transport is OS-agnostic) | Full (transport is OS-agnostic) |
    | Rule matchers keyed on process/window identity | Full (X11/Wayland) | Needs platform-specific identity source (#75) | Needs platform-specific identity source (#75) |

  - **Non-goals (explicitly out of scope for the stretch port):**
    - Bundling, signing, or installing kernel-level audio drivers on any platform — Pipe Deck configures against user-installed virtual audio drivers, it does not ship them.
    - Real-time / pro-audio latency guarantees beyond what each platform's standard API (Core Audio, WASAPI) provides out of the box.
    - Mobile platforms (iOS, Android) or sandboxed store distribution (Mac App Store, Microsoft Store) parity — packaging targets are native installers only (#73).
    - Feature parity on day one of a port landing — the matrix above is the target end state; individual capabilities land as their own tracked issues (#69-#75) and may ship Linux-only for a period.
  - **Open questions (deferred, not blocking #68):**
    - Virtual driver installation/consent UX on macOS/Windows (guiding the user through installing BlackHole/a virtual cable) — owner unassigned, tracked under #72.
    - Portable effects pipeline design once off PipeWire `filter-chain`/LADSPA — owner unassigned, tracked under #74.
    - Codesigning/notarization workflow for macOS and Windows release artifacts — owner unassigned, tracked under #73.
- Rationale: fixing target APIs and a parity matrix up front prevents the backend abstraction in #68 from being designed against an unstated or shifting definition of "cross-platform," and makes explicit that virtual devices — Pipe Deck's most PipeWire-native capability — are the highest-risk parity gap on both other platforms since neither macOS nor Windows exposes an equivalent to `pactl` null-sink modules without a third-party driver.

### PD-020 Node-Scoped Effects, Not Connection-Scoped

- Status: Accepted
- Context: Issue #105 first shipped a per-*connection* volume control: a `pipe-deck-connfeed-*` feed sink inserted between one specific source→target pair, giving that one connection an independent gain distinct from the source's own volume. It broke routing in production. Device-sourced connections never tore down the original direct link, so the target received the signal twice (doubled/echoed audio) and the new slider audibly did nothing. Stream-sourced connections rerouted correctly at the pactl level, but `stream_match.rs`/`graph_enrich.rs` only recognized the older `pipe-deck-feed-` naming, not the new `pipe-deck-connfeed-` naming, so the stream's edge silently vanished from the graph on every refresh. Neither failure raised an error — both were pure topology/graph-model gaps. Replaced same-day with a node-scoped model (this ADR).
- Decision:
  - **Effects attach to a node, never to a specific connection/edge.** A node's effect chain shapes its own output once, before that output is piped anywhere downstream — it does not vary by which target the node happens to be routed to. (Concretely: if a source feeds two different outputs, it has one volume, not two independent ones — this is a deliberate simplification from the original per-connection ask.)
  - **Volume is a permanent, pinned, always-present first row** on every effects-capable node — not something you add. It represents the node's real, final output gain (`set_device_volume`/`set_stream_volume`, the same mechanism that already existed pre-#105 — no new PipeWire object, no topology change, so this class of bug cannot recur for Volume). It never moves from the top of the list, since it's the final gain stage, not an interchangeable link in a processing chain.
  - **Future effect kinds attach below Volume**, added via a per-node context menu ("Add effect") and drag-reorderable **among themselves** (order matters once more than one stage exists — e.g. one stage's distortion feeding into the next) — but never reordered above Volume.
  - **Scope: audio source streams and Pipe Deck's own virtual devices** (mixer/mic/virtual outputs) are effects-capable. Physical hardware devices (real mics, real speakers/headphones) are explicitly out — they keep only their pre-existing plain volume control, no effects-list framing, since Pipe Deck doesn't own or control their processing path the way it does its own virtual sinks. This may be revisited later, but is not assumed by default.
  - **Any future effect's DSP mechanism must not insert a new PipeWire object into the middle of an existing connection** — that's the specific mistake this ADR reverses. The existing per-device filter-chain swap-by-identity mechanism (PD/issue #15: unload the device's own null-sink module, replace it with a filter-chain node under the same name, relink downstream once) is the template for extending real DSP (EQ, compression, etc.) to Pipe Deck's own virtual devices. Physical devices and app streams have no equivalent swappable module — extending real DSP to them needs a different, not-yet-designed mechanism, and is explicitly flagged as harder future work, not something to improvise via another connection-scoped feed sink.
- Rationale: the incident showed that "independent gain per connection" and "no new routable PipeWire object" are in tension for any source that fans out to multiple targets — the only way to keep both was to keep inserting per-pair objects into live topology, which is exactly the failure mode encountered. Scoping effects to the node instead sacrifices per-target independence (same source, same level everywhere it's routed) in exchange for correctness by construction: a node's own volume/effects chain never has to touch or reason about topology at all.

### PD-021 Plugin `effects.manage` Enforcement: Queued Requests, Not Direct PipeWire Access

- Status: Accepted
- Context: `effects.manage` (`docs/Plugin_API.md`) was declarable and grantable in Settings since v1 but had no host-side handler — granting or revoking it had no runtime effect (#120). Closing that gap means letting a plugin actually request an effects change, but the plugin host (`src-tauri/src/plugins/`) has no reference to `CoreEngine`/`AudioBackend` at all — by design (issue #68's boundary: engine/core code never lets outside callers reach `backend::linux` directly), and giving it one just to satisfy this one capability would be a much bigger structural change than the gap warrants.
- Decision:
  - `effects.manage` is enforced via a **queued-request model**, not a direct call path. A plugin sends an `effects.apply` notification (`{device_id, config}`); `PluginProcess::handle_line` (gated by the capability, mirroring `routing.suggest`) stores it in a small per-process bounded queue — it does not touch PipeWire and has no engine reference.
  - `CoreEngine::apply_queued_plugin_effect_requests` (called once per `refresh_graph`/`apply_graph_update` tick, right after `push_graph`) drains that queue and applies each request through the **existing, already-safety-checked** `set_device_effects` engine method — the same path the first-party effects UI already uses. No new PipeWire-touching code was written for this; the plugin path reuses `set_device_effects`'s built-in `pipe-deck-*`-only device guard verbatim.
  - Every applied (or rejected) request is audit-logged (`~/.local/state/pipe-deck/plugin-audit.jsonl`) with the requesting `plugin_id`.
  - This keeps the plugin host provably incapable of reaching `AudioBackend`/PipeWire directly — a plugin can only ever ask, on a tick-delayed basis, for something the host was already willing to do for its own first-party UI.
- Rationale: reusing `set_device_effects` instead of building a plugin-specific effects-apply path avoids a second, divergent safety implementation (the exact risk PD-015/PD-017 already guard against for first-party effects); the queue keeps the plugin host itself free of any `CoreEngine`/backend dependency, preserving the issue #68 boundary this repo is deliberately strict about.

### PD-022 Plugin Crash-Loop Backoff

- Status: Accepted
- Context: `PluginManager::start_plugin` (`src-tauri/src/plugins/registry.rs`) had no memory of prior failures — every call (from `set_enabled`, `rescan`, or app-startup `start_enabled`) attempted a fresh subprocess spawn regardless of how many times that plugin had just crashed on `initialize` (#102). There's no periodic auto-restart timer in this codebase, so an unattended tight loop wasn't reachable yet, but repeated manual rescans/toggles against a broken plugin had no backoff, and there was no guard rail to make a future auto-restart tick safe.
- Decision:
  - `PluginManager` tracks a per-plugin `RestartState` (consecutive failure count, next-retry timestamp, disabled reason), consulted and updated inside `start_plugin`.
  - A failed `initialize` sets an exponential backoff window (`250ms * 2^(failures-1)`, capped at 5s) before the plugin can be attempted again; a `start_plugin` call inside that window is skipped without touching `last_error` (so the UI keeps showing the real crash reason, not a transient backoff message).
  - After `MAX_CONSECUTIVE_FAILURES` (3) consecutive crashes, the plugin gets a `disabled_reason` and is refused further automatic starts until a user explicitly re-enables it — `set_enabled(id, true)` clears the restart state first, since a deliberate manual retry should never be blocked by leftover crash-loop bookkeeping.
  - `disabled_reason` rides on the existing `PluginStatus`/`list_plugins` payload (no new Tauri command), surfaced in Settings → Plugins next to the existing `last_error` display.
  - Resource ceilings (cgroups CPU/memory limits) proposed in the same issue are out of scope for this decision — this covers crash-loop backoff only.
- Rationale: keeping the backoff state entirely inside `PluginManager` (not a new timer/thread) matches the fact that plugin (re)starts are already only ever triggered by explicit calls (enable toggle, rescan, startup) — there was no need to invent a health-check loop just to make those explicit call sites safe against being hammered against a broken plugin.

### PD-023 Frontend Graph-Update Throttling: Debounce+MaxWait, Not rAF

- Status: Accepted
- Context: the backend already coalesces PipeWire monitor events before emitting `graph-updated` (`MONITOR_DEBOUNCE`/`MAX_COALESCE_WINDOW` in `backend/linux/live.rs`, ~2-5Hz worst case), but the frontend (`useRuntimeGraph` in `src/stores/runtimeGraph.ts`) applied every event unconditionally with a full reactive replacement, triggering a `RoutingGraph.vue` rebuild on each one (#57). There was no client-side rate bound at all.
- Decision:
  - Inbound `graph-updated` events are run through a trailing-edge debounce with a max-wait ceiling (`src/composables/useThrottledGraphUpdates.ts`, `createTrailingDebouncer`) before being written to reactive state: 100ms wait, 150ms max-wait.
  - This mechanism — not a plain debounce, not `requestAnimationFrame` batching — is the standing policy for any future case of bounding a bursty *inbound* event stream in this codebase. A plain debounce risks starving updates indefinitely under sustained churn (each event resets the timer). rAF-batching ties the update rate to paint cadence rather than a fixed budget, and isn't deterministically unit-testable without polyfilling `requestAnimationFrame`; debounce+maxWait is tested with fake timers (`useThrottledGraphUpdates.spec.ts`).
  - This is distinct from the existing *outbound* debounce pattern (`useMixerControls.ts`'s `scheduleChannelVolume`, a plain 120ms debounce for user-initiated volume sends) — outbound sends carry only the latest pending value and aren't at risk of sustained-churn starvation the way a live inbound event stream is, so they don't need a max-wait cap.
  - The explicit "Refresh" pull (`useRuntimeGraph().refresh()`, backed by a direct `get_runtime_graph` invoke) is not debounced — it's a one-shot user action, not part of the churn problem this addresses.
- Rationale: keeps worst-case propagation from a live PipeWire change to the routing/mixer UI comfortably inside the "reflects within 500ms of a user action" budget while bounding the Vue Flow rebuild rate under sustained churn, without coupling the fix to browser paint timing.

### PD-024 Effects on Virtual Input Devices, Not Physical Hardware

- Status: Accepted
- Context: Effects v0 (#15) only applies to `DeviceKind::Virtual && DeviceDirection::Output` devices — there was no way to attach a live effect chain to a virtual input (mic) device, which blocks any voice-changer-style workflow (physical mic → processed virtual mic → apps). Issue #19 asked to attach effect chains directly to physical sinks and capture inputs, but PD-020 deliberately scoped effects to Pipe Deck's own virtual devices only, excluding physical hardware by design — Pipe Deck doesn't own or control a real device's processing path the way it does its own virtual sinks, and that exclusion was a hard-won lesson from the #105 incident, not an oversight to casually reopen (#139).
- Decision:
  - Effects extend to virtual **input** devices using the same node-scoped, swap-by-identity mechanism PD-020 already established for virtual outputs — a second, capture-direction `module-filter-chain` conf template (`media.class = Audio/Source/Virtual` on the exposed side, vs. the existing playback template's `Audio/Sink`), with the module-unload/reload and rollback paths made direction-aware (calling `pactl::create_virtual_source` rather than the output-flavored `create_null_sink` when reverting an input device).
  - PD-020's exclusion of physical hardware is **not reopened**. A physical device (e.g. a real mic) that needs processing must be wrapped by a virtual device first — routed into a virtual input via the existing `mix_sources`/`virtual_mic_mix.rs` mechanism — and the effect chain attaches to that virtual wrapper, never to the hardware device itself. This is the existing product pattern for virtual devices generally, not a new hardware-facing mechanism.
  - Issue #19 is superseded by this decision for capture inputs specifically: "attach chains to physical... capture inputs" is satisfied via wrap-with-virtual-device rather than a direct-to-hardware mechanism.
- Rationale: reusing the same swap-by-identity template for both directions keeps this within one well-understood, already safety-hardened mechanism (PD-017's two-speed live-effects contract) rather than inventing a second one; keeping physical hardware permanently out of scope preserves the exact guarantee PD-020 was written to establish after #105, while still giving users a working path to "effects on my real mic" through the virtual-device layer Pipe Deck already fully owns.

### PD-025 Effect-Add Is the Explicit Action; No Separate Confirm Dialog

- Status: Accepted
- Context: PD-017's live-effects safety contract requires "never restart PipeWire without an explicit, user-confirmed Apply action" — implemented in `Effects.vue` as a distinct "Enable live effects" button behind a confirm dialog, separate from adjusting any slider. Product feedback on the node-scoped effects UI (#139): this extra step is friction with no real safety benefit once effects are attached directly to a node via an explicit "+ Effect" menu action — the add-effect click is already deliberate and infrequent, not something that happens accidentally per-slider-tick.
- Decision:
  - The explicit, user-confirmed action PD-017 requires is now the deliberate act of adding, removing, or reordering an effect stage via the UI (Routing graph context menu, Mixer disclosure panel, or the `Effects.vue` page) — not a separate confirmation dialog afterward. `add_effect_stage`/`remove_effect_stage`/`reorder_effect_stages` (`core/engine/effects_ops.rs`) call straight through to the existing `apply_effect_chain_structural`/`remove_effect_chain_structural` primitives with no intermediate "enable" step.
  - A one-time, non-blocking toast (not a click-through gate) informs the user the first time in a session that adding an effect briefly restarts Pipe Deck's effects daemon — informational, not a confirmation the user must act on.
  - PD-17 bullets 2–6 (preflight validation before any conf write, builtin-only v1 scope, `nofail` + dedicated-service-only restart, rollback-to-plain-sink on any failure, startup cleanup of legacy drop-ins) are all unchanged — only the UX gate in front of Structural Apply changes, not the safety mechanics behind it.
  - The restart mechanism itself (conf.d + `systemctl restart filter-chain.service`) is unchanged in this decision — eliminating the restart entirely is tracked separately (#141: native zero-restart apply via a long-running process linking `libpipewire` directly), a larger, distinct initiative not folded into this one.
- Rationale: PD-017's actual concern (see the #64 incident it was written after) was an unvalidated config silently crashing the session via an automated restart with no human decision point at all — a deliberate "+ Effect" click already is that decision point, preflighted and rollback-safe. Requiring a second, separate confirmation after the user has already chosen to add an effect protects against nothing additional; it only adds friction, which is precisely the feedback that prompted this change.

### PD-026 Virtual Outputs Chain Into Virtual Outputs; Framed as Busses, Not Renamed as a Data Type

- Status: Accepted
- Context: #143 reported that a virtual output could not be routed as a source into another virtual output — chaining a submix into a master mix was rejected — caused by `targetsForVirtualSink` (`src/utils/routingLayout.ts`) hardcoding valid fan-out targets to only physical outputs and virtual inputs, with no backend-side restriction (`split_sink::apply_sink_targets`/`validate_fan_out_target` already permitted it). #144, filed alongside it, asked a broader question first: should "virtual input/output" be reframed as an audio-bus mental model, and is the routing gap a wording problem or a real feature gap? Investigation confirmed both: an unintentional UI-only restriction (no ADR ever justified excluding this), and a genuine wording mismatch — docs and UI copy described virtual devices as "sink/source types you create" rather than the bus-like grouping/processing/chaining behavior users actually want.
- Decision:
  - `targetsForVirtualSink` and `connectionRules.ts`'s `isMicMixCandidate` now allow virtual-output → virtual-output routing as a plain route (`device_route`/`device_targets`), distinct from the mic-mix merge path, which `isMicMixCandidate` now scopes strictly to `target.direction === "input"` instead of the previous overly-broad `!== "duplex"`.
  - `split_sink::apply_sink_targets` gained a cycle guard (`would_create_cycle`) — sink-to-sink chaining introduces a new A → B → A risk that couldn't exist while virtual outputs were leaves; a cycle is rejected with an explicit error rather than silently applied.
  - UI copy and `docs/UI_Spec.md` (new "Virtual Devices as Busses" section) now describe virtual outputs as busses — group, process, chain onward — while virtual inputs remain intentionally merge-only leaves (a virtual mic is consumed by apps, not chained further). `docs/Audio_Terminology.md`'s existing "Bus"/"Submix" glossary entries already matched this framing; only the "Virtual device" entry needed a cross-reference.
  - `DeviceKind`/`DeviceDirection` and the underlying `module-null-sink` primitive are **not renamed or restructured** — this is a UI mental-model and routing-permission change, not a data-model migration. Renaming the enums/commands themselves was considered out of scope: it would touch every `Device`/`RuntimeGraph` call site (Rust and TS) for no functional gain over the copy/routing changes made here.
- Rationale: the routing restriction had no design justification and directly blocked the "submix feeding a master mix" workflow #143/#144 both described as expected; fixing it is a narrow, low-risk change once confirmed backend-safe. Reframing the copy toward bus language costs little (docs + a handful of UI strings) and resolves the terminology confusion #144 raised, without taking on the much larger, higher-risk cost of an actual `DeviceKind`/`DeviceDirection` rename across the Rust/TS boundary — that remains a candidate for a future, separately-scoped decision if the bus framing proves insufficient on its own.

### PD-027 Filter-Chain Native-Hosting Spike: Feasible, Bigger Than Assumed

- Status: Accepted (research spike concluded; no production change)
- Context: #141 proposed eliminating the conf.d + `systemctl restart filter-chain.service` flow by linking `libpipewire` natively (`pipewire-rs`) from a long-running process, calling `pw_context_load_module`/equivalent directly instead of writing a config file and restarting a whole service. PD-017 had already found that `pactl load-module module-filter-chain` isn't exposed via the Pulse compat layer, and that `pw-cli load-module` only loads into `pw-cli`'s own throwaway local context — never the running `pipewire-0` session — but that finding used a short-lived CLI invocation, not a long-running process, so #141 required a hands-on spike before committing to the approach.
- Decision: a throwaway example binary, `src-tauri/examples/filter_chain_spike.rs` (built only behind the new `spike` Cargo feature — `cargo run --example filter_chain_spike --features spike` — never part of `cargo build`/`cargo test`/`cargo check` by default, since it pulls in `pipewire-rs`/`libpipewire-0.3` dev headers most contributors' environments don't have), was written and run against a real PipeWire 1.5.85 session. Findings:
  - **It works, and differs from PD-017's pw-cli finding.** Pumping the main loop for ~1s after `pw_context_load_module` (rather than exiting immediately, which is what a single `pw-cli` command does) lets the module's async node/port setup finish. The resulting node is a real node in the *actual* running `pipewire.service` graph — confirmed externally via `pactl list short sinks`, `pw-link -o`/`-i`, and `pw-cli ls Node` from a separate process, not just this process's own bookkeeping. 5/5 load cycles succeeded; 0/5 leaked past `pw_impl_module_destroy`.
  - **The processed output side does not auto-link anywhere** — its ports exist but have zero links until something explicitly connects them onward, exactly matching today's `effect_output.*` convention. The existing `pw_link`-based "link the processed output onward" logic in `pipewire/filter_chain.rs` could be reused largely as-is.
  - **A real lifecycle hazard, found the hard way**: calling `pw::deinit()` while any `ContextRc`/`MainLoopRc` (or their underlying raw pointers) are still alive segfaults on shutdown, because their `Drop` impls call into an already-deinitialized library. A production implementation needs one centrally-owned lifecycle for this, not something each call site can get subtly wrong.
  - **Not conclusively answered**: whether repeated load/unload cycles genuinely never leak, or just don't leak measurably over 5 cycles (~560kB RSS growth observed, small but unproven flat) — a longer soak test is flagged as necessary follow-up work, not done here.
  - A design sketch of the `AudioBackend` trait shape this would need — `load_effect_chain`/`unload_effect_chain`/`set_effect_chain_live_params`/`effect_chain_capabilities` — was added to `backend/mod.rs` with default `not implemented` bodies so it requires zero changes to any existing backend. This is documentation of the target shape only; no backend implements it yet, and no call site references it.
- Rationale: the spike's job was to convert #141 from "does this work at all" to "here's what a real implementation has to handle" — it succeeded at that. The mechanism is more promising than PD-017's earlier finding suggested (a long-running process genuinely can attach nodes to the live session, not just its own local context), but a full implementation is a materially larger effort than "swap one API call for another": it needs a correct library-lifetime story inside a long-running GUI process, a leak-soak test longer than 5 cycles, and — per #141's original scoping — closing the `core/engine/effects_ops.rs` → `backend::linux::*` boundary gap at the same time rather than as an afterthought. None of that is done here; this decision records the spike's findings and closes the "is this worth pursuing further" question with a qualified yes, not the trait implementation itself.

### PD-028 Milestones Are Releases Only; Epics Move to Native Sub-Issues

- Status: Accepted
- Context: Milestones had been doing two unrelated jobs — tracking releases (e.g. a future `v0.5.0`) and tracking large multi-release roadmap-phase initiatives ("Phase 6 — Consolidation", "Quality & Platform", and similar). This made "what ships next?" hard to answer from the milestone list alone, and predated GitHub's native sub-issue and issue-relationship features, which didn't exist when the milestone-based scheme was set up.
- Decision:
  - Milestones are release-only going forward — one per shipped version, holding only issues actually scoped to that release. See `docs/project-management/milestones-and-releases.md`.
  - The 9 roadmap-phase milestones that previously stood in for epics were each converted to a `[Epic] <name>` issue (label `epic`), with the milestone's description copied verbatim into the Epic body and every issue that was in that milestone (open and closed) re-parented as a native GitHub sub-issue of the new Epic. The original milestones were closed, not deleted, to keep old links resolvable.
  - Blocks/Blocked-by relationships between historical issues were deliberately **not** auto-migrated (would have required parsing free-text dependency language across ~140 issues); native relationships are documented as a going-forward convention only. See `docs/project-management/issue-workflow.md`.
  - `.claude/skills/gh-tickets/SKILL.md` was updated to reflect the new model for future ticket triage.
- Rationale: separating "what ships" (milestone) from "what's the initiative" (epic) lets releases stay short-lived and predictable while initiatives span multiple releases, and uses GitHub's native tooling (sub-issues, relationships) instead of a milestone-based approximation of the same thing. Full detail in `docs/project-management/README.md`.

## Related Documents

- `docs/Product_Requirements.md`
- `docs/Roadmap.md`
- `docs/System_Architecture.md`
- `docs/PipeWire_Design.md`
- `docs/UI_Spec.md`
- `docs/Theming.md`
- `docs/Config_Spec.md`
- `docs/Plugin_API.md`
- `docs/Development.md`
- `docs/Packaging.md`
