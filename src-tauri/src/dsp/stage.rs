/// One stage in a `DspChain`. Every implementor must be safe to call from a
/// real-time audio callback: `process` must never allocate, lock, block, or
/// panic, for any input value (including NaN/infinite — a stage must clamp
/// or otherwise contain a bad upstream sample rather than propagate it as
/// worse than it arrived, since a NaN surviving into the audio driver is
/// audible as a glitch or worse).
///
/// Construction and parameter recomputation (anything that allocates, e.g.
/// building a new set of `BiquadCoeffs` from user-facing dB/Hz params) must
/// happen off the audio thread; only `process`/`reset` run on it.
pub trait DspStage: Send {
    /// Processes one sample and returns the result. Real-time safe.
    fn process(&mut self, input: f32) -> f32;

    /// Clears any internal state (filter memory, delay lines) back to
    /// silence, e.g. after a bypass toggle or a discontinuous parameter
    /// jump, to avoid a stale-state click.
    fn reset(&mut self);
}
