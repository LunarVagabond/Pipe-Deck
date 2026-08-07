//! Portable, platform-agnostic DSP core (issue #74). No `pipewire`/`backend`
//! dependencies — every type here compiles and runs identically on any
//! target. Real-time hosting (turning this into actual audio I/O) is a
//! separate concern per platform; see `pipewire::native_dsp_host` for the
//! Linux `pw::stream` host.

pub mod biquad;
pub mod chain;
pub mod eq5band;
pub mod stage;

pub use chain::DspChain;
pub use eq5band::Eq5BandProcessor;
pub use stage::DspStage;
