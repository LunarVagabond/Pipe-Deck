use crate::core::models::{Device, DeviceDirection, DeviceKind};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pw_link;
use std::collections::HashSet;

pub fn apply_virtual_mic_mix(
    virtual_input: &Device,
    mix_source_system_names: &[String],
) -> Result<(), AdapterError> {
    if virtual_input.kind != DeviceKind::Virtual || virtual_input.direction == DeviceDirection::Duplex {
        return Err(AdapterError::Message(
            "mix sources can only be attached to a virtual input or virtual output".into(),
        ));
    }

    let desired: HashSet<String> = mix_source_system_names.iter().cloned().collect();
    let existing: HashSet<String> =
        pw_link::list_capture_sources_for_virtual_input(&virtual_input.system_name)
            .into_iter()
            .collect();

    for source_name in existing.difference(&desired) {
        pw_link::disconnect_capture_source_from_virtual_input(
            source_name,
            &virtual_input.system_name,
        )?;
    }

    for source_name in mix_source_system_names {
        if source_name.starts_with("pipe-deck-feed-") {
            return Err(AdapterError::Message(
                "cannot mix internal feed sinks into a virtual microphone".into(),
            ));
        }
        pw_link::link_capture_source_to_virtual_input(source_name, &virtual_input.system_name)?;
    }

    Ok(())
}

pub fn disconnect_all_virtual_mic_mixes(virtual_input_system_name: &str) -> Result<(), AdapterError> {
    for source_name in pw_link::list_capture_sources_for_virtual_input(virtual_input_system_name) {
        pw_link::disconnect_capture_source_from_virtual_input(
            &source_name,
            virtual_input_system_name,
        )?;
    }
    Ok(())
}
