use crate::backend::BackendError;
use crate::backend::linux::pactl;
use crate::backend::linux::pw_link;
use crate::core::models::effect_output_name_for_device;

/// A virtual output currently hosting live effects (PD-020) has its
/// identity pinned to the *capture* side — `system_name`'s own "monitor"
/// port only ever carries the raw, pre-processing signal now, since the
/// processed audio leaves via the separately-named `effect_output.*` node
/// instead (see `core::models`/`core::engine::effects_ops`).
/// Every caller that fans this device's audio out to a target must resolve
/// through this first — linking straight to `system_name`'s monitor while
/// effects are live bypasses the effect chain entirely (the target hears
/// unprocessed audio), and on a source that's ALSO already correctly linked
/// via `effect_output.*`, doing so on top of that means the target hears
/// both the raw and the processed signal mixed together. Checked against
/// live port state (not persisted config) because that's the only source of
/// truth for whether the swap has actually happened yet.
pub fn effective_fan_out_source(system_name: &str) -> String {
    let effect_output_name = effect_output_name_for_device(system_name);
    if pw_link::has_output_ports(&effect_output_name) {
        effect_output_name
    } else {
        system_name.to_string()
    }
}

pub fn apply_stream_to_sink(
    graph: &crate::core::models::RuntimeGraph,
    stream_id: &str,
    target_device_id: &str,
) -> Result<(), BackendError> {
    pactl::move_stream_to_target(graph, stream_id, target_device_id)
}

pub fn prune_stale_fan_out_links(
    sink_system_name: &str,
    allowed_targets: &std::collections::HashSet<String>,
) -> Result<(), BackendError> {
    let routes = pw_link::list_all_monitor_routes_for_source(sink_system_name);
    for target_name in routes {
        if !allowed_targets.contains(&target_name) {
            pw_link::disconnect_sink_monitor_route(sink_system_name, &target_name)?;
        }
    }
    Ok(())
}
