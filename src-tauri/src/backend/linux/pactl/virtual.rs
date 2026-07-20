use crate::config::store::ConfigStore;
use crate::core::models::DeviceDirection;
use crate::backend::BackendError;
use crate::backend::linux::pactl::parse::{list_sink_inputs, load_sink_index_names};
use crate::backend::linux::pactl::run_pactl;
use crate::backend::linux::pw_link;
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

pub fn virtual_device_in_use(system_name: &str) -> Result<bool, BackendError> {
    let sink_names = load_sink_index_names();
    Ok(list_sink_inputs().iter().any(|input| {
        input
            .sink_index
            .and_then(|index| sink_names.get(&index))
            .is_some_and(|name| name == system_name)
    }))
}

/// The sink-input indices (app playback streams) currently on `system_name`.
pub fn sink_input_indices_on(system_name: &str) -> Vec<u32> {
    let sink_names = load_sink_index_names();
    list_sink_inputs()
        .iter()
        .filter(|input| {
            input
                .sink_index
                .and_then(|index| sink_names.get(&index))
                .is_some_and(|name| name == system_name)
        })
        .map(|input| input.index)
        .collect()
}

/// Retries `pactl move-sink-input` until `sink_input_indices_on(target_system_name)`
/// confirms the move actually took, or `timeout` elapses. A single fire-and-forget
/// move call can silently fail if `target_system_name` isn't live as a real sink at
/// that exact instant — e.g. immediately after a `filter-chain.service` restart or a
/// plain-sink recreation that's still a beat away from actually completing under
/// real system load, even though the caller's own shorter wait (`wait_for_sink`,
/// `restart_filter_chain_service`'s `is-active` poll) already gave up and returned.
/// Without this retry, audio held on the scratch "Pipe Deck (temporary hold)" sink
/// during an effects swap can get permanently stranded there — the move was only
/// ever attempted once, at the one moment the target wasn't ready yet, and nothing
/// else in the app ever revisits it.
pub fn move_sink_input_with_retry(index: u32, target_system_name: &str, timeout: Duration) {
    let start = Instant::now();
    loop {
        let _ = super::move_sink_input_to_sink_name(index, target_system_name);
        if sink_input_indices_on(target_system_name).contains(&index) {
            return;
        }
        if start.elapsed() > timeout {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
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
    if !sink_input_indices_on(HOLDING_SINK_NAME).is_empty() {
        return Ok(());
    }
    if let Some(module_id) = find_module_id_by_sink_name(HOLDING_SINK_NAME)? {
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

pub fn remove_feed_sink_for_virtual_input(virtual_input_system_name: &str) -> Result<(), BackendError> {
    let feed_name = feed_sink_name_for_virtual_input(virtual_input_system_name);
    let _ = pw_link::disconnect_sink_monitor(&feed_name);
    if let Some(module_id) = find_module_id_by_sink_name(&feed_name)? {
        unload_module(&module_id)?;
    }
    Ok(())
}

pub fn gc_feed_sinks(known_virtual_inputs: &std::collections::HashSet<String>) -> Result<(), BackendError> {
    let sink_names = load_sink_index_names();
    let sinks_with_inputs: std::collections::HashSet<String> = list_sink_inputs()
        .iter()
        .filter_map(|input| {
            input
                .sink_index
                .and_then(|index| sink_names.get(&index).cloned())
        })
        .collect();

    let known_slugs: std::collections::HashSet<&str> = known_virtual_inputs
        .iter()
        .filter_map(|name| name.strip_prefix("pipe-deck-"))
        .collect();

    for (module_id, feed_name) in list_modules_for_sink_prefix("pipe-deck-feed-")? {
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
        let in_use = sinks_with_inputs.contains(&feed_name);

        if virtual_exists && in_use {
            continue;
        }

        let _ = pw_link::disconnect_sink_monitor(&feed_name);
        unload_module(&module_id)?;
    }

    Ok(())
}

fn is_per_pair_mix_feed_sink(feed_sink_rest: &str, known_slugs: &std::collections::HashSet<&str>) -> bool {
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
    let output = run_pactl(&["list", "sinks", "short"])?;
    Ok(output.lines().any(|line| line.split_whitespace().nth(1) == Some(name)))
}

/// The source-direction counterpart to `sink_exists` — used to confirm a
/// virtual input device (backed by `module-null-sink` with
/// `media.class=Audio/Source/Virtual`, see `create_virtual_source`) has
/// (re)appeared after a Structural Apply swap (PD-024).
pub fn source_exists(name: &str) -> Result<bool, BackendError> {
    let output = run_pactl(&["list", "sources", "short"])?;
    Ok(output.lines().any(|line| line.split_whitespace().nth(1) == Some(name)))
}

pub fn create_null_sink(name: &str, description: &str) -> Result<String, BackendError> {
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

pub fn list_pipe_deck_modules() -> Result<Vec<PactlVirtualModule>, BackendError> {
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
        if !system_name.starts_with("pipe-deck-") || system_name.starts_with("pipe-deck-feed-") {
            continue;
        }
        let slug = system_name.strip_prefix("pipe-deck-").unwrap_or(&system_name);
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
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
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
    if let Some(module_id) = find_module_id_by_sink_name(&feed_name)? {
        unload_module(&module_id)?;
    }
    Ok(())
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

    for (module_id, feed_name) in list_modules_for_sink_prefix(&prefix)? {
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
    let sink_names = load_sink_index_names();
    Ok(list_sink_inputs().iter().any(|input| {
        input
            .sink_index
            .and_then(|index| sink_names.get(&index))
            .is_some_and(|name| name == feed_name)
    }))
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
    if rest.starts_with('"') {
        let end = rest[1..].find('"')? + 1;
        return Some(rest[1..end].to_string());
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

    #[test]
    fn is_per_pair_mix_feed_sink_recognizes_mix_pair_names() {
        let known_slugs: std::collections::HashSet<&str> = ["mic"].into_iter().collect();

        // Regression test: `gc_feed_sinks` (the generic playback-feed-sink
        // GC, run on every graph refresh) must never treat a per-pair
        // mix-source feed sink as fair game — it previously did, because its
        // "does this look like a bare mic feed sink" check matched the
        // mix-pair naming scheme too, silently tearing mixed sources down
        // moments after they were created.
        assert!(is_per_pair_mix_feed_sink("mic-alsa_input.headset", &known_slugs));
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
