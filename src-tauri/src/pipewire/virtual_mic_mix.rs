use crate::core::models::{Device, DeviceDirection, DeviceKind, MixSourceSpec};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl;
use crate::pipewire::pw_link;
use std::collections::HashSet;

/// Applies a virtual mic's mix sources, each through its own per-pair feed
/// sink so it gets an independent gain (see `pactl::ensure_feed_sink_for_mix_pair`)
/// instead of being summed at unity gain via a direct port link.
pub fn apply_virtual_mic_mix(
    virtual_input: &Device,
    mix_sources: &[MixSourceSpec],
) -> Result<(), AdapterError> {
    if virtual_input.kind != DeviceKind::Virtual || virtual_input.direction == DeviceDirection::Duplex {
        return Err(AdapterError::Message(
            "mix sources can only be attached to a virtual input or virtual output".into(),
        ));
    }

    let own_playback_feed = pactl::feed_sink_name_for_virtual_input(&virtual_input.system_name);
    let mut keep_source_names = HashSet::new();

    for mix_source in mix_sources {
        if mix_source.system_name == own_playback_feed {
            return Err(AdapterError::Message(
                "cannot mix a virtual mic's own playback feed sink into itself".into(),
            ));
        }

        let feed_name = pactl::ensure_feed_sink_for_mix_pair(
            &virtual_input.system_name,
            &mix_source.system_name,
            &virtual_input.label,
        )?;
        pw_link::link_capture_source_to_sink(&mix_source.system_name, &feed_name)?;
        pactl::set_sink_volume_by_name(&feed_name, mix_source.volume_percent)?;
        pactl::set_sink_mute_by_name(&feed_name, mix_source.muted)?;
        pw_link::link_sink_monitor_to_target(&feed_name, &virtual_input.system_name, true)?;

        keep_source_names.insert(mix_source.system_name.clone());
    }

    pactl::gc_feed_sinks_for_mix_pairs(&virtual_input.system_name, &keep_source_names)?;

    Ok(())
}

/// Sets the gain for one already-mixed source, without touching linking —
/// safe to call at high frequency for a live slider drag.
pub fn set_mix_source_volume(
    virtual_input_system_name: &str,
    source_system_name: &str,
    volume_percent: u8,
) -> Result<(), AdapterError> {
    let feed_name = pactl::feed_sink_name_for_mix_pair(virtual_input_system_name, source_system_name);
    pactl::set_sink_volume_by_name(&feed_name, volume_percent)
}

/// Mutes/unmutes one already-mixed source's feed sink directly — no relinking,
/// so the port connections (and this source's place in the mix) are completely
/// untouched. This is the mechanism behind "mute without breaking the link".
pub fn set_mix_source_mute(
    virtual_input_system_name: &str,
    source_system_name: &str,
    muted: bool,
) -> Result<(), AdapterError> {
    let feed_name = pactl::feed_sink_name_for_mix_pair(virtual_input_system_name, source_system_name);
    pactl::set_sink_mute_by_name(&feed_name, muted)
}

pub fn disconnect_all_virtual_mic_mixes(virtual_input_system_name: &str) -> Result<(), AdapterError> {
    pactl::gc_feed_sinks_for_mix_pairs(virtual_input_system_name, &HashSet::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mic(direction: DeviceDirection) -> Device {
        Device {
            id: "mic".into(),
            system_name: "pipe-deck-mic".into(),
            label: "Mic".into(),
            kind: DeviceKind::Virtual,
            direction,
            sink_mode: None,
            volume_percent: None,
            muted: None,
            current_target: None,
            current_targets: Vec::new(),
            mix_sources: Vec::new(),
        }
    }

    #[test]
    fn rejects_own_playback_feed_sink_as_mix_source() {
        let mic = sample_mic(DeviceDirection::Input);
        let sources = vec![MixSourceSpec {
            system_name: "pipe-deck-feed-mic".into(),
            volume_percent: 100,
            muted: false,
        }];
        let error = apply_virtual_mic_mix(&mic, &sources).expect_err("self-loop should be rejected");
        assert!(error.to_string().contains("own playback feed"));
    }

    #[test]
    fn rejects_duplex_target() {
        let mic = sample_mic(DeviceDirection::Duplex);
        let error = apply_virtual_mic_mix(&mic, &[]).expect_err("duplex should be rejected");
        assert!(error.to_string().contains("virtual input or virtual output"));
    }
}
