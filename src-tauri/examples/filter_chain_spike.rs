//! Research spike for issue #141: does repeatedly loading/unloading
//! `module-filter-chain` in-process via `pipewire-rs`, instead of the
//! current conf.d + `systemctl restart filter-chain.service` flow
//! (`pipewire::pipewire_restart`), actually work against a real PipeWire
//! session — without leaking, destabilizing the session, or needing an
//! extra export step?
//!
//! Throwaway prototype only, not wired into the app. Run by hand against a
//! real PipeWire session (needs `libpipewire-0.3` dev headers + a running
//! `pipewire.service`):
//!
//!     cargo run --example filter_chain_spike --features spike
//!
//! See docs/Decisions.md PD-017/PD-025/PD-026 and issue #141 for context.
//! PD-017 already ruled out two easier paths: `pactl load-module
//! module-filter-chain` (not exposed via the Pulse compat layer) and
//! `pw-cli load-module` (only loads into pw-cli's own throwaway local
//! context, never the running `pipewire-0` session). This spike checks
//! whether calling the same underlying C API
//! (`pw_context_load_module`/`pw_impl_module_destroy`, unwrapped by
//! `pipewire-rs`) from a *long-running* process fares any differently, or
//! hits the same "local context only" wall.
//!
//! ## Findings from an actual run against a live session (2026-07-16)
//!
//! - **It works**, and differently from PD-017's pw-cli finding: pumping the
//!   main loop for ~1s after `pw_context_load_module` (instead of exiting
//!   immediately, which is what a `pw-cli` invocation does) lets the async
//!   node/port setup finish, and the resulting node is a real node in the
//!   *actual* running `pipewire.service` graph — confirmed via `pactl list
//!   short sinks`/`sources`, `pw-link -o`/`-i`, and `pw-cli ls Node`, not
//!   just our own local bookkeeping. 5/5 load cycles succeeded, 0/5 leaked
//!   past `pw_impl_module_destroy`.
//! - **The playback side does not auto-link anywhere** — its `output_FL`/
//!   `output_FR` ports exist but have no links until something (this app's
//!   existing `pw_link` code) explicitly connects them onward. This matches
//!   today's `effect_output.*` convention exactly, so the existing "link the
//!   processed output onward" logic could be reused as-is.
//! - **Ordering gotcha**: calling `pw::deinit()` while `ContextRc`/
//!   `MainLoopRc` are still in scope segfaults on shutdown (their `Drop`
//!   impls call `pw_context_destroy`/`pw_main_loop_destroy` into an
//!   already-deinitialized library). Fixed here by scoping them into `run()`
//!   and calling `deinit()` only after that returns — but this is exactly
//!   the kind of lifecycle hazard a long-running GUI process embedding this
//!   would need to get right once, centrally, not per call site.
//! - **Not yet checked**: RSS grew ~560kB over 5 cycles in one run — small,
//!   but not proven flat. A longer soak (50-100+ cycles) is needed before
//!   trusting "no leak" as a real conclusion rather than one-time warmup;
//!   left as follow-up rather than inflating this spike's scope.

use pipewire as pw;
use pipewire::sys as pw_sys;
use std::ffi::CString;
use std::process::Command;
use std::time::Duration;

const CYCLES: u32 = 5;

fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|kb| kb.parse().ok())
    })
}

/// Snapshot of sink node names visible to `pactl` in the *real* running
/// session — this is the thing that actually matters (does Pipe Deck's
/// virtual-device machinery see the node), independent of whatever our own
/// local `pw_context` thinks is loaded.
fn live_session_sink_names() -> Vec<String> {
    let Ok(output) = Command::new("pactl").args(["list", "short", "sinks"]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split('\t').nth(1).map(str::to_string))
        .collect()
}

fn main() {
    pw::init();
    run();
    // `context`/`mainloop` must be fully dropped (their `Drop` impls call
    // `pw_context_destroy`/`pw_main_loop_destroy`) before `deinit()` tears
    // down the library — calling `deinit()` while they're still in scope
    // segfaults on exit, discovered the hard way while writing this spike.
    unsafe { pw::deinit() };
}

fn run() {
    let node_name = "pipe-deck-spike-filter-chain";
    let module_args = format!(
        r#"{{
            node.description = "Pipe Deck Filter Chain Spike"
            media.name       = "Pipe Deck Filter Chain Spike"
            filter.graph = {{
                nodes = [
                    {{ type = builtin name = passthrough label = copy }}
                ]
            }}
            audio.channels = 2
            audio.position = [ FL FR ]
            capture.props = {{
                node.name   = "{node_name}.in"
                media.class = Audio/Sink
            }}
            playback.props = {{
                node.name    = "{node_name}"
                node.passive = true
            }}
        }}"#
    );
    let module_args_c = CString::new(module_args).expect("module args must not contain NUL");
    let module_name_c = CString::new("libpipewire-module-filter-chain").unwrap();

    let mainloop = pw::main_loop::MainLoopRc::new(None).expect("failed to create main loop");
    let context = pw::context::ContextRc::new(&mainloop, None).expect("failed to create context");

    println!("=== Pipe Deck filter-chain spike (issue #141) ===");
    println!("Baseline live session sinks: {:?}", live_session_sink_names());
    let rss_before = rss_kb();
    println!("Baseline RSS: {rss_before:?} kB");

    let mut load_failures = 0u32;
    let mut node_appeared_in_live_session = 0u32;
    let mut node_stayed_after_unload = 0u32;

    for cycle in 1..=CYCLES {
        println!("\n--- cycle {cycle}/{CYCLES} ---");

        let module_ptr = unsafe {
            pw_sys::pw_context_load_module(
                context.as_raw_ptr(),
                module_name_c.as_ptr(),
                module_args_c.as_ptr(),
                std::ptr::null_mut(),
            )
        };

        if module_ptr.is_null() {
            println!("pw_context_load_module returned NULL — module failed to load");
            load_failures += 1;
            continue;
        }
        println!("pw_context_load_module succeeded, module ptr = {module_ptr:?}");

        // Pump the loop briefly so the module's async init (node creation,
        // port setup) actually runs before we go check for it.
        let loop_ = mainloop.loop_();
        for _ in 0..20 {
            loop_.iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
        }

        let sinks_after_load = live_session_sink_names();
        let capture_name = format!("{node_name}.in");
        let appeared = sinks_after_load.iter().any(|name| name == &capture_name);
        println!("Live session sinks after load: {sinks_after_load:?}");
        println!("Capture-side node ({capture_name}) visible in the real running session? {appeared}");
        if appeared {
            node_appeared_in_live_session += 1;
        }

        unsafe { pw_sys::pw_impl_module_destroy(module_ptr) };
        for _ in 0..20 {
            loop_.iterate(pw::loop_::Timeout::Finite(Duration::from_millis(50)));
        }

        let sinks_after_unload = live_session_sink_names();
        let still_present = sinks_after_unload.iter().any(|name| name == &capture_name);
        println!("Live session sinks after unload: {sinks_after_unload:?}");
        println!("Node still present after destroy (leak)? {still_present}");
        if still_present {
            node_stayed_after_unload += 1;
        }
    }

    let rss_after = rss_kb();
    println!("\n=== Summary ===");
    println!("Cycles run: {CYCLES}");
    println!("Load failures: {load_failures}/{CYCLES}");
    println!("Node appeared in the live (real) session: {node_appeared_in_live_session}/{CYCLES}");
    println!("Node leaked past unload: {node_stayed_after_unload}/{CYCLES}");
    println!("RSS before: {rss_before:?} kB, RSS after: {rss_after:?} kB");
    println!(
        "\nInterpretation: if 'appeared in the live session' is {CYCLES}/{CYCLES} with 0 leaks, \
         that means pw_context_load_module from a long-running process CAN attach real nodes into \
         the actual running pipewire.service graph — a materially different (better) result than \
         PD-017's pw-cli finding, which likely failed only because pw-cli exits immediately after \
         issuing the load, before the module's async node/port setup completes. If it's 0/{CYCLES}, \
         this matches PD-017 exactly: local-context-only, and #141 needs a different design (the \
         process itself acting as a PipeWire session/server, not just linking libpipewire)."
    );
}
