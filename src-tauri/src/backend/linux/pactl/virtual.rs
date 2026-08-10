use crate::backend::linux::pactl::run_pactl;
use crate::backend::linux::pw_link;
use crate::backend::linux::pw_virtual_device_native as native;
use crate::backend::BackendError;
use crate::config::store::ConfigStore;
use crate::core::models::DeviceDirection;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Renames the live PipeWire/PulseAudio node backing a primary virtual
/// device (not a feed sink). Neither `pactl` (this stack's PipeWire-Pulse
/// compat shim has no `update-sink-proplist`/`update-source-proplist`) nor
/// `pw-cli`/`pw-metadata` can mutate a node's description in place, so — same
/// as `sync_feed_sink_description` already does for feed sinks — the only way
/// to change it is to unload the module and recreate it with the same
/// `system_name` and the new description. Skips (returns `Ok(None)`) when the
/// description is already current or the device is actively carrying audio,
/// so a rename never disrupts a live stream.
pub fn sync_virtual_device_description(
    system_name: &str,
    direction: DeviceDirection,
    module_id: &str,
    description: &str,
) -> Result<Option<String>, BackendError> {
    if sink_description(system_name)?.as_deref() == Some(description) {
        return Ok(None);
    }

    if virtual_device_in_use(system_name)? {
        return Ok(None);
    }

    unload_module(module_id)?;
    let new_module_id = match direction {
        DeviceDirection::Input => create_virtual_source(system_name, description)?,
        DeviceDirection::Output | DeviceDirection::Duplex => {
            create_null_sink(system_name, description)?
        }
    };
    Ok(Some(new_module_id))
}

/// True if any live PipeWire link currently feeds `system_name`'s
/// `playback_*` ports — a device or a stream, either way, since #428
/// confirmed a stream is just another node whose own output ports can be
/// inspected the same as a device's. Replaces the old `pactl list
/// sink-inputs`/`list sinks short` pair (two full-session shell-outs) with
/// `pw_link::list_capture_sources_for_sink`, which already has its own
/// native-first/CLI-fallback dispatch (#412/#428) — this function just asks
/// "is that list non-empty" instead of re-deriving the same answer through
/// Pulse-compat sink-input indices.
fn sink_has_incoming_connection(system_name: &str) -> bool {
    !pw_link::list_capture_sources_for_sink(system_name).is_empty()
}

pub fn virtual_device_in_use(system_name: &str) -> Result<bool, BackendError> {
    Ok(sink_has_incoming_connection(system_name))
}

/// A shared scratch sink used to briefly hold an in-use device's playback
/// streams while its underlying module is swapped out (e.g. for a
/// Structural Apply), so the swap doesn't just silently fail whatever those
/// streams were doing — see `core::engine::effects_ops::apply_effect_chain_structural`.
pub const HOLDING_SINK_NAME: &str = "pipe-deck-hold";

pub fn ensure_holding_sink() -> Result<(), BackendError> {
    if sink_exists(HOLDING_SINK_NAME)? {
        return Ok(());
    }
    create_null_sink(HOLDING_SINK_NAME, "Pipe Deck (temporary hold)").map(|_| ())
}

/// Tears the scratch hold sink back down once every stream that was parked on
/// it has been moved back to its real device — it's only ever meant to exist
/// for the duration of a single swap, not persist across the session. Safe to
/// call even if the sink is still carrying streams or doesn't exist: skips
/// removal rather than risk stranding audio, and no-ops if already gone.
pub fn remove_holding_sink() -> Result<(), BackendError> {
    if !sink_exists(HOLDING_SINK_NAME)? {
        return Ok(());
    }
    if sink_has_incoming_connection(HOLDING_SINK_NAME) {
        return Ok(());
    }
    remove_sink_by_name(HOLDING_SINK_NAME)
}

/// Removes a sink by name, trying [`native::remove`] first — a direct
/// name-based lookup against the live registry, unlike
/// `find_module_id_by_sink_name` + `unload_module`, which can never find a
/// natively-created sink at all (no Pulse "module" entry backs it). Falls
/// back to the module-scan only when the native connection never started or
/// the node genuinely isn't indexed (the sink really was `pactl`-created).
/// See PD-049 — every feed-sink/holding-sink removal call site had this same
/// bug once `create_null_sink` started creating natively by default (#422):
/// the removal half never got the matching update, so a natively-created
/// feed sink or holding sink was never actually torn down, just silently
/// leaked.
fn remove_sink_by_name(name: &str) -> Result<(), BackendError> {
    if let Some(result) = native::remove(name) {
        return result;
    }
    if let Some(module_id) = find_module_id_by_sink_name(name)? {
        unload_module(&module_id)?;
    }
    Ok(())
}

pub fn feed_sink_description(virtual_mic_label: &str) -> String {
    format!("{virtual_mic_label} (Pipe Deck route)")
}

pub fn sync_feed_sink_for_virtual_input(
    virtual_input_system_name: &str,
    label: &str,
) -> Result<(), BackendError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    if !sink_exists(&feed_name)? {
        return Ok(());
    }

    sync_feed_sink_description(
        &feed_name,
        virtual_input_system_name,
        &feed_sink_description(label),
    )
}

pub fn feed_sink_name_for_virtual_input(virtual_input_system_name: &str) -> String {
    let slug = virtual_input_system_name
        .strip_prefix("pipe-deck-")
        .unwrap_or(virtual_input_system_name);
    format!("pipe-deck-feed-{slug}")
}

pub fn remove_feed_sink_for_virtual_input(
    virtual_input_system_name: &str,
) -> Result<(), BackendError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    let _ = pw_link::disconnect_sink_monitor(&feed_name);
    remove_sink_by_name(&feed_name)
}

pub fn gc_feed_sinks(
    known_virtual_inputs: &std::collections::HashSet<String>,
) -> Result<(), BackendError> {
    let known_slugs: std::collections::HashSet<&str> = known_virtual_inputs
        .iter()
        .filter_map(|name| name.strip_prefix("pipe-deck-"))
        .collect();

    for (module_id, feed_name) in list_sink_names_for_prefix("pipe-deck-feed-")? {
        let Some(rest) = feed_name.strip_prefix("pipe-deck-feed-") else {
            continue;
        };

        // Per-pair mix-source feed sinks (`pipe-deck-feed-{mic}-{source}`,
        // one per contributor to a mic's mix) are owned by
        // `gc_feed_sinks_for_mix_pairs` instead, which understands their
        // real in-use signal (a live pw-link connection, not a pactl
        // sink-input). This function's `in_use` check below can't see that,
        // so without this guard it would tear a mix source's feed sink down
        // on every graph refresh regardless of whether it was just created.
        if is_per_pair_mix_feed_sink(rest, &known_slugs) {
            continue;
        }

        let virtual_input = format!("pipe-deck-{rest}");
        let virtual_exists = known_virtual_inputs.contains(&virtual_input);
        let in_use = sink_has_incoming_connection(&feed_name);

        if virtual_exists && in_use {
            continue;
        }

        let _ = pw_link::disconnect_sink_monitor(&feed_name);
        unload_module(&module_id)?;
    }

    Ok(())
}

/// `(module_id, system_name)` pairs for every live sink whose name starts
/// with `prefix` — the shared discovery step `gc_feed_sinks`/
/// `gc_feed_sinks_for_mix_pairs` both need. Prefers a native node-scan
/// (via [`native::list_nodes`], same rationale as `list_pipe_deck_modules`)
/// over the legacy `pactl list modules short` scan, which — like every
/// other module-scan in this file — can never see a natively-created feed
/// sink at all, meaning GC previously never collected one regardless of how
/// long it sat unused.
fn list_sink_names_for_prefix(prefix: &str) -> Result<Vec<(String, String)>, BackendError> {
    if let Some(nodes) = native::list_nodes() {
        return Ok(nodes
            .into_iter()
            .filter(|node| node.system_name.starts_with(prefix))
            .map(|node| {
                (
                    format!("{NATIVE_MODULE_ID_PREFIX}{}", node.system_name),
                    node.system_name,
                )
            })
            .collect());
    }
    list_modules_for_sink_prefix(prefix)
}

fn is_per_pair_mix_feed_sink(
    feed_sink_rest: &str,
    known_slugs: &std::collections::HashSet<&str>,
) -> bool {
    known_slugs
        .iter()
        .any(|slug| feed_sink_rest.starts_with(&format!("{slug}-")))
}

/// True if a `pipe-deck-*` virtual device with this `system_name` is
/// currently live, whether or not it's currently backed by a module in the
/// *main* session's module table. `list_pipe_deck_modules`/module-based
/// presence checks (used by `core::restore` and
/// `VirtualDeviceRegistry::discover_from_pactl`) can never see a device
/// currently hosting live effects — its `module-filter-chain` module is
/// loaded into the separate `filter-chain.service` PipeWire instance
/// (PD-017/PD-020), never into the module table `pactl list modules`
/// inspects — even though its sink/source is genuinely live and visible.
/// Left unchecked, every caller that used only a module-scan presence check
/// concluded such a device didn't exist and created a *second*, plain
/// null-sink with the same `system_name` right alongside it — two real
/// PipeWire nodes sharing one name, which makes every name-prefix-based
/// port lookup (`pw_link.rs`) ambiguous between them.
pub fn pipe_deck_device_is_live(system_name: &str, direction: DeviceDirection) -> bool {
    match direction {
        DeviceDirection::Input => source_exists(system_name).unwrap_or(false),
        _ => sink_exists(system_name).unwrap_or(false),
    }
}

pub fn sink_exists(name: &str) -> Result<bool, BackendError> {
    if let Some(exists) = native::sink_exists(name) {
        return Ok(exists);
    }
    let output = run_pactl(&["list", "sinks", "short"])?;
    Ok(output
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(name)))
}

/// The source-direction counterpart to `sink_exists` — used to confirm a
/// virtual input device (backed by `module-null-sink` with
/// `media.class=Audio/Source/Virtual`, see `create_virtual_source`) has
/// (re)appeared after a Structural Apply swap (PD-024).
pub fn source_exists(name: &str) -> Result<bool, BackendError> {
    if let Some(exists) = native::source_exists(name) {
        return Ok(exists);
    }
    let output = run_pactl(&["list", "sources", "short"])?;
    Ok(output
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(name)))
}

/// Prefix marking a `module_id` as backed by a natively-created node (#422)
/// rather than a real `pactl` module index — `unload_module` dispatches on
/// it to pick the right teardown path.
const NATIVE_MODULE_ID_PREFIX: &str = "native:";

pub fn create_null_sink(name: &str, description: &str) -> Result<String, BackendError> {
    if let Some(result) = native::create_output(name, description) {
        result?;
        return Ok(format!("{NATIVE_MODULE_ID_PREFIX}{name}"));
    }

    let props = description_module_args(description);
    let output = run_pactl(&[
        "load-module",
        "module-null-sink",
        &format!("sink_name={name}"),
        &props[0],
        &props[1],
        &props[2],
    ])?;
    Ok(output.trim().to_string())
}

/// PipeWire does not provide `module-null-source`. Create a virtual capture
/// endpoint using a null sink configured as an Audio/Source node.
pub fn create_virtual_source(name: &str, description: &str) -> Result<String, BackendError> {
    if let Some(result) = native::create_input(name, description) {
        result?;
        return Ok(format!("{NATIVE_MODULE_ID_PREFIX}{name}"));
    }

    let props = description_module_args(description);
    let output = run_pactl(&[
        "load-module",
        "module-null-sink",
        "media.class=Audio/Source/Virtual",
        &format!("sink_name={name}"),
        &props[0],
        &props[1],
        &props[2],
        "channel_map=front-left,front-right",
    ])?;
    Ok(output.trim().to_string())
}

pub fn find_module_id_by_sink_name(sink_name: &str) -> Result<Option<String>, BackendError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    for line in output.lines() {
        let Some((module_id, args)) = parse_module_short_line(line) else {
            continue;
        };
        if args.contains(&format!("sink_name={sink_name}")) {
            return Ok(Some(module_id));
        }
    }
    Ok(None)
}

/// Whether a live `sink_name=` belongs in the plain virtual-device registry
/// (`VirtualDeviceRegistry`/`RuntimeGraph.devices`) — the canonical
/// classification predicate `list_pipe_deck_modules`'s scan filters through.
/// `pipe-deck-feed-*` (mix-source feed sinks) and `pipe-deck-proc-*` (PD-032
/// processing nodes, `RuntimeGraph.processing_nodes`) both have their own
/// separate representation and must never be absorbed here — that would
/// silently misclassify them as a plain `Device`, the #105-style
/// "enrichment doesn't recognize the new object" failure PD-032 exists to
/// prevent. See `naming_allowlist_coverage` below for the cross-file
/// assertion this predicate is meant to keep true.
pub(crate) fn belongs_in_virtual_device_registry(system_name: &str) -> bool {
    system_name.starts_with("pipe-deck-")
        && !system_name.starts_with("pipe-deck-feed-")
        && !system_name.starts_with("pipe-deck-proc-")
}

/// Discovers every live `pipe-deck-*` virtual device. Prefers a native
/// node-scan ([`native::list_nodes`]) over the legacy `pactl list modules
/// short` scan — the node-scan sees every virtual device uniformly
/// regardless of how it was created, whereas a plain `adapter` node (any
/// device created via [`native::create_output`]/[`native::create_input`])
/// has no Pulse "module" entry at all and is invisible to the module-scan
/// path (see this module's own file-level PD-049 note). Falls back to the
/// module-scan only if the native connection never started.
pub fn list_pipe_deck_modules() -> Result<Vec<PactlVirtualModule>, BackendError> {
    if let Some(nodes) = native::list_nodes() {
        return Ok(list_pipe_deck_modules_from_native(nodes));
    }
    list_pipe_deck_modules_from_pactl()
}

fn list_pipe_deck_modules_from_native(
    nodes: Vec<crate::backend::linux::pw_virtual_device_native::NodeInfo>,
) -> Vec<PactlVirtualModule> {
    let config_labels = configured_virtual_labels();
    let mut entries = Vec::new();

    for node in nodes {
        if !belongs_in_virtual_device_registry(&node.system_name) {
            continue;
        }
        let slug = node
            .system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(&node.system_name);
        let multi = node.system_name.starts_with("pipe-deck-split-");
        let direction = if node
            .media_class
            .as_deref()
            .is_some_and(|class| class.starts_with("Audio/Source"))
        {
            DeviceDirection::Input
        } else {
            DeviceDirection::Output
        };
        let label = configured_label_for_system_name(&node.system_name, &config_labels)
            .or(node.description)
            .unwrap_or_else(|| node.system_name.clone());

        entries.push(PactlVirtualModule {
            module_id: format!("{NATIVE_MODULE_ID_PREFIX}{}", node.system_name),
            device_id: format!("virtual-{slug}"),
            system_name: node.system_name,
            label,
            direction,
            multi,
        });
    }

    entries
}

fn list_pipe_deck_modules_from_pactl() -> Result<Vec<PactlVirtualModule>, BackendError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    let mut entries = Vec::new();
    let config_labels = configured_virtual_labels();

    for line in output.lines() {
        let Some((module_id, args)) = parse_module_short_line(line) else {
            continue;
        };
        let Some(system_name) = extract_arg_value(&args, "sink_name=") else {
            continue;
        };
        if !belongs_in_virtual_device_registry(&system_name) {
            continue;
        }
        let slug = system_name
            .strip_prefix("pipe-deck-")
            .unwrap_or(&system_name);
        let multi = system_name.starts_with("pipe-deck-split-");
        let direction = if args.contains("media.class=Audio/Source/Virtual") {
            DeviceDirection::Input
        } else {
            DeviceDirection::Output
        };
        let label = configured_label_for_system_name(&system_name, &config_labels)
            .or_else(|| extract_description(&args))
            .unwrap_or_else(|| system_name.clone());

        entries.push(PactlVirtualModule {
            module_id,
            device_id: format!("virtual-{slug}"),
            system_name,
            label,
            direction,
            multi,
        });
    }

    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct PactlVirtualModule {
    pub module_id: String,
    pub device_id: String,
    pub system_name: String,
    pub label: String,
    pub direction: DeviceDirection,
    pub multi: bool,
}

pub fn unload_module(module_id: &str) -> Result<(), BackendError> {
    if let Some(system_name) = module_id.strip_prefix(NATIVE_MODULE_ID_PREFIX) {
        if let Some(result) = native::remove(system_name) {
            return result;
        }
    }
    run_pactl(&["unload-module", module_id]).map(|_| ())
}

/// Feed sink name for one mix-source contribution to one virtual mic. Each
/// source gets its own sink so its volume can be controlled independently of
/// the mic's other sources and of the source device's own volume.
pub fn feed_sink_name_for_mix_pair(mic_system_name: &str, source_system_name: &str) -> String {
    let mic_slug = mic_system_name
        .strip_prefix("pipe-deck-")
        .unwrap_or(mic_system_name);
    let source_slug = slugify_for_feed_name(source_system_name);
    format!("pipe-deck-feed-{mic_slug}-{source_slug}")
}

fn slugify_for_feed_name(system_name: &str) -> String {
    system_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

pub fn ensure_feed_sink_for_mix_pair(
    mic_system_name: &str,
    source_system_name: &str,
    mic_label: &str,
) -> Result<String, BackendError> {
    let feed_name = feed_sink_name_for_mix_pair(mic_system_name, source_system_name);
    if sink_exists(&feed_name)? {
        return Ok(feed_name);
    }
    create_null_sink(&feed_name, &feed_sink_description(mic_label))?;
    Ok(feed_name)
}

pub fn remove_feed_sink_for_mix_pair(
    mic_system_name: &str,
    source_system_name: &str,
) -> Result<(), BackendError> {
    let feed_name = feed_sink_name_for_mix_pair(mic_system_name, source_system_name);
    let _ = pw_link::disconnect_sink_monitor(&feed_name);
    remove_sink_by_name(&feed_name)
}

/// Removes any per-pair feed sink for `mic_system_name` whose source is no
/// longer part of `keep_source_system_names`. Call after every mix apply so
/// dropped sources don't leave orphaned sinks behind.
pub fn gc_feed_sinks_for_mix_pairs(
    mic_system_name: &str,
    keep_source_system_names: &std::collections::HashSet<String>,
) -> Result<(), BackendError> {
    let mic_slug = mic_system_name
        .strip_prefix("pipe-deck-")
        .unwrap_or(mic_system_name);
    let prefix = format!("pipe-deck-feed-{mic_slug}-");
    let keep_names: std::collections::HashSet<String> = keep_source_system_names
        .iter()
        .map(|name| feed_sink_name_for_mix_pair(mic_system_name, name))
        .collect();

    for (module_id, feed_name) in list_sink_names_for_prefix(&prefix)? {
        if keep_names.contains(&feed_name) {
            continue;
        }
        let _ = pw_link::disconnect_sink_monitor(&feed_name);
        unload_module(&module_id)?;
    }

    Ok(())
}

pub(crate) fn ensure_feed_sink_for_virtual_input(
    virtual_input_system_name: &str,
    label: &str,
) -> Result<String, BackendError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    let description = feed_sink_description(label);

    if sink_exists(&feed_name)? {
        sync_feed_sink_description(&feed_name, virtual_input_system_name, &description)?;
        return Ok(feed_name);
    }

    create_null_sink(&feed_name, &description)?;
    // The feed sink can be routinely destroyed and recreated (see
    // `gc_feed_sinks`, which drops it the moment it has no attached
    // sink-input, even though its virtual-input target is still around) —
    // without waiting for the recreated node's monitor ports to actually
    // register, the caller's immediate `pw_link::link_sink_monitor_to_target`
    // call finds no monitor ports yet and fails, which is exactly what made
    // reconnecting a stream to a virtual mic it was previously routed away
    // from unreliable. Same race already fixed in
    // `effects_ops.rs::remove_effect_chain_structural`.
    wait_for_monitor_ports_registered(&feed_name, Duration::from_secs(5));
    Ok(feed_name)
}

fn wait_for_monitor_ports_registered(name: &str, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if pw_link::has_output_ports(name) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn sync_feed_sink_description(
    feed_name: &str,
    virtual_input_system_name: &str,
    description: &str,
) -> Result<(), BackendError> {
    if sink_description(feed_name)?.as_deref() == Some(description) {
        return Ok(());
    }

    if feed_sink_in_use(feed_name)? {
        return Ok(());
    }

    remove_feed_sink_for_virtual_input(virtual_input_system_name)?;
    create_null_sink(feed_name, description)?;
    Ok(())
}

fn feed_sink_in_use(feed_name: &str) -> Result<bool, BackendError> {
    Ok(sink_has_incoming_connection(feed_name))
}

fn sink_description(name: &str) -> Result<Option<String>, BackendError> {
    let output = run_pactl(&["list", "sinks"])?;
    let mut current_name = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name: ") {
            current_name = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Description: ") {
            if current_name.as_deref() == Some(name) {
                return Ok(Some(rest.trim().to_string()));
            }
        }
    }

    Ok(None)
}

fn list_modules_for_sink_prefix(prefix: &str) -> Result<Vec<(String, String)>, BackendError> {
    let output = run_pactl(&["list", "modules", "short"])?;
    let mut entries = Vec::new();

    for line in output.lines() {
        let Some((module_id, args)) = parse_module_short_line(line) else {
            continue;
        };
        let Some(sink_name) = extract_arg_value(&args, "sink_name=") else {
            continue;
        };
        if sink_name.starts_with(prefix) {
            entries.push((module_id, sink_name));
        }
    }

    Ok(entries)
}

/// `pactl list modules short` is tab-separated: index, module name, arguments.
/// Arguments may contain spaces inside quoted property values.
fn parse_module_short_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if line.contains('\t') {
        let mut parts = line.splitn(3, '\t');
        let module_id = parts.next()?.trim().to_string();
        let _module_name = parts.next()?;
        let args = parts.next().unwrap_or("").trim().to_string();
        return Some((module_id, args));
    }

    let mut parts = line.splitn(3, char::is_whitespace);
    let module_id = parts.next()?.to_string();
    let _module_name = parts.next()?;
    let args = parts.next().unwrap_or("").to_string();
    Some((module_id, args))
}

fn description_module_args(description: &str) -> [String; 3] {
    let description = escape_sink_property(description);
    [
        format!("device.description=\"{description}\""),
        format!("node.description=\"{description}\""),
        format!("node.nick=\"{description}\""),
    ]
}

fn escape_sink_property(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn configured_virtual_labels() -> HashMap<String, String> {
    let mut labels = ConfigStore::new().device_aliases();
    for spec in ConfigStore::new().virtual_devices() {
        labels
            .entry(format!("pipe-deck-{}", spec.slug))
            .or_insert(spec.label);
    }
    labels
}

fn configured_label_for_system_name(
    system_name: &str,
    labels: &HashMap<String, String>,
) -> Option<String> {
    labels.get(system_name).cloned()
}

fn extract_arg_value(args: &str, prefix: &str) -> Option<String> {
    let start = args.find(prefix)? + prefix.len();
    let rest = &args[start..];
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    let end = rest.find(' ').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_description(args: &str) -> Option<String> {
    // node.nick survives legacy sink_properties bundles that truncated device.description.
    extract_quoted_property(args, "node.nick=\"")
        .or_else(|| extract_quoted_property(args, "node.description=\""))
        .or_else(|| extract_quoted_property(args, "device.description=\""))
}

fn extract_quoted_property(args: &str, marker: &str) -> Option<String> {
    let start = args.find(marker)? + marker.len();
    let rest = &args[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PD-032's naming-allowlist coverage assertion: a synthetic
    /// `pipe-deck-proc-*` name must be recognized as a Pipe Deck object by
    /// the canonical predicate (`core::models::is_pipe_deck_device`) while
    /// being explicitly excluded from the plain virtual-device registry
    /// (`belongs_in_virtual_device_registry`, used by `list_pipe_deck_modules`)
    /// — the same way `pipe-deck-feed-*` already is. A future call site that
    /// forgets this exclusion would silently misclassify a processing node
    /// as a `Device` on the next live rescan; this test is the guard against
    /// that regressing unnoticed.
    #[test]
    fn naming_allowlist_coverage_excludes_processing_nodes_from_virtual_device_registry() {
        let proc_name = "pipe-deck-proc-mixer-test";
        assert!(crate::core::models::is_pipe_deck_device(proc_name));
        assert!(!belongs_in_virtual_device_registry(proc_name));

        // Sibling prefixes keep their existing behavior unchanged.
        assert!(belongs_in_virtual_device_registry("pipe-deck-game-mix"));
        assert!(!belongs_in_virtual_device_registry("pipe-deck-feed-mic"));
        assert!(!belongs_in_virtual_device_registry("alsa_output.pci"));
    }

    #[test]
    fn is_per_pair_mix_feed_sink_recognizes_mix_pair_names() {
        let known_slugs: std::collections::HashSet<&str> = ["mic"].into_iter().collect();

        // Regression test: `gc_feed_sinks` (the generic playback-feed-sink
        // GC, run on every graph refresh) must never treat a per-pair
        // mix-source feed sink as fair game — it previously did, because its
        // "does this look like a bare mic feed sink" check matched the
        // mix-pair naming scheme too, silently tearing mixed sources down
        // moments after they were created.
        assert!(is_per_pair_mix_feed_sink(
            "mic-alsa_input.headset",
            &known_slugs
        ));
        assert!(!is_per_pair_mix_feed_sink("mic", &known_slugs));
        assert!(!is_per_pair_mix_feed_sink("some-other-thing", &known_slugs));
    }

    #[test]
    fn feed_sink_name_derives_from_virtual_input() {
        assert_eq!(
            feed_sink_name_for_virtual_input("pipe-deck-test"),
            "pipe-deck-feed-test"
        );
        assert_eq!(
            feed_sink_name_for_virtual_input("pipe-deck-virtual-input"),
            "pipe-deck-feed-virtual-input"
        );
    }

    #[test]
    fn feed_sink_description_uses_virtual_mic_label() {
        assert_eq!(
            feed_sink_description("YouTube to Discord"),
            "YouTube to Discord (Pipe Deck route)"
        );
    }

    #[test]
    fn parse_module_short_line_preserves_quoted_spaces() {
        let line = "42\tmodule-null-sink\tsink_name=pipe-deck-the-run node.description=\"The Run\" node.nick=\"The Run\" device.description=\"The Run\"";
        let (id, args) = parse_module_short_line(line).unwrap();
        assert_eq!(id, "42");
        assert_eq!(
            extract_arg_value(&args, "sink_name="),
            Some("pipe-deck-the-run".into())
        );
        assert_eq!(extract_description(&args), Some("The Run".into()));
    }

    #[test]
    fn parse_module_short_line_space_separated_args() {
        let line = r#"12 module-null-sink sink_name=pipe-deck-game-mix node.description="Game Mix" node.nick="Game Mix" device.description="Game Mix""#;
        let (id, args) = parse_module_short_line(line).unwrap();
        assert_eq!(id, "12");
        assert_eq!(extract_description(&args), Some("Game Mix".into()));
    }

    #[test]
    fn extract_description_prefers_node_nick_for_legacy_modules() {
        let args = r#"sink_name=pipe-deck-old sink_properties=device.description="Test" node.description="Test" node.nick="Test With Name Spaces""#;
        assert_eq!(
            extract_description(args),
            Some("Test With Name Spaces".into())
        );
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as every
    //! other `live_tests` module in this codebase. Exercises
    //! `list_pipe_deck_modules` end to end (the actual call site
    //! `core::restore`/`VirtualDeviceRegistry::discover_from_pactl` use, not
    //! `pw_virtual_device_native::list_nodes` directly) to confirm the
    //! native node-scan this module now prefers produces a real
    //! `PactlVirtualModule` entry a caller can round-trip through
    //! `unload_module` — the actual integration point #432's Gap 2 changes,
    //! not just the lower-level index.
    use super::*;
    use crate::backend::linux::live::LinuxPipeWireBackend;
    use crate::backend::AudioBackend;
    use std::thread;
    use std::time::Duration;

    #[test]
    #[ignore]
    fn list_pipe_deck_modules_discovers_a_natively_created_device_and_can_unload_it() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let device = backend
            .create_virtual_output("Pipe Deck List Modules Test", false)
            .expect("create should succeed");

        let found = (0..20).find_map(|_| {
            let entries = list_pipe_deck_modules().expect("list_pipe_deck_modules should succeed");
            let entry = entries
                .into_iter()
                .find(|entry| entry.system_name == device.system_name);
            if entry.is_some() {
                return entry;
            }
            thread::sleep(Duration::from_millis(100));
            None
        });
        let entry =
            found.expect("expected list_pipe_deck_modules to discover the natively-created device");
        assert_eq!(entry.direction, DeviceDirection::Output);
        assert_eq!(entry.label, "Pipe Deck List Modules Test");

        unload_module(&entry.module_id)
            .expect("unload_module should succeed via the entry's own module_id");

        let removed = (0..20).any(|_| {
            let still_present = list_pipe_deck_modules()
                .expect("list_pipe_deck_modules should succeed")
                .iter()
                .any(|entry| entry.system_name == device.system_name);
            if !still_present {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            removed,
            "expected list_pipe_deck_modules to no longer report the device after unload_module"
        );
    }

    /// Regression test for the removal bug #432's Gap 2 fixed: before
    /// `remove_sink_by_name` tried `native::remove` directly,
    /// `remove_feed_sink_for_virtual_input` resolved its target via
    /// `find_module_id_by_sink_name` (a `pactl list modules short` scan),
    /// which can never find a natively-created feed sink — so removal
    /// silently no-opped and the feed sink leaked forever. Creates a
    /// virtual input, ensures its feed sink, removes it, and confirms via
    /// `pactl` — an independent pipeline — that it's actually gone.
    #[test]
    #[ignore]
    fn removes_a_natively_created_feed_sink() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let mic = backend
            .create_virtual_input("Pipe Deck Feed Sink Removal Test")
            .expect("create should succeed");

        let feed_name = ensure_feed_sink_for_virtual_input(
            &mic.system_name,
            "Pipe Deck Feed Sink Removal Test",
        )
        .expect("ensure_feed_sink_for_virtual_input should succeed");

        let created = (0..20).any(|_| {
            if sink_exists(&feed_name).unwrap_or(false) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            created,
            "expected the feed sink to be visible via pactl after creation"
        );

        remove_feed_sink_for_virtual_input(&mic.system_name)
            .expect("remove_feed_sink_for_virtual_input should succeed");

        let removed = (0..20).any(|_| {
            if !sink_exists(&feed_name).unwrap_or(true) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            removed,
            "expected the feed sink to be gone from pactl's own listing after removal"
        );

        let _ = backend.remove_virtual_device(&mic.system_name);
    }

    /// Regression test for the same bug as
    /// `removes_a_natively_created_feed_sink`, but through `gc_feed_sinks`'s
    /// own discovery path (`list_sink_names_for_prefix`) instead of a direct
    /// `remove_feed_sink_for_virtual_input` call — before this fix,
    /// `gc_feed_sinks` discovered orphaned feed sinks via `pactl list
    /// modules short`, which never sees a natively-created feed sink, so it
    /// would silently never collect one no matter how long it sat unused
    /// with no owning virtual input.
    #[test]
    #[ignore]
    fn gc_feed_sinks_collects_an_orphaned_natively_created_feed_sink() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let backend =
            LinuxPipeWireBackend::new().expect("backend should start against a real session");
        let mic = backend
            .create_virtual_input("Pipe Deck GC Feed Sink Test")
            .expect("create should succeed");

        let feed_name =
            ensure_feed_sink_for_virtual_input(&mic.system_name, "Pipe Deck GC Feed Sink Test")
                .expect("ensure_feed_sink_for_virtual_input should succeed");
        let created = (0..20).any(|_| {
            if sink_exists(&feed_name).unwrap_or(false) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            created,
            "expected the feed sink to be visible via pactl after creation"
        );

        // Empty `known_virtual_inputs` marks every feed sink as orphaned
        // (its owning virtual input is "gone" as far as this call is
        // concerned) — the same signal a real orphan (owner removed without
        // its feed sink being cleaned up first) would produce.
        gc_feed_sinks(&std::collections::HashSet::new()).expect("gc_feed_sinks should succeed");

        let removed = (0..20).any(|_| {
            if !sink_exists(&feed_name).unwrap_or(true) {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
            false
        });
        assert!(
            removed,
            "expected gc_feed_sinks to collect the orphaned natively-created feed sink"
        );

        let _ = backend.remove_virtual_device(&mic.system_name);
    }
}
