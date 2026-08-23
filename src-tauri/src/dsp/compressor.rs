use crate::core::models::EffectStage;
use crate::dsp::stage::DspStage;

/// Floor for the envelope's dB conversion — real silence (`level == 0.0`)
/// has no finite dB value; clamping to this instead of letting `log10(0.0)`
/// produce `-inf` keeps every downstream computation finite, per `DspStage`'s
/// own contract that `process` must never propagate a non-finite sample.
const SILENCE_FLOOR_DB: f32 = -120.0;

/// A classic feedforward compressor: peak envelope follower (one-pole
/// attack/release smoothing in the dB domain, not the linear domain — the
/// standard choice, since it makes a fixed `ratio_x10` produce a
/// perceptually-linear dB-per-dB gain reduction slope above threshold) into
/// a static gain computer, plus a final makeup-gain trim. No lookahead or
/// soft knee (hard knee only) — the same "even a hard limiter is a real
/// proof point" scope the #509 spike used, now extended with a real
/// attack/release envelope rather than an instantaneous hard clamp.
///
/// This is the portable equivalent of what a `libpipewire-module-filter-chain`
/// `bq_*`/`clamp`/`delay` builtin backs for `Eq5Band`/`Limiter`/`Delay` —
/// PipeWire ships no builtin envelope-following dynamics primitive (verified
/// against `man 7 libpipewire-module-filter-chain`; see issue #86), so this
/// is genuinely hand-written DSP, hosted via `pipewire::native_dsp_host`
/// (issue #74) rather than a builtin filter-chain config string.
pub struct CompressorProcessor {
    threshold_db: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    makeup_gain_linear: f32,
    envelope_db: f32,
}

impl CompressorProcessor {
    /// Built off the audio thread from an `EffectStage::Compressor` config;
    /// panics if given any other stage kind — callers are expected to
    /// dispatch on `EffectStage`'s variant before constructing this, same
    /// convention as `Eq5BandProcessor::from_stage`.
    pub fn from_stage(sample_rate_hz: f64, stage: &EffectStage) -> Self {
        let EffectStage::Compressor {
            threshold_db,
            ratio_x10,
            attack_ms,
            release_ms,
            makeup_gain_db,
            ..
        } = stage
        else {
            panic!("CompressorProcessor::from_stage called with a non-Compressor EffectStage");
        };

        // A ratio below 1:1 would mean "expand", not "compress", and a
        // sub-millisecond attack/release time constant divides by (near)
        // zero below — both clamped to sane minimums rather than trusted
        // from already-preflighted-elsewhere user input, since this
        // constructor has no preflight step of its own to lean on.
        let ratio = (*ratio_x10 as f32 / 10.0).max(1.0);
        let attack_seconds = (*attack_ms as f32 / 1000.0).max(0.001);
        let release_seconds = (*release_ms as f32 / 1000.0).max(0.001);
        let sample_rate_hz = sample_rate_hz as f32;

        Self {
            threshold_db: *threshold_db as f32,
            ratio,
            attack_coeff: (-1.0 / (sample_rate_hz * attack_seconds)).exp(),
            release_coeff: (-1.0 / (sample_rate_hz * release_seconds)).exp(),
            makeup_gain_linear: 10f32.powf(*makeup_gain_db as f32 / 20.0),
            envelope_db: SILENCE_FLOOR_DB,
        }
    }
}

impl DspStage for CompressorProcessor {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let input_db = (20.0 * input.abs().log10()).max(SILENCE_FLOOR_DB);

        // Attack while the signal is louder than the envelope's current
        // estimate (envelope needs to catch up quickly), release while it's
        // quieter (envelope needs to relax slowly) — the standard
        // peak-detector shape, not a fixed single time constant.
        let coeff = if input_db > self.envelope_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope_db = coeff * self.envelope_db + (1.0 - coeff) * input_db;

        let gain_reduction_db = if self.envelope_db > self.threshold_db {
            (self.threshold_db - self.envelope_db) * (1.0 - 1.0 / self.ratio)
        } else {
            0.0
        };

        let gain_linear = 10f32.powf(gain_reduction_db / 20.0) * self.makeup_gain_linear;
        let output = input * gain_linear;
        if output.is_finite() {
            output
        } else {
            0.0
        }
    }

    fn reset(&mut self) {
        self.envelope_db = SILENCE_FLOOR_DB;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(threshold_db: i32, ratio_x10: i32, attack_ms: i32, release_ms: i32) -> EffectStage {
        EffectStage::Compressor {
            id: "compressor".to_string(),
            threshold_db,
            ratio_x10,
            attack_ms,
            release_ms,
            makeup_gain_db: 0,
        }
    }

    /// A signal held well below threshold should pass through essentially
    /// unchanged (no gain reduction), after the envelope has settled.
    #[test]
    fn quiet_signal_below_threshold_is_left_alone() {
        let mut compressor = CompressorProcessor::from_stage(48000.0, &stage(-18, 40, 5, 50));
        let mut last = 0.0;
        for _ in 0..2000 {
            last = compressor.process(0.1);
        }
        assert!((last - 0.1).abs() < 0.001, "expected ~0.1, got {last}");
    }

    /// A signal held well above threshold should settle to a steady-state
    /// gain reduction matching the configured ratio, once the envelope has
    /// converged (attack time constant elapsed many times over).
    #[test]
    fn loud_signal_above_threshold_settles_to_the_configured_ratio() {
        let threshold_db = -18.0;
        let ratio = 4.0;
        let input_level: f32 = 0.5; // well above -18dB (~-6dB)
        let mut compressor = CompressorProcessor::from_stage(48000.0, &stage(-18, 40, 5, 50));
        let mut last = 0.0;
        for _ in 0..48000 {
            last = compressor.process(input_level);
        }

        let input_db = 20.0 * input_level.log10();
        let expected_reduction_db = (threshold_db - input_db) * (1.0 - 1.0 / ratio);
        let expected_output = input_level * 10f32.powf(expected_reduction_db / 20.0);
        assert!(
            (last - expected_output).abs() < 0.01,
            "expected ~{expected_output}, got {last}"
        );
        assert!(
            last.abs() < input_level,
            "compressed output {last} should be quieter than input {input_level}"
        );
    }

    /// Makeup gain should scale the final output beyond whatever the gain
    /// computer alone would produce.
    #[test]
    fn makeup_gain_boosts_the_output() {
        let base_stage = EffectStage::Compressor {
            id: "compressor".to_string(),
            threshold_db: -18,
            ratio_x10: 40,
            attack_ms: 5,
            release_ms: 50,
            makeup_gain_db: 0,
        };
        let boosted_stage = EffectStage::Compressor {
            id: "compressor".to_string(),
            threshold_db: -18,
            ratio_x10: 40,
            attack_ms: 5,
            release_ms: 50,
            makeup_gain_db: 12,
        };

        let mut base = CompressorProcessor::from_stage(48000.0, &base_stage);
        let mut boosted = CompressorProcessor::from_stage(48000.0, &boosted_stage);
        let mut base_out = 0.0;
        let mut boosted_out = 0.0;
        for _ in 0..48000 {
            base_out = base.process(0.5);
            boosted_out = boosted.process(0.5);
        }

        let ratio = boosted_out / base_out;
        assert!(
            (ratio - 10f32.powf(12.0 / 20.0)).abs() < 0.01,
            "expected ~{}x boost, got {ratio}x",
            10f32.powf(12.0 / 20.0)
        );
    }

    /// Every `DspStage` must never propagate a non-finite sample — silence
    /// (input `0.0`, `log10` of which is `-inf` before the floor clamp) is
    /// the classic case that would otherwise break this.
    #[test]
    fn silence_produces_a_finite_output() {
        let mut compressor = CompressorProcessor::from_stage(48000.0, &stage(-18, 40, 5, 50));
        for _ in 0..100 {
            let out = compressor.process(0.0);
            assert!(out.is_finite(), "expected finite output, got {out}");
        }
    }

    #[test]
    fn reset_clears_the_envelope_back_to_silence() {
        let mut compressor = CompressorProcessor::from_stage(48000.0, &stage(-18, 40, 5, 50));
        for _ in 0..1000 {
            compressor.process(0.9);
        }
        compressor.reset();
        // Immediately after reset, a quiet signal should not still be
        // compressed by the (now-cleared) envelope's prior loud history.
        let out = compressor.process(0.01);
        assert!(
            (out - 0.01).abs() < 0.001,
            "expected ~0.01 right after reset, got {out}"
        );
    }
}
