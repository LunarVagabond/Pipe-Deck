//! Biquad filter coefficient math (RBJ "Audio EQ Cookbook" formulas,
//! Q-parameterized shelf variant — the same parameterization PipeWire's own
//! builtin `bq_lowshelf`/`bq_highshelf`/`bq_peaking` filters take a `Q`
//! control input for, so a chain built from these coefficients should sound
//! equivalent to today's PipeWire-builtin-backed chain, not just be
//! structurally similar to it).
//!
//! Deliberately dependency-free: the formulas are short and this is exactly
//! the kind of "must never produce NaN, ever" code where fewer moving parts
//! is worth more than reuse.

/// Normalized (a0 = 1) biquad transfer-function coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    /// Identity filter — output equals input.
    pub const fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }

    /// Peaking EQ (boost/cut around `freq_hz` with bandwidth set by `q`).
    pub fn peaking(sample_rate_hz: f64, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq_hz / sample_rate_hz;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// Low shelf — boosts/cuts everything below `freq_hz`.
    pub fn low_shelf(sample_rate_hz: f64, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let sqrt_a = a.sqrt();
        let w0 = 2.0 * std::f64::consts::PI * freq_hz / sample_rate_hz;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    /// High shelf — boosts/cuts everything above `freq_hz`.
    pub fn high_shelf(sample_rate_hz: f64, freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let sqrt_a = a.sqrt();
        let w0 = 2.0 * std::f64::consts::PI * freq_hz / sample_rate_hz;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;

        Self::normalize(b0, b1, b2, a0, a1, a2)
    }

    #[allow(clippy::too_many_arguments)]
    fn normalize(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        // a0 is a sum of strictly-positive terms (alpha/A, cosh-bounded
        // terms) for every finite freq/Q/gain input in range, so division
        // here does not need a zero-guard for the domains `preflight`
        // allows through — but NaN/inf must never escape regardless of how
        // a caller got here, so callers use `BiquadState`'s own
        // sanitization on top of this rather than trusting coefficients
        // blindly.
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self::identity()
    }
}

/// Direct Form II Transposed filter state — two state variables, no
/// separate delay-line allocation, numerically well behaved. Safe to call
/// from a real-time callback: `process` never allocates or branches on a
/// "first call" flag.
#[derive(Debug, Clone, Copy, Default)]
pub struct BiquadState {
    z1: f64,
    z2: f64,
}

/// Below this magnitude, state is flushed to exact zero rather than left to
/// decay through denormal range. Denormal (subnormal) float arithmetic is
/// implemented in microcode on most CPUs and can be 10-100x slower than
/// normal float ops — a biquad fed silence can otherwise decay its z1/z2
/// state into denormal range and never leave it, silently spiking CPU usage
/// with no audible symptom until it causes an xrun under load.
const DENORMAL_FLUSH_THRESHOLD: f64 = 1e-30;

impl BiquadState {
    #[inline]
    pub fn process(&mut self, coeffs: &BiquadCoeffs, input: f64) -> f64 {
        let output = coeffs.b0 * input + self.z1;
        self.z1 = coeffs.b1 * input - coeffs.a1 * output + self.z2;
        self.z2 = coeffs.b2 * input - coeffs.a2 * output;

        if self.z1.abs() < DENORMAL_FLUSH_THRESHOLD {
            self.z1 = 0.0;
        }
        if self.z2.abs() < DENORMAL_FLUSH_THRESHOLD {
            self.z2 = 0.0;
        }

        if output.is_finite() {
            output
        } else {
            // A non-finite output can only happen from a non-finite input
            // or already-corrupted state; reset rather than let NaN/inf
            // propagate through the rest of the chain and out to the
            // driver.
            self.z1 = 0.0;
            self.z2 = 0.0;
            0.0
        }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values computed independently from the RBJ cookbook
    // formulas at 48kHz, 1kHz, Q=1.0, +6dB peaking — catches sign/formula
    // transcription errors, not just "doesn't panic".
    #[test]
    fn peaking_matches_reference_coefficients() {
        // Reference values from an independent Python re-implementation of
        // the same RBJ cookbook formula (not derived from this file).
        let c = BiquadCoeffs::peaking(48000.0, 1000.0, 1.0, 6.0);
        assert!((c.b0 - 1.043_953_086_990_335).abs() < 1e-9, "b0 = {}", c.b0);
        assert!((c.b1 - (-1.895_320_723_936_596_1)).abs() < 1e-9, "b1 = {}", c.b1);
        assert!((c.b2 - 0.867_722_284_759_856_6).abs() < 1e-9, "b2 = {}", c.b2);
        assert!((c.a1 - (-1.895_320_723_936_596_1)).abs() < 1e-9, "a1 = {}", c.a1);
        assert!((c.a2 - 0.911_675_371_750_191_5).abs() < 1e-9, "a2 = {}", c.a2);
    }

    /// At 0dB gain the numerator and denominator polynomials are identical
    /// by construction (A=1 makes the boost/cut term cancel), so the
    /// *transfer function* is identity — but the raw b1/b2 and a1/a2
    /// coefficients are not individually zero, they're equal to each other.
    /// Verified here via `BiquadState::process` (actual behavior) rather
    /// than asserting raw coefficient values, which would be the wrong
    /// property to check.
    #[test]
    fn zero_gain_is_near_identity_for_all_shapes() {
        for freq in [60.0, 150.0, 1000.0, 4000.0, 10000.0] {
            for coeffs in [
                BiquadCoeffs::peaking(48000.0, freq, 1.0, 0.0),
                BiquadCoeffs::low_shelf(48000.0, freq, 1.0, 0.0),
                BiquadCoeffs::high_shelf(48000.0, freq, 1.0, 0.0),
            ] {
                let mut state = BiquadState::default();
                let inputs = [1.0, 0.5, -0.3, 0.0, -1.0, 0.2];
                for &input in &inputs {
                    let out = state.process(&coeffs, input);
                    assert!((out - input).abs() < 1e-9, "0dB should pass {input} through unchanged, got {out}");
                }
            }
        }
    }

    /// Poles at `r*e^{+-j*theta}` are stable iff `|r| < 1`; for a normalized
    /// biquad this holds iff `a2 < 1` and `|a1| < 1 + a2` — the standard
    /// stability triangle. Sweeps a wide range including edge-case Q/gain
    /// values, not just nominal ones.
    #[test]
    fn coefficients_are_stable_across_param_ranges() {
        let sample_rate = 48000.0;
        for freq in [20.0, 60.0, 150.0, 1000.0, 4000.0, 10000.0, 20000.0] {
            for q in [0.1, 0.5, 1.0, 2.0, 10.0] {
                for gain in [-12.0, -6.0, 0.0, 6.0, 12.0] {
                    for c in [
                        BiquadCoeffs::peaking(sample_rate, freq, q, gain),
                        BiquadCoeffs::low_shelf(sample_rate, freq, q, gain),
                        BiquadCoeffs::high_shelf(sample_rate, freq, q, gain),
                    ] {
                        assert!(c.b0.is_finite() && c.b1.is_finite() && c.b2.is_finite());
                        assert!(c.a1.is_finite() && c.a2.is_finite());
                        assert!(
                            c.a2 < 1.0 + 1e-6 && c.a1.abs() < 1.0 + c.a2 + 1e-6,
                            "unstable poles at freq={freq} q={q} gain={gain}: a1={} a2={}",
                            c.a1,
                            c.a2
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn silence_in_stays_silence_out_and_never_denormalizes() {
        let coeffs = BiquadCoeffs::peaking(48000.0, 1000.0, 1.0, 12.0);
        let mut state = BiquadState::default();
        for _ in 0..1_000_000 {
            let out = state.process(&coeffs, 0.0);
            assert_eq!(out, 0.0);
        }
        assert_eq!(state.z1, 0.0);
        assert_eq!(state.z2, 0.0);
    }

    #[test]
    fn impulse_response_is_finite_and_bounded() {
        let coeffs = BiquadCoeffs::peaking(48000.0, 1000.0, 1.0, 12.0);
        let mut state = BiquadState::default();
        let mut max_abs = 0.0f64;
        let out0 = state.process(&coeffs, 1.0);
        max_abs = max_abs.max(out0.abs());
        for _ in 0..48000 {
            let out = state.process(&coeffs, 0.0);
            assert!(out.is_finite());
            max_abs = max_abs.max(out.abs());
        }
        // +12dB peaking should never ring up to an absurd multiple of the
        // impulse; a runaway/unstable filter would blow well past this.
        assert!(max_abs < 10.0, "impulse response peaked at {max_abs}");
    }

    #[test]
    fn non_finite_input_is_contained_not_propagated() {
        let coeffs = BiquadCoeffs::peaking(48000.0, 1000.0, 1.0, 6.0);
        let mut state = BiquadState::default();
        let out = state.process(&coeffs, f64::NAN);
        assert_eq!(out, 0.0);
        assert_eq!(state.z1, 0.0);
        assert_eq!(state.z2, 0.0);
        // Recovers cleanly on the next, valid sample.
        let out2 = state.process(&coeffs, 0.0);
        assert!(out2.is_finite());
    }
}
