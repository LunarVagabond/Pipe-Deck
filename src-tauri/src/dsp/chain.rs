use crate::core::models::{EffectChainConfig, EffectStage};
use crate::dsp::eq5band::Eq5BandProcessor;
use crate::dsp::stage::DspStage;

/// An ordered pipeline of `DspStage`s, itself a `DspStage`. Built from an
/// `EffectChainConfig` off the audio thread; the real-time host swaps the
/// whole chain in atomically (see `pipewire::native_dsp_host`) rather than
/// mutating one in place from two threads, and must hand the *old* chain
/// back out to be dropped off the audio thread — `Box`/`Vec` deallocation
/// is not real-time safe, so a chain must never be dropped inside the audio
/// callback.
pub struct DspChain {
    stages: Vec<Box<dyn DspStage>>,
}

impl DspChain {
    /// Empty chain — pure passthrough.
    pub fn empty() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn from_stages(stages: Vec<Box<dyn DspStage>>) -> Self {
        Self { stages }
    }

    /// Builds a chain from a device's effect config. Only `EffectStage::Eq5Band`
    /// has a real `DspStage` implementation today (issue #74's scope); any
    /// other stage kind is skipped here rather than erroring, since those
    /// kinds are only ever reached through `ProcessingNode`'s
    /// builtin-module transport (`pipewire::native_host`), never through
    /// this portable path — this function only has to handle what
    /// `native_dsp_host` is actually asked to host.
    pub fn from_config(sample_rate_hz: f64, config: &EffectChainConfig) -> Self {
        let stages = config
            .stages
            .iter()
            .filter_map(|stage| match stage {
                EffectStage::Eq5Band { .. } => {
                    let processor = Eq5BandProcessor::from_stage(sample_rate_hz, stage);
                    Some(Box::new(processor) as Box<dyn DspStage>)
                }
                _ => None,
            })
            .collect();
        Self { stages }
    }
}

impl DspStage for DspChain {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut sample = input;
        for stage in &mut self.stages {
            sample = stage.process(sample);
        }
        sample
    }

    fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_is_passthrough() {
        let mut chain = DspChain::empty();
        assert_eq!(chain.process(0.42), 0.42);
        assert_eq!(chain.process(-1.0), -1.0);
    }

    #[test]
    fn from_config_builds_only_eq5band_stages() {
        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 6,
                eq_bass: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        let mut chain = DspChain::from_config(48000.0, &config);
        assert_eq!(chain.stages.len(), 1);
        let out = chain.process(1.0);
        assert!(out.is_finite());
    }

    #[test]
    fn from_config_skips_non_eq5band_stages() {
        let config = EffectChainConfig {
            stages: vec![EffectStage::Hpf { id: "hpf".to_string(), freq_hz: 100, resonance_x10: 10 }],
            ..Default::default()
        };
        let chain = DspChain::from_config(48000.0, &config);
        assert_eq!(chain.stages.len(), 0);
    }

    #[test]
    fn reset_propagates_to_all_stages() {
        let config = EffectChainConfig {
            stages: vec![EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_sub: 10,
                eq_bass: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        let mut chain = DspChain::from_config(48000.0, &config);
        chain.process(1.0);
        chain.reset();
        assert_eq!(chain.process(0.0), 0.0);
    }
}
