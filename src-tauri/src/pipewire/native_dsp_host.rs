//! Portable DSP real-time host (issue #74) — runs the platform-agnostic
//! `dsp::DspChain` (biquad EQ, today; any future `DspStage`) against live
//! audio, instead of loading PipeWire's builtin `filter-chain` module the
//! way `native_host.rs` does. This is the one file in this codebase that
//! actually touches an audio sample.
//!
//! ## Capture: `pw::stream`. Outlet: raw `pw_filter` FFI.
//!
//! The `pipewire` crate this project depends on (0.10) has no `filter`
//! binding — only `stream` (confirmed by inspecting its source; there is no
//! `filter` module). Two independent `pw::stream`s (one capture, one
//! playback, joined only by an internal ring buffer — the same shape
//! PipeWire's own builtin `module-loopback` uses) were tried first and
//! process real audio correctly, but the *outlet* specifically could not be
//! made to carry `media.class = Audio/Source/Virtual` — see below. The
//! *capture* side has no such problem and stays a plain `pw::stream`
//! (`media.class = Audio/Sink`, works exactly as `module-loopback`'s own
//! capture side does). The outlet is rebuilt on raw `pipewire-sys`/`libspa-sys`
//! FFI against `pipewire/filter.h`'s `pw_filter_*` C API instead — the same
//! mechanism `libpipewire-module-filter-chain` itself is built on, and the
//! same "raw FFI, no safe wrapper" pattern `native_host.rs` already uses for
//! module loading. `pipewire-sys` already generates full bindings for every
//! `pw_filter_*` symbol (it's part of the same header set `pw_stream_*`
//! comes from) — nothing needed vendoring or hand-declaring.
//!
//! ## Why the outlet needed `pw_filter`, not `pw::stream`
//!
//! Confirmed live: giving a `Direction::Output` `pw::stream` `media.class =
//! Audio/Source/Virtual` made PipeWire's `audioconvert` adapter (the thing
//! that backs a plain client stream) create **zero output ports**
//! (`max-output-ports: 129`, `n-output-ports: 0` in `pw-dump`) — nothing
//! could ever link to it, silently. A `pw_filter`'s ports are explicit —
//! this module calls `pw_filter_add_port` itself, once per channel, rather
//! than relying on an adapter to conjure ports from a stream "role" — so it
//! isn't subject to that failure mode. Each port is a mono `F32` "DSP"
//! buffer (`pw_filter_get_dsp_buffer`, negotiated via the port property
//! `PW_KEY_FORMAT_DSP = "32 bit float mono audio"`, not a connect-time
//! `spa_pod` format array the way `pw::stream` needs) — confirmed against
//! PipeWire's own `audio-dsp-filter.c` example, the canonical pattern for
//! exactly this "N mono DSP ports on one filter" shape.
//!
//! ## Naming / identity (PD-020 swap-by-identity)
//!
//! Device-attached effect chains only ever reach virtual **input** (mic)
//! devices today (`core::engine::effects_ops::apply_effect_chain_structural`'s
//! own direction gate) — this module only implements that capture-direction
//! template, mirroring `fx_validate::render_conf_capture`'s naming:
//! - `effect_input.<device_system_name>` — the raw inlet (`pw::stream`,
//!   `media.class = Audio/Sink` — required, or the session manager's
//!   default policy never links other apps' `target.object` requests to it
//!   at all, confirmed live). Matches the builtin-module capture template's
//!   own `capture.props`.
//! - `<device_system_name>` itself — the processed outlet (`pw_filter`,
//!   `media.class = Audio/Source/Virtual` on the filter's own node props),
//!   taking over the device's own identity so everything already routed to
//!   it keeps working — the builtin-module path's swap-by-identity
//!   contract, matching its `playback.props`.
//!
//! ## Real-time safety
//!
//! `process` callbacks never allocate, lock, or block. Chain swaps
//! (structural apply *and* live param pushes both — rebuilding 5 biquad
//! coefficient sets off the audio thread is cheap enough that unifying both
//! paths through one "push a whole new chain" mechanism isn't worth a
//! separate fast path) arrive via a lock-free `ringbuf` channel; the
//! *old* chain is hop over to a second ring rather than dropped in-place
//! (`Box`/`Vec` deallocation is not real-time safe either), and is dropped
//! back on the control thread by `drain_dropped_chains`.
//!
//! ## Connection ownership — shares `native_host`'s, doesn't open its own
//!
//! An earlier version of this module opened its own independent
//! `ThreadLoopRc`/`ContextRc`/`CoreRc` (mirroring `native_host.rs`'s own
//! pattern, and PD-042/PD-043's "give each native subsystem its own
//! process-wide connection" convention). That worked in every isolated,
//! in-process test but hung indefinitely — confirmed via `strace`: the
//! daemon's per-connection handler thread made *zero* syscalls for 8
//! straight seconds inside `load_chain`, a pure userspace stall, not a wait
//! on anything identifiable — as soon as it was exercised through the real
//! daemon, which also runs `native_host`'s own connection and
//! `backend::linux::live::LinuxPipeWireBackend`'s graph-watching connection
//! concurrently. The exact interaction was never root-caused (no `gdb` in
//! the environment this was debugged in; a "call `.connect()` outside the
//! lock" hypothesis was tried and proven wrong by libpipewire's own runtime
//! check — `.connect()` genuinely requires the lock held, like everything
//! else here). Rather than keep guessing at a second connection's
//! interaction with two other concurrent ones, this module now calls
//! `native_host::shared_connection()` and reuses that single,
//! already-production-proven connection instead — one fewer independent
//! moving part. If a future change reintroduces a second connection here,
//! re-verify against a real daemon (not just an isolated test binary)
//! before trusting it.
//!
//! ## Scope / known limitation — not `pactl`/system-mic-picker visible
//!
//! The outlet does **not** appear in `pactl list sources`, and so cannot be
//! selected as an input device by other apps (Discord, a browser, OBS) the
//! way a real virtual mic can. Confirmed live and root-caused: `pactl`
//! visibility isn't gated by properties or port count (the *builtin-module*
//! outlet has zero output ports yet still enumerates as a source) — it's
//! gated by *how the node was created*. The builtin module's outlet carries
//! `library.name = audioconvert/libspa-audioconvert`, `factory.id`,
//! `adapt.follower.spa-node`, `node.group`/`link-group` — properties that
//! only appear on nodes created via PipeWire's `adapter` factory
//! (`pw_context_create_object("adapter", ...)`, the same mechanism
//! `backend::linux::pw_virtual_device_native` uses for real virtual
//! devices, PD-044) or via a genuinely loaded module
//! (`pw_context_load_module`, what `native_host.rs` does). A client-created
//! `pw_filter` (this module) or `pw::stream` is a fundamentally different
//! kind of object from PipeWire's perspective — no property this module can
//! set changes that.
//!
//! This does **not** block Pipe Deck's own routing: everything here is
//! linkable by name via `pw-link`, verified live end to end (a real source
//! → this capture inlet → `DspChain` → this outlet → a real destination).
//! Only *external, non-Pipe-Deck apps'* own device pickers are affected.
//! Tracked as a separate, deliberately-scoped-out follow-up issue (search
//! "Voice Changer on Linux") rather than solved here — getting real
//! `pactl` visibility for **processed** (not passthrough) audio likely
//! needs a genuinely loaded module (real SPA-node-ABI `.so`, not a client
//! connection), a materially bigger undertaking than this file.
//!
//! Also fixed at exactly 2 channels (stereo, `front-left`/`front-right`) —
//! matching the only channel map `backend::linux::pactl::virtual` ever
//! creates a Pipe Deck virtual device with today
//! (`channel_map=front-left,front-right`). A future non-stereo virtual
//! device would need this generalized; not attempted here since it isn't
//! reachable from any current code path. `AUDIO_RING_FRAMES` (the
//! capture-to-playback loopback latency) has **not** been tuned against a
//! sustained real-world load yet — see docs/architecture/Decisions.md
//! PD-051 and the live-sandbox verification pass this issue requires
//! before merge.

use crate::core::models::EffectChainConfig;
use crate::dsp::{DspChain, DspStage};
use pipewire as pw;
use pipewire::properties::properties;
use pipewire::spa;
use pipewire::spa::param::audio::{AudioFormat, AudioInfoRaw};
use pipewire::spa::pod::Pod;
use pipewire::spa::sys as spa_sys;
use pipewire::sys as pw_sys;
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::{Mutex, OnceLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeDspHostError {
    #[error("failed to create capture stream for {0}")]
    CaptureStreamFailed(String),
    #[error("failed to create playback filter for {0}")]
    PlaybackFilterFailed(String),
    #[error("failed to add a port to the playback filter for {0}")]
    PlaybackPortFailed(String),
    #[error("failed to connect the playback filter for {0}")]
    PlaybackConnectFailed(String),
    #[error("failed to connect stream for {0}: {1}")]
    ConnectFailed(String, String),
    #[error("failed to build a Format pod: {0}")]
    PodBuildFailed(String),
}

/// Fixed at stereo — see module doc "Scope / known limitation".
const HOST_CHANNELS: usize = 2;

/// Capture-to-playback loopback latency, in frames. Needs live tuning (see
/// module doc) — too small underruns (audible dropouts, `PlaybackState`
/// falls back to silence on an empty ring), too large adds needless
/// end-to-end latency for what's meant to be a live effect.
const AUDIO_RING_FRAMES: usize = 8192;

/// One independent `DspChain` per channel — each channel needs its own
/// filter memory (`BiquadState`), never shared, or left/right audio would
/// bleed into each other's filter history.
type ChannelChains = Vec<DspChain>;

fn build_channel_chains(sample_rate_hz: f64, config: &EffectChainConfig) -> ChannelChains {
    (0..HOST_CHANNELS).map(|_| DspChain::from_config(sample_rate_hz, config)).collect()
}

struct CaptureState {
    chains: Box<ChannelChains>,
    chain_rx: HeapCons<Box<ChannelChains>>,
    chain_drop_tx: HeapProd<Box<ChannelChains>>,
    audio_tx: HeapProd<f32>,
}

/// User data for the raw `pw_filter` playback outlet — see the module doc's
/// "Outlet: raw `pw_filter`" section for why this is hand-written FFI
/// instead of the safe `pw::stream` API `CaptureState` uses.
struct PlaybackFilterUserData {
    audio_rx: HeapCons<f32>,
    /// One `pw_filter` port-data pointer per channel (`HOST_CHANNELS`),
    /// filled in by `pw_filter_add_port` before this is constructed. Each
    /// port carries exactly one channel's mono `F32` DSP buffer — the
    /// `pw_filter_get_dsp_buffer` convention, not an interleaved buffer the
    /// way `pw::stream` used.
    ports: [*mut c_void; HOST_CHANNELS],
}

// SAFETY: only ever touched from the PipeWire thread-loop's own dispatch
// thread (the `process` callback) or under `host()`'s mutex during
// setup/teardown — never concurrently.
unsafe impl Send for PlaybackFilterUserData {}

/// Owns the raw `pw_filter` outlet: the filter pointer itself, the
/// heap-boxed `spa_hook` PipeWire writes its listener bookkeeping into (must
/// stay at a stable address for the filter's whole life — a `Box`'s heap
/// allocation doesn't move even when the `Box` itself does, so this is safe
/// to relocate as part of `LoadedChain`), and the boxed user data `process`
/// reads via the raw pointer handed to `pw_filter_add_listener`.
struct PlaybackFilter {
    filter: *mut pw_sys::pw_filter,
    _hook: Box<spa_sys::spa_hook>,
    _user_data: Box<PlaybackFilterUserData>,
}

// SAFETY: same reasoning as `PlaybackFilterUserData` — every touch of
// `filter`/`_hook` happens under `host()`'s mutex with `thread_loop.lock()`
// held (construction, `pw_filter_destroy` in `Drop`), matching every other
// raw `pw::*` call in this codebase's contract.
unsafe impl Send for PlaybackFilter {}

impl Drop for PlaybackFilter {
    fn drop(&mut self) {
        // SAFETY: `self.filter` was returned by a successful `pw_filter_new`
        // and never destroyed elsewhere. Caller (`unload_chain`) already
        // holds `thread_loop.lock()` when this drops, matching every other
        // `pw::*` teardown in this codebase.
        unsafe { pw_sys::pw_filter_destroy(self.filter) };
    }
}

/// The playback filter's `process` event — real-time: no allocation, no
/// locking, no blocking. Reads `position.clock.duration` for the frame
/// count this cycle (the `spa_io_position` convention every `pw_filter`
/// DSP example uses, since a `pw_filter` port has no `chunk`/`stride` the
/// way a `pw::stream` buffer does — `pw_filter_get_dsp_buffer` returns a
/// flat `f32` array of exactly that many samples), then de-interleaves from
/// the shared ring buffer frame-major/channel-minor — matching
/// `CaptureState`'s own push order exactly, so channel identity survives
/// the ring round trip.
unsafe extern "C" fn playback_filter_process(data: *mut c_void, position: *mut spa_sys::spa_io_position) {
    if data.is_null() || position.is_null() {
        return;
    }
    // SAFETY: `data` is the pointer this module itself handed to
    // `pw_filter_add_listener`, pointing at a `Box<PlaybackFilterUserData>`
    // kept alive by the owning `PlaybackFilter` for exactly as long as this
    // filter can be calling back into it (destroyed together).
    let user_data = unsafe { &mut *(data as *mut PlaybackFilterUserData) };
    // SAFETY: `position` is a valid pointer PipeWire itself provides for
    // the duration of this call.
    let n_samples = unsafe { (*position).clock.duration } as usize;

    let mut buffers: [*mut f32; HOST_CHANNELS] = [std::ptr::null_mut(); HOST_CHANNELS];
    for (channel, &port_data) in user_data.ports.iter().enumerate() {
        // SAFETY: `port_data` came from a successful `pw_filter_add_port`
        // for this same filter, still valid (the port outlives the filter,
        // which outlives this callback).
        buffers[channel] = unsafe { pw_sys::pw_filter_get_dsp_buffer(port_data, n_samples as u32) as *mut f32 };
    }

    for frame in 0..n_samples {
        for &buf_ptr in &buffers {
            // Underrun (ring empty) falls back to silence, never
            // uninitialized/garbage memory — an audible dropout is the
            // correct, safe degradation; garbage samples are not.
            let sample = user_data.audio_rx.try_pop().unwrap_or(0.0);
            if !buf_ptr.is_null() {
                // SAFETY: `buf_ptr` (when non-null) points at a
                // `pw_filter_get_dsp_buffer`-returned array of at least
                // `n_samples` `f32`s; `frame < n_samples`.
                unsafe { *buf_ptr.add(frame) = sample };
            }
        }
    }
}

/// `'static` — see `create_playback_filter`'s `pw_filter_add_listener` call
/// for why this can't be a stack local.
static PLAYBACK_FILTER_EVENTS: pw_sys::pw_filter_events = pw_sys::pw_filter_events {
    version: pw_sys::PW_VERSION_FILTER_EVENTS,
    destroy: None,
    state_changed: None,
    io_changed: None,
    param_changed: None,
    add_buffer: None,
    remove_buffer: None,
    process: Some(playback_filter_process),
    drained: None,
    command: None,
};

/// Builds the raw `pw_filter` playback outlet for `device_system_name` —
/// `media.class = Audio/Source/Virtual` on this filter's own node props
/// (unlike the `pw::stream` attempt this replaced, a `pw_filter`'s ports are
/// explicit — created by this function calling `pw_filter_add_port`, never
/// implicitly by an `audioconvert` adapter — so it isn't subject to the
/// adapter-creates-zero-ports failure mode that broke `media.class` on a
/// plain output stream). Must be called with `thread_loop.lock()` held,
/// same contract as every other raw `pw::*` call in this codebase.
fn create_playback_filter(
    core: &pw::core::CoreRc,
    device_system_name: &str,
    audio_rx: HeapCons<f32>,
) -> Result<PlaybackFilter, NativeDspHostError> {
    let name_c = CString::new(device_system_name)
        .map_err(|_| NativeDspHostError::PlaybackFilterFailed(device_system_name.to_string()))?;

    // Matches the builtin-module outlet's own prop set exactly (diffed live
    // against a real `libpipewire-module-filter-chain` node via `pw-dump`):
    // no `media.category` at all, but `node.virtual = true` — that property,
    // not port count or `media.class` alone, is what actually makes
    // pipewire-pulse enumerate this as a real `pactl` source. Confirmed the
    // builtin module's own outlet has *zero* output ports yet still shows
    // up in `pactl list sources`, so port count was never the gate.
    // `media.category = Playback` matters for *this* (client-created)
    // node, unlike the builtin-module's adapter-created one: removing it to
    // match the builtin module's own prop set (chasing `pactl` visibility,
    // which it turned out not to affect either way) broke the session
    // manager's policy-based autoconnect for other apps' `target.object`
    // links — confirmed live (an app's own `pw-cat --record --target=...`
    // stopped forming a link at all once this was removed). Restored.
    let filter_props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::MEDIA_CLASS => "Audio/Source/Virtual",
        *pw::keys::NODE_NAME => device_system_name,
        *pw::keys::NODE_DESCRIPTION => format!("Pipe Deck Effects - {device_system_name}").as_str(),
        *pw::keys::NODE_VIRTUAL => "true",
        *pw::keys::AUDIO_CHANNELS => HOST_CHANNELS.to_string().as_str(),
    };
    // SAFETY: `core.as_raw_ptr()` is a valid, live core pointer (this
    // module's shared connection); `name_c`/`filter_props.into_raw()` are
    // freshly built, valid for this call. `pw_filter_new` takes ownership
    // of the properties pointer per its own doc ("ownership is taken").
    let filter =
        unsafe { pw_sys::pw_filter_new(core.as_raw_ptr(), name_c.as_ptr(), filter_props.into_raw()) };
    if filter.is_null() {
        return Err(NativeDspHostError::PlaybackFilterFailed(device_system_name.to_string()));
    }

    // One mono DSP port per channel — the `pw_filter_get_dsp_buffer`
    // convention (confirmed against PipeWire's own `audio-dsp-filter.c`
    // example: `PW_KEY_FORMAT_DSP = "32 bit float mono audio"` as a port
    // *property*, not a negotiated `spa_pod` format param the way
    // `pw::stream`'s capture side needs).
    const PORT_LABELS: [&str; HOST_CHANNELS] = ["FL", "FR"];
    let mut ports: [*mut c_void; HOST_CHANNELS] = [std::ptr::null_mut(); HOST_CHANNELS];
    for (channel, label) in PORT_LABELS.iter().enumerate() {
        let port_props = properties! {
            *pw::keys::FORMAT_DSP => "32 bit float mono audio",
            *pw::keys::PORT_NAME => format!("output_{label}").as_str(),
        };
        // SAFETY: `filter` was just confirmed non-null and is not shared
        // with any other thread while `thread_loop.lock()` is held.
        let port_data = unsafe {
            pw_sys::pw_filter_add_port(
                filter,
                spa_sys::SPA_DIRECTION_OUTPUT,
                pw_sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                0,
                port_props.into_raw(),
                std::ptr::null_mut(),
                0,
            )
        };
        if port_data.is_null() {
            // SAFETY: `filter` is still valid and owned solely by this
            // function up to this point; destroying it also tears down any
            // ports already added.
            unsafe { pw_sys::pw_filter_destroy(filter) };
            return Err(NativeDspHostError::PlaybackPortFailed(device_system_name.to_string()));
        }
        ports[channel] = port_data;
    }

    let mut user_data = Box::new(PlaybackFilterUserData { audio_rx, ports });
    // A `Box`'s heap allocation address is stable across the `Box` value
    // itself moving (e.g. into the `PlaybackFilter` this function returns)
    // — safe to hand this raw pointer to PipeWire now and move `user_data`
    // into the returned struct afterward.
    let user_data_ptr: *mut c_void = user_data.as_mut() as *mut PlaybackFilterUserData as *mut c_void;

    let mut hook: Box<spa_sys::spa_hook> = Box::new(unsafe { std::mem::zeroed() });
    // SAFETY: `filter`/`hook` valid. `PLAYBACK_FILTER_EVENTS` must be
    // `'static` — `pw_filter_add_listener` stores the *pointer* to this
    // struct in the hook's callback table (`spa_hook_list`'s own
    // `SPA_CALLBACKS_INIT` convention), not a copy of its contents. A
    // stack-local `events` here segfaulted on the very first live test: the
    // pointer PipeWire retained for every future `process` call outlived
    // this function's stack frame the moment it returned.
    unsafe {
        pw_sys::pw_filter_add_listener(filter, hook.as_mut() as *mut spa_sys::spa_hook, &PLAYBACK_FILTER_EVENTS, user_data_ptr);
    }

    // No format/latency params — a `pw_filter`'s DSP ports negotiate format
    // via each port's `PW_KEY_FORMAT_DSP` property above, not a connect-time
    // `spa_pod` array the way `pw::stream` needs.
    // SAFETY: `filter` valid, `hook`/`user_data` already registered as this
    // filter's listener.
    let connect_result =
        unsafe { pw_sys::pw_filter_connect(filter, pw_sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS, std::ptr::null_mut(), 0) };
    if connect_result < 0 {
        // SAFETY: destroys everything this function allocated; nothing else
        // references `filter` yet since it was never returned to a caller.
        unsafe { pw_sys::pw_filter_destroy(filter) };
        return Err(NativeDspHostError::PlaybackConnectFailed(device_system_name.to_string()));
    }

    Ok(PlaybackFilter { filter, _hook: hook, _user_data: user_data })
}

/// Everything kept alive for one loaded chain. Dropping this (via
/// `unload_chain` removing it from `HOST`'s map) disconnects and destroys
/// both the capture stream and the playback filter.
struct LoadedChain {
    _capture: pw::stream::StreamRc,
    _playback: PlaybackFilter,
    _capture_listener: pw::stream::StreamListener<CaptureState>,
    chain_tx: HeapProd<Box<ChannelChains>>,
    chain_drop_rx: HeapCons<Box<ChannelChains>>,
    sample_rate_hz: f64,
}

struct DspHost {
    thread_loop: pw::thread_loop::ThreadLoopRc,
    core: pw::core::CoreRc,
    loaded: HashMap<String, LoadedChain>,
}

// SAFETY: same reasoning as `native_host.rs`'s `NativeHost` — every access
// to the `pw::*` fields here goes through `thread_loop.lock()` first, matching
// `pw_thread_loop`'s own thread-affinity contract. `HeapProd`/`HeapCons` are
// `Send` by construction (ringbuf's `SharedRb` is designed to cross threads).
unsafe impl Send for DspHost {}

static HOST: OnceLock<Mutex<DspHost>> = OnceLock::new();

/// Reuses `native_host`'s connection (`shared_connection()`) rather than
/// opening a second, independent one — an earlier version of this module
/// did open its own, and hitting it through the real daemon (two concurrent
/// PipeWire thread-loops/connections in one process, alongside
/// `backend::linux::live::LinuxPipeWireBackend`'s own graph-watching
/// connection) produced a hang that never reproduced in isolation. Sharing
/// the one connection this codebase already runs in production
/// (PD-027/PD-029) removes that variable; this module's own state is just
/// its `loaded` bookkeeping now.
fn host() -> &'static Mutex<DspHost> {
    HOST.get_or_init(|| {
        let (thread_loop, core) = crate::pipewire::native_host::shared_connection();
        Mutex::new(DspHost { thread_loop, core, loaded: HashMap::new() })
    })
}

/// Forces the one-time setup above to happen now rather than inside the
/// first real request — same rationale and call site as
/// `native_host::warm_up`, which this itself now depends on (call that
/// first, or this transitively triggers it via `shared_connection()`).
pub fn warm_up() {
    host();
}

fn audio_info_pod(rate_hz: Option<u32>) -> Result<Vec<u8>, NativeDspHostError> {
    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_channels(HOST_CHANNELS as u32);
    if let Some(rate) = rate_hz {
        audio_info.set_rate(rate);
    }
    let mut position = [0u32; spa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    audio_info.set_position(position);

    pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: pw::spa::sys::SPA_TYPE_OBJECT_Format,
            id: pw::spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|error| NativeDspHostError::PodBuildFailed(format!("{error:?}")))
}

/// Whether `config` is fully expressible through this portable host —
/// today, exactly the "every stage is `Eq5Band`" shape
/// `dsp::DspChain::from_config` builds a real `DspStage` for. `false` for an
/// empty config too: there's nothing to gain from connecting a portable host
/// that would process nothing, so an inactive config just stays on whatever
/// path it's already on. Callers (`backend::linux::live`) use this to decide
/// whether to route a capture-direction (virtual input/mic) device's effect
/// chain through this module or through `native_host`'s builtin-module path.
pub fn supports(config: &EffectChainConfig) -> bool {
    !config.stages.is_empty()
        && config.stages.iter().all(|stage| matches!(stage, crate::core::models::EffectStage::Eq5Band { .. }))
}

/// Loads (or replaces) a real-time DSP chain for `device_system_name` — the
/// portable-DSP equivalent of `native_host::load_chain`, scoped to whatever
/// `dsp::DspChain::from_config` can actually build a stage for (today: only
/// `EffectStage::Eq5Band`). Returns the processed-outlet node name
/// (`device_system_name` itself — see module doc's naming section).
///
/// `sample_rate_hz` is fixed at connect time (48kHz is what every Pipe Deck
/// virtual device is created at — see `backend::linux::pactl::virtual`) —
/// unlike the builtin-module path, this does not attempt to follow a graph
/// rate change after the fact; a rate change would need a full
/// disconnect/reconnect, not attempted in this pass.
pub fn load_chain(device_system_name: &str, config: &EffectChainConfig) -> Result<String, NativeDspHostError> {
    const SAMPLE_RATE_HZ: f64 = 48000.0;

    unload_chain(device_system_name);

    let capture_name = format!("effect_input.{device_system_name}");
    let playback_name = device_system_name.to_string();

    let mut guard = host().lock().expect("native dsp host mutex poisoned");

    // Built inside a closure so the `thread_loop` lock guard(s) it holds are
    // dropped before `guard.loaded.insert` below needs an exclusive borrow
    // of `guard` — a lock guard's lifetime is tied to a shared borrow of
    // `guard` through `Deref`, which the borrow checker can't otherwise see
    // is disjoint from the `loaded` field.
    //
    // NOTE: an earlier version of this function dropped the lock before
    // calling `.connect()`, hypothesizing `.connect()` needed the loop
    // dispatching to complete. That was wrong and made things worse:
    // libpipewire's own runtime check immediately flagged it —
    // "pw_stream_connect called from wrong context, check thread and
    // locking" — confirming `.connect()`, like everything else here, must
    // run with the lock held. Reverted; the lock spans object creation AND
    // `.connect()` for both streams, matching every other `pw::*` call in
    // this codebase's contract.
    let build_result: Result<LoadedChain, NativeDspHostError> = (|| {
    // Pure Rust setup — no PipeWire calls, doesn't need the lock at all.
    // `chain_tx`/`chain_drop_rx` are kept in this outer scope for the final
    // `LoadedChain`; their other halves move into `CaptureState` below.
    let audio_ring = HeapRb::<f32>::new(AUDIO_RING_FRAMES * HOST_CHANNELS);
    let (audio_tx, audio_rx) = audio_ring.split();
    // Capacity 2 is enough — the capture callback only ever has at most one
    // *unconsumed* pending swap between polls (it drains on every callback),
    // a second slot just avoids a spurious `try_push` failure racing a swap
    // against a callback that hasn't run yet.
    let chain_ring = HeapRb::<Box<ChannelChains>>::new(2);
    let (chain_tx, chain_rx) = chain_ring.split();
    let drop_ring = HeapRb::<Box<ChannelChains>>::new(2);
    let (chain_drop_tx, chain_drop_rx) = drop_ring.split();
    let initial_chains = Box::new(build_channel_chains(SAMPLE_RATE_HZ, config));

    let _lock = guard.thread_loop.lock();

    // `media.class = Audio/Sink` matters, not just cosmetic: without it this
    // registers as a plain input *stream* (category "Stream/Input/Audio"),
    // which the session manager's default policy does not treat as a valid
    // link target for other apps' `target.object`-based autoconnect —
    // confirmed live (pw-cat's own autoconnect silently formed zero links
    // until this was added). Matches `fx_validate`'s builtin-module capture
    // template exactly (`capture.props { media.class = Audio/Sink }`),
    // which is how the pre-existing path already gets this right.
    let capture_props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_CLASS => "Audio/Sink",
        *pw::keys::NODE_NAME => capture_name.as_str(),
        *pw::keys::NODE_DESCRIPTION => format!("Pipe Deck Effects (in) - {device_system_name}").as_str(),
    };
    let capture = pw::stream::StreamRc::new(guard.core.clone(), &capture_name, capture_props)
        .map_err(|_| NativeDspHostError::CaptureStreamFailed(device_system_name.to_string()))?;

    let capture_state = CaptureState { chains: initial_chains, chain_rx, chain_drop_tx, audio_tx };
    let capture_listener = capture
        .add_local_listener_with_user_data(capture_state)
        .process(|stream, state: &mut CaptureState| {
            if let Some(new_chains) = state.chain_rx.try_pop() {
                let old = std::mem::replace(&mut state.chains, new_chains);
                // Best-effort: if the drop ring is somehow still full (a
                // second swap landed before the control thread drained the
                // first), this drops `old` right here rather than blocking
                // — a `Vec`/`Box` deallocation slipping onto the RT thread
                // in that rare race is judged better than ever blocking the
                // audio callback.
                let _ = state.chain_drop_tx.try_push(old);
            }

            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else { return };
            let stride = HOST_CHANNELS * std::mem::size_of::<f32>();
            let n_frames = data.chunk().size() as usize / stride;
            let Some(bytes) = data.data() else { return };

            for frame in 0..n_frames {
                for (channel, chain) in state.chains.iter_mut().enumerate() {
                    let start = frame * stride + channel * std::mem::size_of::<f32>();
                    let Some(sample_bytes) = bytes.get(start..start + 4) else { break };
                    let sample = f32::from_le_bytes(sample_bytes.try_into().unwrap_or_default());
                    let processed = chain.process(sample);
                    // Ring full means playback has fallen behind (an
                    // xrun-adjacent condition already) — dropping this
                    // sample rather than blocking keeps the callback
                    // real-time safe; a persistently-full ring is a tuning
                    // problem for `AUDIO_RING_FRAMES`, not something this
                    // callback can fix by waiting.
                    let _ = state.audio_tx.try_push(processed);
                }
            }
        })
        .register()
        .map_err(|_| NativeDspHostError::CaptureStreamFailed(device_system_name.to_string()))?;

    let playback = create_playback_filter(&guard.core, device_system_name, audio_rx)?;

    let format_bytes = audio_info_pod(Some(SAMPLE_RATE_HZ as u32))?;
    let format_pod = Pod::from_bytes(&format_bytes)
        .ok_or_else(|| NativeDspHostError::PodBuildFailed("serialized format pod bytes were malformed".into()))?;
    let mut capture_params = [format_pod];
    capture
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
            &mut capture_params,
        )
        .map_err(|error| NativeDspHostError::ConnectFailed(capture_name.clone(), error.to_string()))?;

        Ok(LoadedChain {
            _capture: capture,
            _playback: playback,
            _capture_listener: capture_listener,
            chain_tx,
            chain_drop_rx,
            sample_rate_hz: SAMPLE_RATE_HZ,
        })
    })();

    guard.loaded.insert(device_system_name.to_string(), build_result?);

    Ok(playback_name)
}

/// Unloads a previously loaded chain, disconnecting and destroying both
/// streams. A no-op if nothing is loaded for `device_system_name`.
pub fn unload_chain(device_system_name: &str) {
    let mut guard = host().lock().expect("native dsp host mutex poisoned");
    let Some(loaded) = guard.loaded.remove(device_system_name) else { return };
    // Streams must be disconnected/destroyed with the thread loop locked,
    // same contract as every other `pw::*` teardown in this codebase — drop
    // them explicitly inside the lock rather than relying on `loaded`'s own
    // `Drop` running at an unlocked, unspecified point.
    let _lock = guard.thread_loop.lock();
    drop(loaded);
}

/// Whether a portable-DSP chain is currently loaded for `device_system_name`.
pub fn is_loaded(device_system_name: &str) -> bool {
    host().lock().expect("native dsp host mutex poisoned").loaded.contains_key(device_system_name)
}

/// Pushes an updated chain (rebuilt off the audio thread from new params) to
/// an already-loaded device's real-time host — used for both the initial
/// load and every subsequent live param push, see module doc. Also drains
/// whatever chain(s) a *previous* swap displaced, since that's real-time-unsafe
/// to drop on the audio thread itself (see `CaptureState`'s swap handling).
pub fn set_live_chain(device_system_name: &str, config: &EffectChainConfig) -> Result<(), NativeDspHostError> {
    let mut guard = host().lock().expect("native dsp host mutex poisoned");
    let Some(loaded) = guard.loaded.get_mut(device_system_name) else {
        return Ok(());
    };
    let new_chains = Box::new(build_channel_chains(loaded.sample_rate_hz, config));
    let _ = loaded.chain_tx.try_push(new_chains);
    while loaded.chain_drop_rx.try_pop().is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diagnostic-only (issue #74): mirrors the daemon's own structure —
    /// `warm_up()` on one thread (matching daemon startup), then
    /// `load_chain` from a *different* spawned thread (matching a
    /// `thread::spawn`-per-IPC-connection handler) — to isolate whether
    /// crossing threads between `host()`'s lazy-init and its later use is
    /// what causes the hang observed when this is wired through the real
    /// daemon, independent of the socket/IPC machinery.
    #[test]
    #[ignore]
    fn load_chain_from_a_different_thread_than_warm_up_used() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        warm_up();

        let device_system_name = "pipe-deck-dsp-host-cross-thread-test";
        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 0,
                eq_bass: 6,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let handle = std::thread::spawn(move || load_chain(device_system_name, &config));
        let result = handle.join().expect("spawned thread panicked");
        unload_chain(device_system_name);
        result.expect("load_chain from a different thread than warm_up failed");
    }

    /// `#[ignore]`d: hits a real PipeWire session, same convention as every
    /// other `live_tests`-style module in this codebase. Confirms the two
    /// streams actually connect and appear live (not suspended) — the
    /// concrete thing that would mean this real-time host silently doesn't
    /// work at all, which no unit test above can catch.
    #[test]
    #[ignore]
    fn load_chain_creates_live_unsuspended_streams_and_unload_tears_them_down() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let device_system_name = "pipe-deck-dsp-host-smoke-test";
        let cleanup = || unload_chain(device_system_name);
        cleanup();

        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 0,
                eq_bass: 6,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let playback_name = match load_chain(device_system_name, &config) {
            Ok(name) => name,
            Err(error) => {
                cleanup();
                panic!("load_chain failed: {error}");
            }
        };
        assert_eq!(playback_name, device_system_name);
        assert!(is_loaded(device_system_name));

        std::thread::sleep(std::time::Duration::from_millis(500));

        let capture_name = format!("effect_input.{device_system_name}");
        let capture_id = crate::pipewire::pw_cli::find_node_id_by_name(&capture_name).ok().flatten();
        let playback_id = crate::pipewire::pw_cli::find_node_id_by_name(&playback_name).ok().flatten();

        if capture_id.is_none() || playback_id.is_none() {
            cleanup();
            panic!("expected both {capture_name:?} and {playback_name:?} to appear live via pw-dump; capture={capture_id:?} playback={playback_id:?}");
        }

        // Live param push exercises the chain-swap path end to end too.
        if let Err(error) = set_live_chain(device_system_name, &config) {
            cleanup();
            panic!("set_live_chain failed on an already-loaded chain: {error}");
        }

        cleanup();
        assert!(!is_loaded(device_system_name));
    }

    #[test]
    fn build_channel_chains_produces_one_chain_per_channel() {
        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 3,
                eq_bass: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        let chains = build_channel_chains(48000.0, &config);
        assert_eq!(chains.len(), HOST_CHANNELS);
    }
}
