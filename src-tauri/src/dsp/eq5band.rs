use crate::core::models::EffectStage;
use crate::dsp::biquad::{BiquadCoeffs, BiquadState};
use crate::dsp::stage::DspStage;

/// Band center frequencies/Q, matching `pipewire::fx_validate`'s
/// `render_module_args`' `bq_lowshelf`/`bq_peaking`/`bq_highshelf` template
/// exactly, so this portable implementation sounds the same as the
/// PipeWire-builtin-backed chain it replaces, not just structurally similar
/// to it.
const SUB_FREQ_HZ: f64 = 60.0;
const BASS_FREQ_HZ: f64 = 150.0;
const MID_FREQ_HZ: f64 = 1000.0;
const TREBLE_FREQ_HZ: f64 = 4000.0;
const AIR_FREQ_HZ: f64 = 10000.0;
const BAND_Q: f64 = 1.0;

const NUM_BANDS: usize = 5;

/// Five cascaded biquads (low-shelf, peaking x3, high-shelf) plus an output
/// trim gain — the portable equivalent of the `eq_sub -> eq_bass -> eq_mid
/// -> eq_treble -> eq_air -> out_gain` filter-chain graph
/// `fx_validate::render_conf` currently generates as PipeWire config text.
pub struct Eq5BandProcessor {
    bands: [(BiquadCoeffs, BiquadState); NUM_BANDS],
    output_gain_linear: f64,
}

impl Eq5BandProcessor {
    /// Flat response (all bands 0dB, unity output gain).
    pub fn new() -> Self {
        Self {
            bands: [(BiquadCoeffs::identity(), BiquadState::default()); NUM_BANDS],
            output_gain_linear: 1.0,
        }
    }

    /// Built off the audio thread from an `EffectStage::Eq5Band` config;
    /// panics if given any other stage kind — callers are expected to
    /// dispatch on `EffectStage`'s variant before constructing this.
    pub fn from_stage(sample_rate_hz: f64, stage: &EffectStage) -> Self {
        let EffectStage::Eq5Band {
            eq_sub,
            eq_bass,
            eq_mid,
            eq_treble,
            eq_air,
            output_gain,
            ..
        } = stage
        else {
            panic!("Eq5BandProcessor::from_stage called with a non-Eq5Band EffectStage");
        };

        let bands = [
            (
                BiquadCoeffs::low_shelf(sample_rate_hz, SUB_FREQ_HZ, BAND_Q, f64::from(*eq_sub)),
                BiquadState::default(),
            ),
            (
                BiquadCoeffs::peaking(sample_rate_hz, BASS_FREQ_HZ, BAND_Q, f64::from(*eq_bass)),
                BiquadState::default(),
            ),
            (
                BiquadCoeffs::peaking(sample_rate_hz, MID_FREQ_HZ, BAND_Q, f64::from(*eq_mid)),
                BiquadState::default(),
            ),
            (
                BiquadCoeffs::peaking(
                    sample_rate_hz,
                    TREBLE_FREQ_HZ,
                    BAND_Q,
                    f64::from(*eq_treble),
                ),
                BiquadState::default(),
            ),
            (
                BiquadCoeffs::high_shelf(sample_rate_hz, AIR_FREQ_HZ, BAND_Q, f64::from(*eq_air)),
                BiquadState::default(),
            ),
        ];

        Self {
            bands,
            output_gain_linear: 10f64.powf(f64::from(*output_gain) / 20.0),
        }
    }
}

impl Default for Eq5BandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl DspStage for Eq5BandProcessor {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut sample = f64::from(input);
        for (coeffs, state) in &mut self.bands {
            sample = state.process(coeffs, sample);
        }
        sample *= self.output_gain_linear;
        sample as f32
    }

    fn reset(&mut self) {
        for (_, state) in &mut self.bands {
            state.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_stage() -> EffectStage {
        EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_sub: 0,
            eq_bass: 0,
            eq_mid: 0,
            eq_treble: 0,
            eq_air: 0,
            output_gain: 0,
        }
    }

    #[test]
    fn flat_config_is_near_unity_passthrough() {
        let mut eq = Eq5BandProcessor::from_stage(48000.0, &flat_stage());
        // Settle transient, then confirm a steady-state impulse passes
        // through close to unchanged.
        for _ in 0..1000 {
            eq.process(0.0);
        }
        let out = eq.process(1.0);
        assert!(
            (out - 1.0).abs() < 1e-3,
            "flat EQ should pass an impulse near-unchanged, got {out}"
        );
    }

    #[test]
    fn extreme_boost_stays_finite_and_bounded() {
        let stage = EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_sub: 12,
            eq_bass: 12,
            eq_mid: 12,
            eq_treble: 12,
            eq_air: 12,
            output_gain: 12,
        };
        let mut eq = Eq5BandProcessor::from_stage(48000.0, &stage);
        let mut max_abs = 0.0f32;
        for i in 0..48000 {
            // A loud, non-trivial test signal (not silence) so every band
            // actually processes something.
            let input = if i % 2 == 0 { 0.8 } else { -0.8 };
            let out = eq.process(input);
            assert!(out.is_finite());
            max_abs = max_abs.max(out.abs());
        }
        assert!(
            max_abs < 20.0,
            "5-band max boost should not run away, peaked at {max_abs}"
        );
    }

    #[test]
    fn silence_stays_silent_after_settling() {
        let stage = EffectStage::Eq5Band {
            id: "eq".to_string(),
            eq_sub: 8,
            eq_bass: -8,
            eq_mid: 4,
            eq_treble: -4,
            eq_air: 6,
            output_gain: 0,
        };
        let mut eq = Eq5BandProcessor::from_stage(48000.0, &stage);
        // Prime with a transient, then feed silence — should decay to
        // exact zero (denormal flush), not leak forever.
        eq.process(1.0);
        for _ in 0..500_000 {
            eq.process(0.0);
        }
        assert_eq!(eq.process(0.0), 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut eq = Eq5BandProcessor::from_stage(48000.0, &flat_stage());
        eq.process(1.0);
        eq.process(1.0);
        eq.reset();
        // Right after reset, a zero input should produce exactly zero
        // (no leftover filter memory).
        assert_eq!(eq.process(0.0), 0.0);
    }

    #[test]
    #[should_panic(expected = "non-Eq5Band")]
    fn from_stage_panics_on_wrong_variant() {
        let stage = EffectStage::Hpf {
            id: "hpf".to_string(),
            freq_hz: 100,
            resonance_x10: 10,
        };
        let _ = Eq5BandProcessor::from_stage(48000.0, &stage);
    }
}
