//! GUI<->daemon IPC for native-effects hosting (issue #148). See
//! `protocol` for the wire format, `server` for the daemon-side listener
//! (the only caller of `pipewire::native_host` now), and `client` for the
//! GUI-side (`backend::linux::live`) caller.

pub mod client;
pub mod protocol;
pub mod server;
