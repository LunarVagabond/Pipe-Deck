use crate::core::models::{EffectChainConfig, Profile, RuntimeGraph};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::filter_chain;
use crate::core::routing::RoutingError;

pub fn apply_device_effect_chain(
    system_name: &str,
    config: &EffectChainConfig,
) -> Result<Option<String>, AdapterError> {
    filter_chain::apply_effect_chain(system_name, config)
}

pub fn apply_profile_effects(
    graph: &RuntimeGraph,
    profile: &Profile,
) -> Result<Vec<String>, RoutingError> {
    let mut warnings = Vec::new();
    for (device_id, config) in &profile.effect_state {
        let system_name = graph
            .devices
            .iter()
            .find(|device| device.id == *device_id)
            .map(|device| device.system_name.clone())
            .or_else(|| profile.device_assumptions.get(device_id).cloned());

        let Some(system_name) = system_name else {
            continue;
        };

        if !filter_chain::is_pipe_deck_device(&system_name) {
            continue;
        }

        if let Some(warning) = apply_device_effect_chain(&system_name, config)? {
            warnings.push(warning);
        }
    }
    Ok(warnings)
}
