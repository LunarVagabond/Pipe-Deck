//! Research spike for issue #509: can Pipe Deck host genuinely hand-written,
//! compiled-in DSP (an envelope-following limiter, a first proof point
//! toward a real compressor for #86) inside a `pw_filter` process()
//! callback, running against a real PipeWire session — rather than only
//! ever loading PipeWire's own `module-filter-chain` builtins the way
//! `pipewire/native_host.rs` does today (#148/PD-027 and its addenda)?
//!
//! `pipewire-rs` 0.10 (the version this crate depends on) has no safe
//! wrapper for `pw_filter` — only `pw_stream` (`pipewire::stream::Stream`)
//! is bound. The raw `pw_filter_*` C API is, however, present in the
//! `pipewire-sys`-generated bindings (`pipewire::sys`), since
//! `pipewire/pipewire.h` transitively includes `pipewire/filter.h` and the
//! crate's bindgen allowlist is a permissive `pw_.*`. So this spike calls
//! `pw_filter_new_simple`/`pw_filter_add_port`/`pw_filter_connect`/
//! `pw_filter_get_dsp_buffer` directly via `pw_sys`, the same pattern
//! `native_host.rs` already uses for `pw_context_load_module` — nothing
//! here is unprecedented for this codebase, just a different corner of the
//! same FFI surface.
//!
//! Throwaway prototype only, not wired into the app. Run by hand against a
//! real PipeWire session (needs `libpipewire-0.3` dev headers + a running
//! `pipewire.service`; also shells out to `python3` to render a test tone
//! and `pw-cat`/`pw-link`/`pactl` to play it and inspect the live graph):
//!
//!     cargo run --example pw_filter_dsp_spike
//!
//! ## What this proves and how
//!
//! A hand-written hard limiter (`out = clamp(in, -threshold, threshold)`,
//! no lookahead/soft-knee — the "even just a hard limiter, as a first proof
//! point" scope #509 explicitly calls out) runs inside the filter's
//! `process()` callback. To make this a real end-to-end test against the
//! live session rather than a synthetic self-test:
//!
//! 1. A loud (0.9 amplitude) mono sine test tone is rendered to a temp raw
//!    PCM file and played into the system's *default sink* via `pw-cat
//!    --playback --target <default-sink>` — a real, audible (if briefly)
//!    stream in the actual running session, not our own local bookkeeping.
//! 2. This spike's filter node is created with one input DSP port and one
//!    output DSP port (`format.dsp = "32 bit float mono audio"`, the same
//!    prop-based DSP-port shape PipeWire's own `audio-dsp-filter.c` example
//!    uses — no manual SPA pod format negotiation needed).
//! 3. The filter's input port is `pw-link`ed to the default sink's
//!    `monitor_FL` port — i.e. real audio from a real playback stream,
//!    delivered through the real session's graph, not injected in-process.
//! 4. `process()` reads the real input buffer via `pw_filter_get_dsp_buffer`,
//!    applies the limiter per-sample, writes the result to the output DSP
//!    buffer, and records peak-in/peak-out/clamp-event counts in plain
//!    atomics — no allocation, no locking, nothing but pointer arithmetic
//!    and lock-free counter updates, which is what "real-time-safe" has to
//!    mean inside this callback (`PW_FILTER_FLAG_RT_PROCESS` is set, so this
//!    genuinely runs on the graph's realtime data thread, not the mainloop).
//! 5. The output port is deliberately left unlinked (no `pw-link` onward) —
//!    routing the limited signal audibly to real speakers isn't needed to
//!    answer the mechanism question and risks an unpleasant feedback/loop
//!    surprise for whoever runs this by hand.
//!
//! Findings from an actual run against a live session are recorded in
//! `docs/architecture/Decisions.md` PD-051.

use pipewire as pw;
use pipewire::properties::properties;
use pipewire::sys as pw_sys;
use std::ffi::{c_void, CString};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

const SAMPLE_RATE: u32 = 48_000;
const TONE_FREQ_HZ: f32 = 1_000.0;
const TONE_AMPLITUDE: f32 = 0.9;
const TONE_DURATION_SECS: u32 = 3;
const LIMITER_THRESHOLD: f32 = 0.3;
const NODE_NAME: &str = "pipe-deck-spike-dsp-limiter";

/// Everything `process()` needs, reachable only through the raw `data`
/// pointer `pw_filter_new_simple` hands back to us — this is the "user
/// data" half of the C callback convention `native_host.rs`'s module
/// callbacks already use, just for a per-sample callback instead of a
/// one-shot load/unload.
struct FilterState {
    in_port: *mut c_void,
    out_port: *mut c_void,
    process_calls: AtomicU64,
    samples_processed: AtomicU64,
    peak_in_bits: AtomicU32,
    peak_out_bits: AtomicU32,
    clamp_events: AtomicU64,
}

// SAFETY: only ever touched from `process()` (the realtime data thread,
// serialized by PipeWire itself — never called concurrently with itself)
// and, after `pw_filter_disconnect`, from this spike's own main thread once
// the realtime thread can no longer be invoking it.
unsafe impl Sync for FilterState {}

fn atomic_max_f32(slot: &AtomicU32, candidate: f32) {
    let candidate_bits = candidate.abs().to_bits();
    let mut current = slot.load(Ordering::Relaxed);
    while f32::from_bits(current) < f32::from_bits(candidate_bits) {
        match slot.compare_exchange_weak(
            current,
            candidate_bits,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

unsafe extern "C" fn on_process(data: *mut c_void, position: *mut pw::spa::sys::spa_io_position) {
    let state = &*(data as *const FilterState);
    let n_samples = if position.is_null() {
        0
    } else {
        (*position).clock.duration as u32
    };
    if n_samples == 0 {
        return;
    }

    state.process_calls.fetch_add(1, Ordering::Relaxed);

    let in_ptr = pw_sys::pw_filter_get_dsp_buffer(state.in_port, n_samples) as *const f32;
    let out_ptr = pw_sys::pw_filter_get_dsp_buffer(state.out_port, n_samples) as *mut f32;
    if out_ptr.is_null() {
        return;
    }

    state
        .samples_processed
        .fetch_add(n_samples as u64, Ordering::Relaxed);

    for i in 0..n_samples as isize {
        // An unlinked (or not-yet-linked) input port legitimately yields
        // null — real-time code must handle that as silence, not crash.
        let sample = if in_ptr.is_null() {
            0.0
        } else {
            *in_ptr.offset(i)
        };
        atomic_max_f32(&state.peak_in_bits, sample);

        let limited = sample.clamp(-LIMITER_THRESHOLD, LIMITER_THRESHOLD);
        if limited != sample {
            state.clamp_events.fetch_add(1, Ordering::Relaxed);
        }
        atomic_max_f32(&state.peak_out_bits, limited);

        *out_ptr.offset(i) = limited;
    }
}

fn render_sine_tone_file() -> std::io::Result<std::path::PathBuf> {
    let path = std::env::temp_dir().join("pipe-deck-spike-509-tone.f32le");
    let n_samples = SAMPLE_RATE * TONE_DURATION_SECS;
    let script = format!(
        "import struct, math, sys\n\
         n = {n_samples}\n\
         rate = {SAMPLE_RATE}\n\
         freq = {TONE_FREQ_HZ}\n\
         amp = {TONE_AMPLITUDE}\n\
         with open(sys.argv[1], 'wb') as f:\n\
         \tfor i in range(n):\n\
         \t\tv = amp * math.sin(2 * math.pi * freq * i / rate)\n\
         \t\tf.write(struct.pack('<f', v))\n"
    );
    let status = Command::new("python3")
        .arg("-c")
        .arg(&script)
        .arg(&path)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("python3 tone render failed"));
    }
    Ok(path)
}

fn default_sink_name() -> Option<String> {
    let output = Command::new("pactl")
        .arg("get-default-sink")
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn play_tone_into_default_sink(tone_path: &std::path::Path, sink: &str) -> std::io::Result<Child> {
    Command::new("pw-cat")
        .args([
            "--playback",
            "--raw",
            "--format",
            "f32",
            "--rate",
            &SAMPLE_RATE.to_string(),
            "--channels",
            "1",
            "--target",
            sink,
        ])
        .arg(tone_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

/// `pw-link -i`/`-o` output is `node:port`, one per line — same discovery
/// approach `backend/linux/pw_link.rs` already uses for real routing.
const NULL_SINK_NAME: &str = "pipe_deck_spike_509_sink";

/// A throwaway mono null sink to link our filter's output port into — a
/// completely unlinked output turned out to matter (see PD-051): the
/// driver never gives an unconnected-on-both-sides node a real scheduled
/// quantum, so `spa_io_position.clock.duration` stayed 0 forever without
/// this. Returns the `pactl` module id for cleanup.
fn load_throwaway_null_sink() -> Option<String> {
    let output = Command::new("pactl")
        .args([
            "load-module",
            "module-null-sink",
            &format!("sink_name={NULL_SINK_NAME}"),
            "sink_properties=device.description=PipeDeckSpike509Sink",
            "rate=48000",
            "channels=1",
            "channel_map=mono",
        ])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn unload_module(module_id: &str) {
    let _ = Command::new("pactl")
        .args(["unload-module", module_id])
        .status();
}

fn list_ports(direction_flag: &str) -> Vec<String> {
    let Ok(output) = Command::new("pw-link").arg(direction_flag).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn main() {
    println!("=== Pipe Deck pw_filter hand-written DSP spike (issue #509) ===");

    let tone_path = render_sine_tone_file().expect("failed to render test tone");
    let sink = default_sink_name();
    println!("Default sink: {sink:?}");

    pw::init();
    run(&tone_path, sink.as_deref());
    // Same lifecycle hazard PD-027 already documented for the module-loading
    // spike: `pw::deinit()` while any pw object backed by this loop is still
    // alive segfaults on exit. Everything pw-owned is dropped/destroyed
    // inside `run()` before this returns.
    unsafe { pw::deinit() };

    let _ = std::fs::remove_file(&tone_path);
}

fn run(tone_path: &std::path::Path, sink: Option<&str>) {
    let state = Box::into_raw(Box::new(FilterState {
        in_port: std::ptr::null_mut(),
        out_port: std::ptr::null_mut(),
        process_calls: AtomicU64::new(0),
        samples_processed: AtomicU64::new(0),
        peak_in_bits: AtomicU32::new(0),
        peak_out_bits: AtomicU32::new(0),
        clamp_events: AtomicU64::new(0),
    }));

    // A plain `pw::main_loop::MainLoopRc`, manually `iterate()`d in bursts
    // from this function (the first thing tried here), reproduces #303
    // exactly: `process()` gets called, but `spa_io_position.clock.duration`
    // stays 0 on every call, so no samples ever actually flow — the graph
    // driver isn't reliably scheduling this client because nothing is
    // continuously servicing its loop the rest of the time. Switching to a
    // `ThreadLoopRc`, started once and left running on its own OS thread for
    // the filter's whole life (the same fix `native_host.rs` already
    // documents for module-hosted effects), resolves it here too — see
    // PD-051 for the measured before/after.
    let thread_loop =
        unsafe { pw::thread_loop::ThreadLoopRc::new(Some("pipe-deck-spike-509"), None) }
            .expect("failed to create PipeWire thread loop");
    thread_loop.start();

    // Plain string literals rather than the raw `pw_sys::PW_KEY_*` byte
    // constants — those are NUL-terminated C string bytes
    // (`b"media.type\0"`), and `Properties::insert` builds a `CString` from
    // whatever it's given, which panics on any embedded NUL (including a
    // trailing one).
    let filter_props = properties! {
        "media.type" => "Audio",
        "media.category" => "Filter",
        "media.role" => "DSP",
        "media.name" => "Pipe Deck DSP Limiter Spike",
        "node.name" => NODE_NAME,
    };
    let filter_name = CString::new(NODE_NAME).unwrap();

    let events = pw_sys::pw_filter_events {
        version: pw_sys::PW_VERSION_FILTER_EVENTS,
        destroy: None,
        state_changed: None,
        io_changed: None,
        param_changed: None,
        add_buffer: None,
        remove_buffer: None,
        process: Some(on_process),
        drained: None,
        command: None,
    };

    // Setup calls on a `ThreadLoopRc` must happen with the loop locked, per
    // `pw_thread_loop`'s own contract (same requirement `native_host.rs`
    // documents) — released before the sleeps below so the background
    // thread can actually make progress while we wait.
    let filter = {
        let _lock = thread_loop.lock();
        let filter = unsafe {
            pw_sys::pw_filter_new_simple(
                thread_loop.loop_().as_raw_ptr(),
                filter_name.as_ptr(),
                filter_props.into_raw(),
                &events,
                state as *mut c_void,
            )
        };
        assert!(!filter.is_null(), "pw_filter_new_simple returned NULL");

        let in_props = properties! {
            "format.dsp" => "32 bit float mono audio",
            "port.name" => "input",
        };
        let out_props = properties! {
            "format.dsp" => "32 bit float mono audio",
            "port.name" => "output",
        };

        let in_port = unsafe {
            pw_sys::pw_filter_add_port(
                filter,
                pw::spa::sys::SPA_DIRECTION_INPUT,
                pw_sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_NONE,
                0,
                in_props.into_raw(),
                std::ptr::null_mut(),
                0,
            )
        };
        let out_port = unsafe {
            pw_sys::pw_filter_add_port(
                filter,
                pw::spa::sys::SPA_DIRECTION_OUTPUT,
                pw_sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_NONE,
                0,
                out_props.into_raw(),
                std::ptr::null_mut(),
                0,
            )
        };
        assert!(
            !in_port.is_null() && !out_port.is_null(),
            "pw_filter_add_port failed"
        );

        unsafe {
            (*state).in_port = in_port;
            (*state).out_port = out_port;
        }

        let connect_result = unsafe {
            pw_sys::pw_filter_connect(
                filter,
                pw_sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS,
                std::ptr::null_mut(),
                0,
            )
        };
        assert_eq!(connect_result, 0, "pw_filter_connect failed");
        filter
    };

    // Let async node/port setup finish, same ~1s PD-027's spike found
    // necessary before the node is visible to other processes — the thread
    // loop is running continuously in the background the whole time now, so
    // this is a plain sleep rather than a manual `iterate()` poll.
    std::thread::sleep(Duration::from_millis(1000));

    let our_input_ports: Vec<String> = list_ports("-i")
        .into_iter()
        .filter(|p| p.starts_with(NODE_NAME))
        .collect();
    let our_output_ports: Vec<String> = list_ports("-o")
        .into_iter()
        .filter(|p| p.starts_with(NODE_NAME))
        .collect();
    println!("Our filter's real input ports (pw-link -i): {our_input_ports:?}");
    println!("Our filter's real output ports (pw-link -o): {our_output_ports:?}");
    let node_is_real = !our_input_ports.is_empty() && !our_output_ports.is_empty();
    println!("Node appeared with real ports in the live session? {node_is_real}");

    let null_sink_module_id = load_throwaway_null_sink();
    std::thread::sleep(Duration::from_millis(500));
    let mut linked_to_real_sink = false;
    if let (Some(_), Some(our_output)) = (&null_sink_module_id, our_output_ports.first()) {
        let sink_input_port = list_ports("-i")
            .into_iter()
            .find(|p| p.starts_with(NULL_SINK_NAME));
        if let Some(sink_input_port) = sink_input_port {
            println!("Linking {our_output} -> {sink_input_port}");
            let link_status = Command::new("pw-link")
                .args([our_output, &sink_input_port])
                .status();
            linked_to_real_sink = matches!(link_status, Ok(s) if s.success());
        }
        println!("Output port linked to a real downstream consumer? {linked_to_real_sink}");
    } else {
        println!("Failed to create a throwaway null sink for the output port");
    }

    let mut tone_child = None;
    let mut linked_to_real_source = false;
    if let Some(sink) = sink {
        match play_tone_into_default_sink(tone_path, sink) {
            Ok(child) => tone_child = Some(child),
            Err(e) => println!("Failed to spawn pw-cat playback: {e}"),
        }
        // Give pw-cat a moment to connect its own stream before we look for
        // the sink's monitor port to link from.
        std::thread::sleep(Duration::from_millis(500));

        let monitor_port = list_ports("-o")
            .into_iter()
            .find(|p| p.starts_with(sink) && p.contains("monitor") && p.contains("FL"));
        if let (Some(monitor_port), Some(our_input)) = (monitor_port, our_input_ports.first()) {
            println!("Linking {monitor_port} -> {our_input}");
            let link_status = Command::new("pw-link")
                .args([&monitor_port, our_input])
                .status();
            linked_to_real_source = matches!(link_status, Ok(s) if s.success());
            println!("Link to real live-session source succeeded? {linked_to_real_source}");
        } else {
            println!("Could not find a default-sink monitor_FL port to link from");
        }
    } else {
        println!("No default sink found — skipping live-source link, testing unconnected-input path only");
    }

    // Wait through the tone's duration so process() actually sees real
    // audio (if linked) flow through the callback.
    std::thread::sleep(Duration::from_secs(TONE_DURATION_SECS as u64));

    if let Some(mut child) = tone_child {
        let _ = child.kill();
        let _ = child.wait();
    }

    {
        let _lock = thread_loop.lock();
        unsafe { pw_sys::pw_filter_disconnect(filter) };
    }
    std::thread::sleep(Duration::from_millis(500));
    {
        let _lock = thread_loop.lock();
        unsafe { pw_sys::pw_filter_destroy(filter) };
    }
    thread_loop.stop();

    if let Some(module_id) = &null_sink_module_id {
        unload_module(module_id);
    }

    let state = unsafe { Box::from_raw(state) };
    let process_calls = state.process_calls.load(Ordering::Relaxed);
    let samples_processed = state.samples_processed.load(Ordering::Relaxed);
    let peak_in = f32::from_bits(state.peak_in_bits.load(Ordering::Relaxed));
    let peak_out = f32::from_bits(state.peak_out_bits.load(Ordering::Relaxed));
    let clamp_events = state.clamp_events.load(Ordering::Relaxed);

    println!("\n=== Summary ===");
    println!("Node had real, externally-visible ports: {node_is_real}");
    println!("Input port linked to a real live-session source: {linked_to_real_source}");
    println!("Output port linked to a real downstream consumer: {linked_to_real_sink}");
    println!("process() invocations: {process_calls}");
    println!("Samples processed: {samples_processed}");
    println!(
        "Peak input amplitude seen: {peak_in:.4} (test tone was rendered at {TONE_AMPLITUDE})"
    );
    println!(
        "Peak output amplitude produced: {peak_out:.4} (limiter threshold: {LIMITER_THRESHOLD})"
    );
    println!("Samples where the limiter actually engaged (clamp events): {clamp_events}");
    println!(
        "\nInterpretation: if 'real ports' is true, 'linked to a real live-session source' is \
         true, peak input is close to {TONE_AMPLITUDE} while peak output stays at or under \
         {LIMITER_THRESHOLD}, and clamp events is a large, nonzero share of samples processed, \
         that means hand-written DSP math inside a raw pw_filter process() callback genuinely \
         ran against real audio flowing through the actual running PipeWire session — not just \
         this process's own bookkeeping — using only atomics and pointer arithmetic (no \
         allocation or locking) inside the realtime callback. That's a real, better-than-Clamp \
         mechanism #86 could build on."
    );
}
