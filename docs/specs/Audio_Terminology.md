# Audio Terminology

## Purpose

A reference glossary for the audio-engineering and PipeWire concepts Pipe Deck's UI and docs assume. Written for contributors and users who know Linux but not pro-audio/DAW vocabulary, and for pro-audio people who don't know PipeWire's names for the same ideas. Each entry notes how (or whether) Pipe Deck currently models it — see [System Architecture](../architecture/System_Architecture.md) and [PipeWire Design](../architecture/PipeWire_Design.md) for the implementation side.

## In Scope

- Core signal-flow vocabulary (bus, send/return, ducking, gain staging, etc.)
- PipeWire/session-manager terms as distinct from the generic audio-engineering terms they map to
- Effects/dynamics terminology used by the Effects view and `core/engine/effects_ops.rs`

## Out of Scope

- Exhaustive DSP/filter theory (biquad math, FFT internals)
- Terms specific to a single competing app's UI

---

## Signal path basics

**Source / sink** — PipeWire's names for "thing producing audio" and "thing consuming audio." A microphone or an app's playback stream is a source; speakers, headphones, or a virtual device you route into is a sink. Pipe Deck's `Device` model (`core/models.rs`) covers both directions with a capability flag rather than separate types.

**Stream** — A single running audio connection between an application and PipeWire — one Firefox tab, one Discord call, one music player instance. Distinct from a *device*: a stream is transient (it appears when the app starts playing/recording and disappears when it stops), a device is comparatively stable. This distinction is why rule matching (`core/rules/matching.rs`) has to identify streams by `app_name`/`media_name`/`executable` rather than a stable ID — see issue #116 on the limits of that.

**Node / port / link** — PipeWire's low-level graph primitives, one layer below Pipe Deck's `Device`/`Stream` model. A *node* is anything with audio ports (a device, a stream, a filter). A *port* is one mono audio terminal on a node (a stereo node has `FL`/`FR` or similar). A *link* connects one output port to one input port. Pipe Deck mostly hides these behind the normalized `RuntimeGraph`, but `pw_link.rs` deals with them directly when discovering and pairing ports for routing — it prefers discovering real port names via `pw-link -o`/`pw-link -i` and pairing them up over hardcoding stereo `FL`/`FR` suffixes, since a mono capture device (e.g. a headset mic reported as `...mono-fallback`) exposes a single `MONO` port that suffix-guessing silently fails to link.

**Playback vs. capture** — Playback is audio going *out* to a sink (an app plays a sound). Capture is audio coming *in* from a source (recording a mic, or an app capturing desktop audio). The same physical device is often both: a USB headset exposes a playback stream (headphones) and a capture stream (mic) as separate PipeWire nodes.

**Monitor** — Every PipeWire sink also exposes a read-only "monitor" port carrying a copy of whatever's being sent to that sink — this is how you tap "what's currently playing on this sink" without intercepting the original stream. Pipe Deck's virtual-sink → output routing (`pw-link` monitor→playback, per PipeWire Design) relies on this: a virtual sink's monitor is what actually gets linked onward to real outputs or a virtual mic.

**Loopback** — Routing a sink's output back into a source, so what plays "out" also arrives "in" — the mechanism underlying "route this app's audio into my mic" (application passthrough, #76) and the hidden **feed sink** pattern PipeWire Design describes.

## Levels and gain staging

**Gain / volume / level** — Loosely interchangeable, but *gain* usually means "amount of amplification applied at one point in the chain" (a mixer fader, a plugin's input trim) while *volume* is the user-facing single-number version of it. *Level* usually means "how loud is it right now" (a meter reading), not a control.

**Headroom** — The distance between the current signal level and the point where it clips (distorts from exceeding the max representable value). Leaving headroom (not running every fader near maximum) is why gain staging matters: if every stage in a chain runs close to its ceiling, small level increases anywhere clip immediately.

**Clipping** — Distortion from a signal exceeding the maximum level a stage can represent — the digital-audio equivalent of an over-driven amp, but harsher-sounding and non-euphonic. Caused by too much gain somewhere upstream, not (usually) fixable downstream.

**Peak vs. RMS** — Two different ways to measure "how loud": peak is the single highest instantaneous sample value (what tells you if you're about to clip); RMS (root-mean-square) is a windowed average that tracks perceived loudness better. A meter showing only peak can look quiet while still sounding loud, and vice versa.

**Pan / balance** — Positioning a signal across the stereo field. *Pan* usually implies starting from mono and placing it somewhere in the stereo image; *balance* usually implies adjusting the relative left/right level of an already-stereo signal. Tracked as issue #16 (part of [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182)) for per-channel control in Pipe Deck's effect chains.

**Mute vs. bypass** — Mute silences a channel entirely. Bypass (used for effects) skips a specific processing stage while leaving the signal otherwise flowing — the chain keeps passing audio through unprocessed rather than stopping it. Pipe Deck's per-device chain editor (#15) supports bypassing individual effects without removing them from the saved chain.

## Busses, sends, and mixing structure

**Bus** — A shared signal path that more than one source feeds into and that itself can be treated as a single thing to route, monitor, or process — the mixing-desk equivalent of a shared destination rather than a point-to-point cable. In Pipe Deck terms, a bus is a virtual device multiple sources route or send into.

**Submix** — A bus that groups a subset of sources (e.g. "all my game audio") before that group joins a larger master mix — useful for controlling a whole category's level with one fader instead of adjusting each source individually. Pipe Deck's chainable mixer nodes (issue #77 — per-source volume + one mixer feeding another) implement this pattern.

**Send / return** — A *send* takes a copy of a source's signal (usually at an independently adjustable level, separate from the source's main/"dry" output) and feeds it to a bus, typically one carrying a shared effect (reverb, compression). The *return* is that bus's processed output coming back into the main mix. The point of a send is that the source keeps going to its normal destination *and* contributes to the shared effect, rather than being rerouted through it exclusively — see issue #113 (effect bus with per-source send levels), not yet implemented.

**Master / main mix** — The final combined output everything eventually reaches, upstream of the physical output device. Not a distinct concept in Pipe Deck today — the "master" is whatever real sink(s) other things route to — but the term shows up in mixing-console mental models this app deliberately mirrors.

**Routing matrix** — A grid representation of "which sources connect to which destinations," used in Pipe Deck's Routing view as the visual/interaction model for what's ultimately a set of PipeWire links.

## Automation and dynamics

**Ducking** — Automatically lowering one source's level while another is active, then restoring it afterward — classically "music gets quieter while someone's talking," triggered by the second source's presence/level rather than a manual fader move. This is dynamics processing *driven by a different source's signal* (a sidechain, see below), not a static rule. Pipe Deck doesn't implement ducking today; it would need a live level-triggered volume adjustment on one device driven by activity on another, distinct from the rule engine's static app-identity matching (`core/rules/`).

**Sidechain** — Feeding a *second* signal into a dynamics processor (compressor/gate) to control how it acts on the *first* signal, instead of the processor reacting only to the signal it's processing. Ducking is the classic sidechain-compression use case: the compressor sits on the music bus, but its trigger input is the voice-chat stream.

**Compressor / limiter** — Dynamics processors that reduce a signal's level once it crosses a *threshold*, at a set *ratio* (compressor: gentler, proportional reduction; limiter: a compressor with a very high ratio acting as a hard ceiling that a signal effectively can't cross). Governed by *attack* (how fast it starts reducing once the threshold is crossed) and *release* (how fast it stops once the signal drops back below). Tracked for Pipe Deck as issue #86 (part of [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182)), currently blocked on finding a compliant backing plugin per `docs/architecture/Decisions.md`.

**Noise gate** — The inverse shape of a compressor: instead of reducing loud signal, it mutes/reduces signal *below* a threshold — used to silence a mic between words instead of letting background noise bleed through continuously. Tracked as issue #18 (part of [Epic #182](https://github.com/LunarVagabond/Pipe-Deck/issues/182)), implemented via a chained filter-chain module per `PipeWire Design`.

**Threshold / ratio / attack / release** — The four core dynamics-processor parameters: threshold sets the level where processing kicks in, ratio sets how strongly it reacts once triggered, attack/release set how fast it engages/disengages. The same four concepts apply to gates and compressors, just acting in opposite directions.

**Equalizer (EQ)** — Boosts or cuts specific frequency ranges (bass, mids, treble, or narrower bands) rather than the whole signal at once. Pipe Deck ships a 3-band biquad EQ live-processing path today (per `PipeWire Design`'s "what did work" section); parametric EQ with adjustable center frequency/bandwidth per band is tracked as #17.

**Effect chain** — An ordered sequence of processing stages (EQ, gate, compressor, ...) applied to one device's signal — the unit Pipe Deck's `EffectChainConfig`/chain editor (#15) operates on. Order matters: a gate before a compressor behaves very differently than a compressor before a gate.

**Filter chain (PipeWire-specific)** — PipeWire's own mechanism (`module-filter-chain`) for building an effect chain out of LADSPA/LV2/builtin plugin nodes inside the PipeWire graph itself, as opposed to processing audio in a separate application. This is the underlying implementation Pipe Deck's effect chains compile down to — see `pipewire/filter_chain.rs`.

## Plugins and processing backends

**LADSPA / LV2** — Two competing (LV2 is the newer, more capable successor) plugin standards for audio effects/processors on Linux, both loadable as filter-chain nodes. Pipe Deck depends on host-installed plugins of these kinds for anything beyond its handful of PipeWire-builtin effects (biquad EQ) — reducing that dependency is tracked as #21.

**Builtin (PipeWire) processing** — A small set of effects PipeWire itself ships without needing an external LADSPA/LV2 plugin installed — currently the only path Pipe Deck trusts for its "always works, no extra install" live-processing guarantee, per the safety contract in issue #64.

**Preflight / validation** — Checking a proposed effect-chain configuration for safety/compatibility *before* committing it to a live PipeWire config change and restart — Pipe Deck's `fx_validate.rs`, required after a past incident where an unvalidated filter-chain config crashed the PipeWire session (see PD-017 in `docs/architecture/Decisions.md`).

## Session/device management (PipeWire-specific)

**PipeWire vs. PulseAudio vs. WirePlumber** — PipeWire is the low-level media server handling the actual audio (and video) graph. PulseAudio is the older sound server PipeWire has mostly replaced, kept around as a compatibility protocol (`pactl`/`pipewire-pulse`) that most desktop apps still speak. WirePlumber is PipeWire's session manager — the policy layer deciding default devices, routing rules, and permissions on top of the graph PipeWire maintains. Pipe Deck shells out to `pactl`/`pw-link`/`pw-dump` rather than linking any of these libraries directly.

**Virtual device** — A software-only sink or source with no physical hardware behind it, created for routing purposes (e.g. "Discord Mix" as a fake microphone that's really several real sources mixed together). Pipe Deck's core building block for multi-source routing scenarios; see the Virtual Device Strategy section of [PipeWire Design](../architecture/PipeWire_Design.md). Pipe Deck's UI presents virtual outputs specifically as **busses** — see "Virtual Devices as Busses" in [UI Spec](../specs/UI_Spec.md) — including chaining one virtual output into another (submix into a master mix), while virtual inputs stay merge-only leaves.

**Default sink/source** — The device PipeWire/WirePlumber routes new streams to when nothing else specifies a destination. Distinct from a device merely being *connected* — a headset can be plugged in without being the default, so new app audio keeps going elsewhere.

**Latency / buffer / xrun** — Latency is the delay between a sound being generated and it reaching the output. Buffer size is the main lever controlling that trade-off (smaller buffer = lower latency, higher risk of glitches). An xrun (buffer under/overrun) is an audible glitch/dropout from the system failing to keep up with the buffer schedule in time — the thing you're trading against when pushing latency lower.

**Profile (Pipe Deck-specific)** — A saved snapshot of routing/device/effect-chain state as YAML (`config/store.rs`, `docs/specs/Config_Spec.md`) that can be restored later — not a PipeWire/WirePlumber concept, but Pipe Deck's own persistence unit layered on top of everything above.

## See also

- [PipeWire Design](../architecture/PipeWire_Design.md) — how these concepts map to Pipe Deck's actual discovery/routing implementation
- [Rule Engine Spec](../specs/Rule_Engine_Spec.md) — how streams get matched to rules using the identity fields mentioned above
- [Decisions](../architecture/Decisions.md) — PD-017 (effects safety contract) and related ADRs referenced throughout
