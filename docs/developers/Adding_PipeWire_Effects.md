# Adding a New PipeWire Effect

This is a practical guide for extending Pipe Deck's DSP-backed processing
nodes (issue #293/PD-032) — e.g. adding a Limiter, Compressor, or any other
`libpipewire-module-filter-chain`-backed effect alongside the existing
5-Band EQ (`ProcessingNodeKind::Eq5Band`). It captures the architecture and
the gotchas found while fixing issue #303 (the EQ5Band node's DSP never
actually ran), since most of those gotchas apply to *any* new effect built
the same way, not just the EQ.

## The mechanism: one PipeWire module, loaded live

Pipe Deck does not write custom DSP or a custom `pw_stream`/`pw_filter`
client. Every DSP-backed processing node is one instance of the stock
`libpipewire-module-filter-chain` module, loaded directly into a live
PipeWire session via `pw_context_load_module` — see
[PipeWire's own filter-chain module reference](https://docs.pipewire.org/page_man_pipewire-filter-chain_conf_5.html).
That module creates **two `pw_stream` objects** internally (a capture
stream and a playback stream) and runs whatever builtin/LADSPA/LV2 filter
graph you hand it in `filter.graph`. Pipe Deck is a thin control layer over
this — see `src-tauri/src/pipewire/native_host.rs`'s module doc comment.

**Read the actual shipped examples before writing a new filter graph.**
`/usr/share/pipewire/filter-chain/*.conf` on any machine with PipeWire
installed contains real, maintainer-authored recipes (`sink-eq6.conf`,
`sink-dolby-surround.conf`, `sink-virtual-surround-5.1-kemar.conf`, etc.).
`fx_validate::render_filter_chain_module_args` was built by diffing against
these — when in doubt about a property, check what they actually use rather
than guessing from generic docs. `man 7 libpipewire-module-filter-chain`
(same content as the page above) documents the full `filter.graph` schema:
nodes, links, inputs/outputs, `capture.volumes`/`playback.volumes`, and the
builtin filter list (Mixer, Copy, Biquads, Parametric EQ, and more).

## Where the pieces live

| Concern | File |
|---|---|
| Renders the `filter.graph` args PipeWire actually loads | `src-tauri/src/pipewire/fx_validate.rs` (`render_filter_chain_module_args` + its `render_module_args`/`render_module_args_capture` wrappers) |
| Loads/unloads the module, pushes live param updates | `src-tauri/src/pipewire/native_host.rs` (daemon-only — see below) |
| GUI-side proxy to the daemon | `src-tauri/src/daemon/ipc/client.rs` (`NativeHostClient`) |
| Domain model for a processing node's config | `src-tauri/src/core/models.rs` (`ProcessingNodeKind`, `ProcessingNodeSpecKind`) |
| Engine operations (create/update/connect/bypass) | `src-tauri/src/core/engine/processing_node_ops.rs` |
| The real backend implementation (`AudioBackend` trait) | `src-tauri/src/backend/linux/live.rs` (`load_processing_node`, `set_processing_node_eq_params`-style methods) |
| Capability probing / validation gate | `src-tauri/src/pipewire/fx_capability.rs`, `fx_validate.rs::preflight` |
| Frontend slider/control UI | `src/components/RoutingGraphNodeEq5Band.vue` (per-kind, one component each) |

## Gotchas found fixing issue #303 (apply these to any new effect too)

1. **The hosting connection must run continuously, not be manually pumped.**
   `native_host.rs` originally used a `pw::main_loop::MainLoopRc`, only
   `iterate()`d in short bursts when a Rust function call happened to touch
   it. A module loaded into a connection that isn't continuously serviced
   never actually gets scheduled by the graph driver — it sits permanently
   `suspended` (visible live via `pw-top`: `QUANT=0`, state `S`, forever,
   even with a correctly-wired active upstream). Fixed by switching to
   `pw::thread_loop::ThreadLoopRc`, `.start()`ed once and left running for
   the process's life, with every PipeWire call wrapped in
   `thread_loop.lock()`/drop-to-unlock. See
   [PipeWire's thread-loop overview](https://docs.pipewire.org/page_thread_loop.html)
   and the [`pw_thread_loop` API reference](https://docs.pipewire.org/group__pw__thread__loop.html).
   This applies to **any** module loaded through `native_host` — a new
   effect reuses `load_chain`, so it inherits this for free, but don't
   reintroduce a manually-pumped loop anywhere in this module.

2. **Don't look up a live node via a registry `global` listener.** The
   PipeWire server sends a `global` *announcement* event to a client
   exactly once, the first time an object becomes visible. A listener
   registered any time after that (e.g. seconds later, on a slider drag)
   never sees it — this isn't a race, it's a protocol property. Use a
   synchronous `pw-dump` snapshot lookup instead
   (`pipewire::pw_cli::find_node_id_by_name`, reused by
   `native_host::set_param`'s node lookup and `load_chain`'s post-creation
   readiness wait).

3. **`node.passive = true` belongs on the playback side, always** — even
   for a node like an EQ that isn't itself a user-selectable device. Every
   official example config sets it there regardless of what the playback
   side is presented as. Per `man 7 pipewire-props`, it tells the session
   manager "this filter sits in front of a sink/source and should suspend
   together with it" — this held up under live testing; removing it was a
   red herring tried and reverted while chasing #303.

4. **Don't add `node.autoconnect = false`.** It seems like the right call
   for a manually-routed, non-default-sink use case (and the general
   PipeWire docs on `node.autoconnect` — see
   [`pipewire-props(7)`](https://docs.pipewire.org/page_man_pipewire-props_7.html) —
   support that reading), but **no official shipped example uses it**, and
   adding it produced no measurable benefit in live testing while being the
   one deviation from every proven-working reference config. If a future
   effect seems to need it, verify live before trusting the theory alone —
   see the [PipeWire config overview](https://docs.pipewire.org/page_config.html)
   for how session-manager properties are documented generally.

5. **A brand-new sink can come up muted/zero-volume**, and — separately —
   `pactl move-sink-input` doesn't always fully detach a stream's old raw
   port links (observed live with Firefox specifically; a `pw-cat`-sourced
   stream detached cleanly on its own). `load_processing_node`'s Eq5Band arm
   force-sets the capture-side sink's volume/mute after creation, and
   `relink_processing_node_port`'s stream-peer branch explicitly disconnects
   any leftover output links to the *old* target after moving a stream via
   `pactl` (`pw_link::disconnect_stale_output_links`). Any new effect kind
   that creates a fresh sink or accepts a stream as a direct input should
   do the same rather than assuming the "obvious" primitive (`pactl`
   volume/move) is sufficient by itself.

6. **A DSP-backed processing node needs a crash/restart reconciliation
   entry.** `native_host`'s connection — and therefore any module loaded
   through it — doesn't survive the daemon process dying (see
   `daemon::mod.rs`'s module doc comment). `reconcile_live_effects_state`
   only ever covered the older device-attached effect chains;
   `reconcile_live_processing_nodes` (added alongside the #303 fix) covers
   `Eq5Band`. **A new DSP-backed kind must be added to
   `reconcile_live_processing_nodes`'s match arm**, or a persisted node of
   that kind will silently stop working after any daemon restart with no
   error until someone tries to use it. `Mixer`/`FanOut` don't need this —
   they're plain `pactl`-created null sinks, owned by the system PipeWire
   session itself, not by `native_host`'s connection.

7. **A filter that needs a bundled asset (an IR file, a preset, ...) can't
   use Tauri's resource-dir API for path resolution.** That API only exists
   on an `AppHandle`, and `native_host` runs inside the daemon binary
   (`bin/pipe-deck-daemon`), a plain Rust process with no Tauri context at
   all — see this file's own "one PipeWire module, loaded live" section and
   `daemon/mod.rs`'s module doc comment for why the daemon, not the GUI, is
   what actually loads the filter-chain module. Reverb (issue #327,
   `fx_validate::reverb_ir_path`) is the first effect that needed this and
   established the pattern: mirror `daemon::daemon_binary_path`'s own
   candidate-list shape (env var override → path relative to the running
   binary → fixed absolute install candidates → a `CARGO_MANIFEST_DIR`-based
   compile-time fallback so `cargo test`/`make check`/an unpackaged
   `cargo run` never need an install step) rather than reaching for a
   Tauri path API that isn't reachable from this process. The asset itself
   needs a `bundle.resources` entry in `tauri.conf.json` *and* explicit
   `linux.deb.files`/`linux.rpm.files` entries (mirroring how the systemd
   unit/desktop file/metainfo XML are already installed to fixed absolute
   paths there) — bundling it isn't enough on its own if the daemon process
   needs to find it at a predictable path outside of Tauri's own resource
   resolution.

## The actual blocker for a Limiter/Compressor specifically

`fx_capability.rs::probe_capabilities` hardcodes `builtin_limiter: false`
(never even probed for), and `fx_validate::preflight` unconditionally
rejects any `Limiter`/`Compressor` stage:
> "Limiter/Compressor has no supported backing plugin on this system yet —
> disabled until one is available"

This isn't a Pipe Deck gap, it's a real PipeWire gap: **no PipeWire version
ships a builtin dynamics-processing filter** (verified against `man 7
libpipewire-module-filter-chain`'s "Biquads"/"Parametric EQ"/"Mixer"/"Copy"
builtin list — nothing dynamics-shaped). The module *does* support LADSPA
and LV2 plugins in the same `filter.graph` (`type = ladspa` / `type = lv2`,
see the man page's plugin examples), the same way `fx_capability.rs`
already probes for a LADSPA noise-gate plugin
(`ladspa_noise_gate`/`find_ladspa_plugin`) for the (also currently blocked)
Noise Gate stage. Shipping a real Limiter means:

1. Picking (or requiring the user install) a specific LADSPA/LV2 limiter
   plugin, the same way noise gate probing looks for
   `librnnoise_ladspa.so`.
2. Extending `fx_capability::probe_capabilities` to actually probe for it
   (mirror the existing noise-gate probe).
3. Extending `render_filter_chain_module_args` (or adding a sibling
   render function) to emit a `type = ladspa` node instead of `type =
   builtin`, with `plugin`/`label`/`control` set from the discovered
   plugin's port names.
4. Removing the unconditional block in `fx_validate::preflight` once a
   plugin is actually found, gating on capability the same way noise gate
   does.

Everything *else* — hosting, live param push, reconciliation, the
create/connect/bypass engine ops, the Vue slider component pattern — is
already generic infrastructure from the EQ5Band work and needs no changes
for a new effect kind.
