use crate::config::ConfigStore;
use crate::core::models::{
    effect_input_name_for_device, is_pipe_deck_device, ApplyResult, DeviceDirection, DeviceKind, EffectChainConfig, EffectStage,
};
use crate::pipewire::fx_capability::FxCapabilities;
use crate::pipewire::fx_validate::PreflightResult;
#[cfg(test)]
use crate::backend::linux::{pactl, pw_link};
use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn get_effect_chains(&self) -> Result<HashMap<String, EffectChainConfig>, EngineError> {
        ConfigStore::new()
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))
    }

    /// What the installed system can actually back for live effects — used
    /// to grey out UI controls nothing can realize, rather than let a user
    /// configure a stage that would silently fail (or worse, get force-fit
    /// through an unvalidated path) at apply time.
    pub fn get_effect_capabilities(&self) -> FxCapabilities {
        self.adapter.effect_capabilities()
    }

    /// Validates a candidate chain against the v1 safety contract without
    /// writing anything or touching PipeWire — safe to call on every slider
    /// change so the UI can show blocking reasons before the user ever hits
    /// Apply.
    pub fn preflight_effect_chain(&self, config: &EffectChainConfig) -> PreflightResult {
        self.adapter.preflight_effect_chain(config)
    }

    /// Whether a device currently has a live effects chain loaded (i.e. a
    /// prior `apply_effect_chain_structural` succeeded and hasn't been
    /// reverted) — lets the UI switch a slider drag between "just persist"
    /// and "push live params in real time" without re-deriving that from
    /// scratch on every keystroke.
    pub fn is_effect_chain_live(&self, device_id: &str) -> bool {
        let Some(device) = self.graph.devices.iter().find(|device| device.id == device_id) else {
            return false;
        };
        self.adapter.is_effect_chain_loaded(&device.system_name)
    }

    pub fn set_device_effects(
        &mut self,
        device_id: &str,
        config: EffectChainConfig,
    ) -> Result<ApplyResult, EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .ok_or_else(|| EngineError::Adapter(format!("device not found: {device_id}")))?;

        if !is_pipe_deck_device(&device.system_name) {
            return Err(EngineError::Adapter(
                "effects may only be applied to pipe-deck virtual devices".into(),
            ));
        }

        let store = ConfigStore::new();
        if config.is_active() {
            // This is the persist-only path (bypass toggle before any stage
            // is live, dynamics-stage persist) — it never itself applies or
            // reverts live processing, so it must not clobber an existing
            // `live: true` set by a real apply just because the frontend's
            // `config` doesn't know about this Rust-only bookkeeping field.
            let mut persisted_config = config;
            let previously_live = store
                .effect_chains()
                .ok()
                .and_then(|chains| chains.get(device_id).map(|existing| existing.live))
                .unwrap_or(false);
            persisted_config.live = previously_live;
            store
                .set_effect_chain(device_id, &persisted_config)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        } else {
            store
                .remove_effect_chain(device_id)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// PD-025: node-scoped effects UI entry point. Appends `stage` to the
    /// device's chain and applies immediately — there is no separate
    /// "enable live effects" step anymore; the deliberate act of adding a
    /// stage via the Routing graph/Mixer/Effects-page UI *is* the explicit
    /// action PD-017 requires before a restart-carrying apply.
    pub fn add_effect_stage(&mut self, device_id: &str, stage: EffectStage) -> Result<ApplyResult, EngineError> {
        let mut config = self.effect_chain_for(device_id)?;
        config.stages.push(stage);
        self.apply_effect_chain_structural(device_id, &config)
    }

    /// Removes the stage matching `stage_id`. If no stages remain (and no
    /// dynamics stage is enabled), fully reverts the device via
    /// `remove_effect_chain_structural` rather than applying an empty chain.
    pub fn remove_effect_stage(&mut self, device_id: &str, stage_id: &str) -> Result<ApplyResult, EngineError> {
        let mut config = self.effect_chain_for(device_id)?;
        config.stages.retain(|stage| stage.id() != stage_id);
        if config.is_active() {
            self.apply_effect_chain_structural(device_id, &config)
        } else {
            self.remove_effect_chain_structural(device_id)
        }
    }

    /// Reorders `stages` to match `ordered_stage_ids` and re-applies.
    /// Nothing to visibly demonstrate with only one stage kind in v1, but
    /// the plumbing needs to exist now so a second stage kind doesn't need
    /// another backend rewrite.
    pub fn reorder_effect_stages(
        &mut self,
        device_id: &str,
        ordered_stage_ids: &[String],
    ) -> Result<ApplyResult, EngineError> {
        let mut config = self.effect_chain_for(device_id)?;
        let mut reordered = Vec::with_capacity(config.stages.len());
        for id in ordered_stage_ids {
            if let Some(index) = config.stages.iter().position(|stage| stage.id() == id) {
                reordered.push(config.stages.remove(index));
            }
        }
        // Any stage not named in `ordered_stage_ids` (shouldn't happen from
        // a well-behaved caller) keeps its relative place at the end rather
        // than silently vanishing.
        reordered.append(&mut config.stages);
        config.stages = reordered;
        self.apply_effect_chain_structural(device_id, &config)
    }

    fn effect_chain_for(&self, device_id: &str) -> Result<EffectChainConfig, EngineError> {
        let chains = ConfigStore::new()
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(chains.get(device_id).cloned().unwrap_or_default())
    }

    /// Structural Apply: the rare, explicit path that actually loads or tears
    /// down live DSP — loads `libpipewire-module-filter-chain` directly into
    /// the running PipeWire session via `pipewire::native_host` (PD-029; no
    /// restart of anything, the pre-#149 conf.d-drop-in-plus-service-restart
    /// mechanism this used to describe no longer exists), verifies the
    /// effects sink/source actually reappeared, and re-links whatever the
    /// device was already routed to (outputs) or fed by (inputs). Any
    /// failure automatically rolls back to the plain sink/source so the
    /// device is never left missing or broken.
    ///
    /// Scope for this pass: EQ + master gain only, on `pipe-deck-*` virtual
    /// **output or input** devices not currently carrying audio (PD-024
    /// extends the original output-only scope to virtual inputs/mics).
    /// Physical hardware devices remain out of scope per PD-020 — a physical
    /// mic gets processed by wrapping it in a virtual input via the existing
    /// mix-sources mechanism instead. Dynamics stages are rejected by
    /// `fx_validate::preflight` unless a real backing plugin is confirmed
    /// present (currently: none are, on any known PipeWire version for
    /// limiter/compressor, and only if a LADSPA plugin is installed for
    /// noise gate).
    pub fn apply_effect_chain_structural(
        &mut self,
        device_id: &str,
        config: &EffectChainConfig,
    ) -> Result<ApplyResult, EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        if !is_pipe_deck_device(&device.system_name) {
            return Err(EngineError::InvalidInput(
                "effects may only be applied to pipe-deck virtual devices".to_string(),
            ));
        }
        // Device-attached output effects (the old Bus mechanism, #287) were
        // retired alongside VirtualRole::Bus — a virtual output device can
        // no longer host effects directly. The dedicated EQ5Band processing
        // node (PD-032) is the replacement. Virtual input (mic) devices are
        // unaffected; that path never depended on Bus.
        if device.kind != DeviceKind::Virtual || device.direction != DeviceDirection::Input {
            return Err(EngineError::InvalidInput(
                "live effects currently only support virtual input (mic) devices - use a dedicated EQ node for output effects".to_string(),
            ));
        }

        let preflight = self.adapter.preflight_effect_chain(config);
        if !preflight.ok {
            return Err(EngineError::InvalidInput(preflight.blocking_reasons.join("; ")));
        }

        let is_input = device.direction == DeviceDirection::Input;

        // Only virtual input (mic) devices reach this point (see the gate
        // above) — `downstream_and_upstream_routes` always returns an empty
        // upstream list for `is_input`, since device-attached output effects
        // (the only case that ever populated it) are retired.
        let (downstream_target_ids, _upstream_sources) = self.downstream_and_upstream_routes(&device, is_input);
        let downstream_targets: Vec<_> = downstream_target_ids
            .iter()
            .filter_map(|id| self.graph.devices.iter().find(|d| &d.id == id).cloned())
            .collect();

        // Input-direction-only: whatever's currently monitor-linked into this
        // device's `input_*` ports (mic-mix feed sinks, or the single feed
        // sink generic routing uses) must be captured *before* the module
        // load below — once the device's module is unloaded, `input_*` ports
        // on this name no longer exist to discover them from.
        let mic_feeders = if is_input { self.adapter.list_mic_feeds(&device.system_name, true) } else { Vec::new() };

        let held_sink_inputs = self.hold_sink_inputs_for_swap_if_output(&device, is_input)?;

        let apply_result = self
            .adapter
            .load_effect_chain(&device, config, &downstream_targets, &mic_feeders)
            .map(|_playback_name| ());

        if let Err(error) = apply_result {
            let _ = self.adapter.unload_effect_chain(&device.system_name);
            // Only virtual input (mic) devices reach this point — device-
            // attached output effects were retired with VirtualRole::Bus
            // (see the gate above), so `downstream_target_ids`/
            // `upstream_sources` are always empty here and this is always
            // the input branch.
            debug_assert!(is_input);
            let _ = self.adapter.revert_to_plain_device(&device, false);
            let _ = self
                .adapter
                .relink_mic_feeds(&mic_feeders, &device.system_name, &device.system_name, true);
            let _ = self.adapter.release_held_sink_inputs(&held_sink_inputs, &device.system_name);
            let _ = self.refresh_graph();
            return Err(EngineError::Adapter(format!(
                "effects apply failed and was rolled back to no effects: {error}"
            )));
        }

        let _ = self.adapter.release_held_sink_inputs(&held_sink_inputs, &device.system_name);

        // Persist `live: true` — PD-017 §1's signal that this chain was
        // explicitly confirmed live (see the field doc on
        // `EffectChainConfig::live`), not just configured.
        let mut persisted_config = config.clone();
        persisted_config.live = true;
        ConfigStore::new()
            .set_effect_chain(device_id, &persisted_config)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: Some(format!("Effects applied to {}", device.label)),
        })
    }

    /// Live Params: pushes updated EQ/gain values straight to the already-running
    /// filter-chain node via `pw-cli set-param` — no conf write, no restart, no
    /// relinking. Safe to call on every slider tick. Only works once
    /// `apply_effect_chain_structural` has actually loaded a chain for this
    /// device; if it hasn't (or the node isn't currently resolvable), this
    /// returns a `success: false` result rather than erroring loudly, since a
    /// slider drag racing ahead of the initial Apply is an expected transient
    /// state, not a bug.
    pub fn set_effect_chain_live_params(
        &mut self,
        device_id: &str,
        config: &EffectChainConfig,
    ) -> Result<ApplyResult, EngineError> {
        if self.graph.data_source == "mock" {
            return Ok(ApplyResult {
                success: true,
                message: Some("live params updated (mock)".to_string()),
            });
        }

        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        let preflight = self.adapter.preflight_effect_chain(config);
        if !preflight.ok {
            return Ok(ApplyResult {
                success: false,
                message: Some(preflight.blocking_reasons.join("; ")),
            });
        }

        let Some(node_id) = self
            .adapter
            .find_live_node_id(&device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?
        else {
            return Ok(ApplyResult {
                success: false,
                message: Some("Live effects aren't enabled yet for this device".to_string()),
            });
        };

        self.adapter
            .push_effect_chain_live_params(&device.system_name, node_id, config)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        // A node was already resolvable above, so this chain is already
        // live — preserve that regardless of what the caller's `config`
        // says (the frontend doesn't track this Rust-only bookkeeping flag,
        // so a naive persist here would silently reset it to `false`).
        let mut persisted_config = config.clone();
        persisted_config.live = true;
        ConfigStore::new()
            .set_effect_chain(device_id, &persisted_config)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Reverts a device from an effects-hosted sink back to the plain
    /// pactl null-sink, re-linking whatever it was routed to. Used both for
    /// "remove effects" and as the rollback path when a Structural Apply fails.
    pub fn remove_effect_chain_structural(&mut self, device_id: &str) -> Result<ApplyResult, EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        if !self.adapter.is_effect_chain_loaded(&device.system_name) {
            return Ok(ApplyResult {
                success: true,
                message: Some("no live effects to remove".to_string()),
            });
        }

        let is_input = device.direction == DeviceDirection::Input;

        let (downstream_target_ids, upstream_sources) = self.downstream_and_upstream_routes(&device, is_input);

        let held_sink_inputs = self.hold_sink_inputs_for_swap_if_output(&device, is_input)?;

        // Capture whatever's currently feeding the live effects inlet before
        // tearing it down (input-direction only) — same "must discover
        // before the swap" reasoning as `apply_effect_chain_structural`.
        let mic_feeders = if is_input {
            self.adapter
                .list_mic_feeds(&effect_input_name_for_device(&device.system_name), false)
        } else {
            Vec::new()
        };

        // Both of these can fail (an unreachable native-effects daemon, a
        // slow-to-reappear plain sink) — unlike `apply_effect_chain_structural`'s
        // matching rollback branch, an early `?` return here would skip
        // `release_held_sink_inputs` entirely, permanently stranding whatever
        // was playing into this device on the `Pipe Deck (temporary hold)`
        // sink with no visible downstream connection. Always release before
        // propagating either error, exactly like apply's failure path does.
        if let Err(error) = self.discard_effect_chain_conf(&device.system_name) {
            let _ = self.adapter.release_held_sink_inputs(&held_sink_inputs, &device.system_name);
            return Err(error);
        }
        if let Err(error) = self.adapter.revert_to_plain_device(&device, true) {
            let _ = self.adapter.release_held_sink_inputs(&held_sink_inputs, &device.system_name);
            return Err(EngineError::Adapter(error.to_string()));
        }

        let _ = self.adapter.release_held_sink_inputs(&held_sink_inputs, &device.system_name);

        self.refresh_graph()?;
        if is_input {
            self.adapter
                .relink_mic_feeds(
                    &mic_feeders,
                    &effect_input_name_for_device(&device.system_name),
                    &device.system_name,
                    true,
                )
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
        }
        // The output-direction branch that used to re-link
        // `downstream_target_ids`/`upstream_sources` via
        // `core::routing::apply_sink_targets` is gone along with
        // device-attached output effects (VirtualRole::Bus retirement) — a
        // pre-existing output device with a live legacy effect chain still
        // has its effects removed above (`revert_to_plain_device`), it just
        // won't be automatically re-linked to whatever it used to route to.
        // New effect chains can only ever be attached to virtual input
        // devices (see the gate in `apply_effect_chain_structural`), so
        // `downstream_target_ids`/`upstream_sources` are always empty for
        // any chain created after this change.
        let _ = (&downstream_target_ids, &upstream_sources);

        ConfigStore::new()
            .remove_effect_chain(device_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: Some(format!("Effects removed from {}", device.label)),
        })
    }

    /// Output-direction-only routing state a structural swap (apply or
    /// remove) needs captured *before* touching the device's module, shared
    /// verbatim by `apply_effect_chain_structural` and
    /// `remove_effect_chain_structural`:
    ///
    /// - `downstream_target_ids`: what the device currently routes to.
    /// - `upstream_sources`: any OTHER virtual output currently chained into
    ///   this one (PD-026 — bus-into-bus). The module load/revert destroys
    ///   and recreates this device's sink node, which silently severs any
    ///   raw pw-link an upstream device's monitor already held into it —
    ///   nothing about the swap itself knows to restore an *incoming*
    ///   device-level route, only this device's own outgoing one, so it has
    ///   to be discovered up front and re-applied after.
    ///
    /// Neither concept applies to a virtual input/mic device — it has no
    /// downstream routing targets of its own, and nothing else can be
    /// "chained into" a mic — so both come back empty for `is_input`.
    fn downstream_and_upstream_routes(&self, device: &crate::core::models::Device, is_input: bool) -> (Vec<String>, Vec<(String, Vec<String>)>) {
        let downstream_target_ids = if is_input {
            Vec::new()
        } else if device.current_targets.is_empty() {
            device.current_target.clone().into_iter().collect::<Vec<_>>()
        } else {
            device.current_targets.clone()
        };

        let upstream_sources: Vec<(String, Vec<String>)> = if is_input {
            Vec::new()
        } else {
            self.graph
                .devices
                .iter()
                .filter(|other| other.id != device.id)
                .filter_map(|other| {
                    let targets = other.resolved_targets();
                    targets.contains(&device.id).then(|| (other.id.clone(), targets))
                })
                .collect()
        };

        (downstream_target_ids, upstream_sources)
    }

    /// The device may currently be carrying audio (apps actively playing
    /// into it, output-direction only — a source's "in use" means
    /// source-outputs, which a structural swap doesn't attempt to
    /// hold/restore, see PD-024 for scope). Rather than refusing to swap at
    /// all, briefly hold those streams on a scratch sink so the caller can
    /// move them back once the swapped-to sink is confirmed up. Shared by
    /// `apply_effect_chain_structural` and `remove_effect_chain_structural`.
    fn hold_sink_inputs_for_swap_if_output(&self, device: &crate::core::models::Device, is_input: bool) -> Result<Vec<String>, EngineError> {
        if is_input {
            return Ok(Vec::new());
        }
        self.adapter
            .hold_sink_inputs_for_swap(&device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))
    }

    /// Unloads `system_name`'s live effects chain, if one is loaded.
    /// Best-effort/no-op if nothing is loaded. Shared by
    /// `remove_effect_chain_structural` (primary operation, propagates
    /// failures via `?`) and `virtual_ops::remove_virtual_device`
    /// (best-effort cleanup ahead of destroying the device outright, which
    /// swallows this call's `Err` — the device being deleted regardless is
    /// not itself a reason to abort). Returns whether a chain was actually
    /// live.
    pub(super) fn discard_effect_chain_conf(&self, system_name: &str) -> Result<bool, EngineError> {
        let was_live = self.adapter.is_effect_chain_loaded(system_name);
        self.adapter
            .unload_effect_chain(system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        Ok(was_live)
    }

    pub fn restore_effect_chains(&mut self) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            return Ok(());
        }

        let store = ConfigStore::new();
        let chains = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let active = self.active_effect_chains(&chains);
        self.reapply_previously_live_effect_chains(&active);

        Ok(())
    }

    /// Re-establishes live processing (Structural Apply) for any device
    /// whose persisted chain has `live: true` — the signal the user had
    /// previously confirmed live processing for this device, so silently
    /// restoring it on app-boot/profile-swap restore isn't turning on live
    /// processing that was never explicitly approved (PD-017 §1). A chain
    /// that's configured but was never applied stays persist-only, same as
    /// before this existed. Skips anything the daemon already reports as
    /// loaded (e.g. `daemon::reconcile_live_effects_state` already reloaded
    /// it after a crash/restart) rather than reloading it redundantly. Each
    /// device is independent — one failing must not block the rest of
    /// restore, per #20's acceptance criteria.
    pub(super) fn reapply_previously_live_effect_chains(&mut self, active: &[(String, EffectChainConfig)]) {
        for (system_name, config) in active {
            if !config.live || self.adapter.is_effect_chain_loaded(system_name) {
                continue;
            }
            let Some(device_id) = self
                .graph
                .devices
                .iter()
                .find(|device| &device.system_name == system_name)
                .map(|device| device.id.clone())
            else {
                continue;
            };
            let _ = self.apply_effect_chain_structural(&device_id, config);
        }
    }

    /// Repairs the routing gap left by `daemon::reconcile_live_effects_state`
    /// (issue #206): after a native-effects daemon crash/restart, that
    /// function reloads persisted-active chains and restores audio
    /// processing immediately, but deliberately leaves downstream routing
    /// (fan-out targets) for it to re-derive elsewhere. There's no
    /// daemon->GUI push channel to react to that reload directly (the IPC
    /// socket is request/response only) — so instead this diffs "was each
    /// active chain's node live as of the last graph refresh" against "is
    /// it live now", using the GUI's own independent live PipeWire
    /// subscription, and re-applies the last-known targets the moment a
    /// chain flips from absent to live again. Called on every graph
    /// refresh; a no-op whenever nothing has actually transitioned.
    pub(super) fn reconcile_effect_chain_liveness_after_refresh(&mut self) {
        if self.graph.data_source == "mock" {
            return;
        }
        let Ok(chains) = ConfigStore::new().effect_chains() else {
            return;
        };
        let active = self.active_effect_chains(&chains);

        for (system_name, _config) in &active {
            let is_live = self.adapter.is_effect_chain_loaded(system_name);
            self.reconcile_one_effect_chain_liveness(system_name, is_live);
        }

        // Drop bookkeeping for chains no longer active, so it doesn't grow
        // unbounded and doesn't resurrect stale targets if a system_name is
        // ever reused by a different device later.
        let active_names: std::collections::HashSet<&String> = active.iter().map(|(name, _)| name).collect();
        self.effect_chain_liveness.retain(|name, _| active_names.contains(name));
        let active_ids: std::collections::HashSet<String> = self
            .graph
            .devices
            .iter()
            .filter(|device| active_names.contains(&device.system_name))
            .map(|device| device.id.clone())
            .collect();
        self.effect_chain_last_targets.retain(|id, _| active_ids.contains(id));
    }

    /// Split out from `reconcile_effect_chain_liveness_after_refresh` so the
    /// transition logic is unit-testable with an explicit `is_live` instead
    /// of requiring a real native-effects PipeWire session.
    fn reconcile_one_effect_chain_liveness(&mut self, system_name: &str, is_live: bool) {
        let Some(device) = self.graph.devices.iter().find(|device| device.system_name == system_name).cloned()
        else {
            return;
        };
        // v1 scope: output fan-out only. Mic-mix feeder recovery for input
        // devices is a real gap too, but needs its own capture-before-swap
        // treatment (see `apply_effect_chain_structural`'s `mic_feeders`)
        // rather than reusing this output-shaped diff — left for follow-up.
        if device.direction != DeviceDirection::Output {
            return;
        }

        let current_targets: Vec<String> = if !device.current_targets.is_empty() {
            device.current_targets.clone()
        } else {
            device.current_target.clone().into_iter().collect()
        };
        if is_live && !current_targets.is_empty() {
            self.effect_chain_last_targets.insert(device.id.clone(), current_targets);
        }

        // Re-linking a reappeared chain's last-known downstream targets used
        // `core::routing::apply_sink_targets`, retired along with
        // device-attached output effects (VirtualRole::Bus). Only a
        // pre-existing legacy output effect chain can still reach this
        // method at all (see the `direction != Output` early return above);
        // it no longer gets automatically re-linked on reappear.

        self.effect_chain_liveness.insert(system_name.to_string(), is_live);
    }

    pub(super) fn active_effect_chains(
        &self,
        chains: &std::collections::HashMap<String, EffectChainConfig>,
    ) -> Vec<(String, EffectChainConfig)> {
        chains
            .iter()
            .filter_map(|(device_id, config)| {
                if !config.is_active() {
                    return None;
                }
                let system_name = self
                    .graph
                    .devices
                    .iter()
                    .find(|device| device.id == *device_id)
                    .map(|device| device.system_name.clone())?;
                if !is_pipe_deck_device(&system_name) {
                    return None;
                }
                Some((system_name, config.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d on purpose: these hit a *real* PipeWire session and
    //! `filter-chain.service`, unlike every other test in this crate. Never
    //! run as part of `cargo test`/CI — only via
    //! `cargo test --lib -- --ignored apply_effect_chain_structural_round_trips_on_a_real_pipewire_session`,
    //! and only on a machine where that's safe to do. Exercises a disposable
    //! `pipe-deck-*` device this test creates and removes itself; never
    //! touches any device the user configured.
    use super::*;

    #[test]
    #[ignore]
    fn apply_effect_chain_structural_round_trips_on_a_real_pipewire_session() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_output("Pipe Deck Live Test")
            .expect("create disposable test device");

        let cleanup = |engine: &mut CoreEngine| {
            let _ = engine.remove_virtual_device(&created.system_name);
        };

        let device_id = created.device_id.clone();
        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 6,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let apply_result = engine.apply_effect_chain_structural(&device_id, &config);
        if let Err(error) = &apply_result {
            cleanup(&mut engine);
            panic!("structural apply failed: {error}");
        }

        let sink_live = pactl::sink_exists(&created.system_name).unwrap_or(false);
        if !sink_live {
            cleanup(&mut engine);
            panic!("effects sink did not appear after structural apply");
        }

        let remove_result = engine.remove_effect_chain_structural(&device_id);
        cleanup(&mut engine);
        remove_result.expect("remove_effect_chain_structural should revert cleanly");
    }

    /// PD-024: the same round trip as
    /// `apply_effect_chain_structural_round_trips_on_a_real_pipewire_session`,
    /// but for a virtual **input** (mic) device — confirms the capture-
    /// direction template actually swaps in, that the device still reports
    /// as `Audio/Source/Virtual` (not silently reverted to a plain sink),
    /// and that its Mixer Node feed (PD-032's replacement for the old
    /// mic-mix mechanism this test used to set up directly) survives both
    /// the apply and the removal.
    #[test]
    #[ignore]
    fn apply_effect_chain_structural_round_trips_on_a_virtual_input() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let mic = engine
            .create_virtual_input("Pipe Deck Live Input Test")
            .expect("create disposable test mic");
        let source = engine
            .create_virtual_output("Pipe Deck Live Input Test Source")
            .expect("create disposable test mix source");

        let cleanup = |engine: &mut CoreEngine, mixer_id: Option<&str>| {
            if let Some(id) = mixer_id {
                let _ = engine.remove_processing_node(id);
            }
            let _ = engine.remove_virtual_device(&mic.system_name);
            let _ = engine.remove_virtual_device(&source.system_name);
        };

        let device_id = mic.device_id.clone();
        let mixer = match engine.create_processing_node(
            "Pipe Deck Live Input Test Mixer",
            crate::core::models::ProcessingNodeSpecKind::Mixer,
        ) {
            Ok(node) => node,
            Err(error) => {
                cleanup(&mut engine, None);
                panic!("failed to create mixer node before testing effects: {error}");
            }
        };
        if let Err(error) =
            engine.connect_processing_node_port(&mixer.id, crate::core::models::PortDirection::Input, &source.device_id)
        {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("failed to connect mixer input before testing effects: {error}");
        }
        if let Err(error) =
            engine.connect_processing_node_port(&mixer.id, crate::core::models::PortDirection::Output, &device_id)
        {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("failed to connect mixer output before testing effects: {error}");
        }

        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 6,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let apply_result = engine.apply_effect_chain_structural(&device_id, &config);
        if let Err(error) = &apply_result {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("structural apply failed: {error}");
        }

        let source_live = pactl::source_exists(&mic.system_name).unwrap_or(false);
        if !source_live {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("effects source did not appear as Audio/Source/Virtual after structural apply");
        }

        let feeders_after_apply = pw_link::list_capture_sources_for_sink(&effect_input_name_for_device(
            &mic.system_name,
        ));
        if !feeders_after_apply.iter().any(|name| name == &mixer.system_name) {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("mixer feed did not survive the structural apply: {feeders_after_apply:?}");
        }

        let remove_result = engine.remove_effect_chain_structural(&device_id);
        if let Err(error) = &remove_result {
            cleanup(&mut engine, Some(&mixer.id));
            panic!("remove_effect_chain_structural failed: {error}");
        }

        let feeders_after_remove = pw_link::list_capture_sources_for_virtual_input(&mic.system_name);
        cleanup(&mut engine, Some(&mixer.id));
        assert!(
            feeders_after_remove.iter().any(|name| name == &mixer.system_name),
            "mixer feed did not survive removal: {feeders_after_remove:?}"
        );
    }

    /// Isolates issue #74's portable-DSP dispatch (`backend::linux::live`'s
    /// `native_dsp_host::supports(config)` branch, wired through
    /// `daemon::ipc`) from the *pre-existing, unrelated* mic-feeder-relink
    /// bug that `apply_effect_chain_structural_round_trips_on_a_virtual_input`
    /// also hits (confirmed: it fails identically with the portable dispatch
    /// forced off, i.e. on the original builtin-module path too — not
    /// something this issue introduced, not fixed here, out of scope for
    /// #74). No mixer/feeders are connected here, so
    /// `apply_effect_chain_structural`'s `relink_mic_feeds` call has nothing
    /// to relink and that bug never triggers — this test exercises
    /// everything else end to end: real `CoreEngine` → `effects_ops` →
    /// `AudioBackend::load_effect_chain` → real daemon IPC →
    /// `native_dsp_host`, confirming the device still reports as
    /// `Audio/Source/Virtual` after apply and reverts cleanly on remove.
    #[test]
    #[ignore]
    fn portable_dsp_dispatch_round_trips_an_eq5band_chain_with_no_mic_feeders() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let mic = engine
            .create_virtual_input("Pipe Deck Portable DSP Dispatch Test")
            .expect("create disposable test mic");
        let cleanup = |engine: &mut CoreEngine| {
            let _ = engine.remove_virtual_device(&mic.system_name);
        };

        let device_id = mic.device_id.clone();
        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 6,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };

        let apply_result = engine.apply_effect_chain_structural(&device_id, &config);
        if let Err(error) = &apply_result {
            cleanup(&mut engine);
            panic!("structural apply via the portable-DSP dispatch failed: {error}");
        }

        let source_live = pactl::source_exists(&mic.system_name).unwrap_or(false);
        if !source_live {
            cleanup(&mut engine);
            panic!("effects source did not appear as Audio/Source/Virtual after structural apply");
        }
        if !engine.is_effect_chain_live(&device_id) {
            cleanup(&mut engine);
            panic!("device does not report as having a live effect chain after apply");
        }

        let remove_result = engine.remove_effect_chain_structural(&device_id);
        cleanup(&mut engine);
        remove_result.expect("remove_effect_chain_structural should revert cleanly");
    }

    #[test]
    #[ignore]
    fn applies_effects_while_in_use_and_live_updates_without_restart() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_output("Pipe Deck Live Param Test")
            .expect("create disposable test device");
        let device_id = created.device_id.clone();

        // Keep a real sink-input alive on the device for the whole test, to
        // prove Structural Apply no longer needs the device to be idle.
        let mut player = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "while true; do paplay --device={} /usr/share/sounds/speech-dispatcher/test.wav; done",
                created.system_name
            ))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start background player");
        std::thread::sleep(Duration::from_millis(500));

        let cleanup = |engine: &mut CoreEngine, player: &mut std::process::Child| {
            let _ = player.kill();
            let _ = player.wait();
            let _ = engine.remove_virtual_device(&created.system_name);
        };

        let in_use = pactl::virtual_device_in_use(&created.system_name).unwrap_or(false);
        if !in_use {
            cleanup(&mut engine, &mut player);
            panic!("test setup failed: device should show as in-use before effects are applied");
        }

        let config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 6,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        if let Err(error) = engine.apply_effect_chain_structural(&device_id, &config) {
            cleanup(&mut engine, &mut player);
            panic!("structural apply should succeed even while the device is in use: {error}");
        }

        std::thread::sleep(Duration::from_millis(300));
        let still_in_use = pactl::virtual_device_in_use(&created.system_name).unwrap_or(false);
        if !still_in_use {
            cleanup(&mut engine, &mut player);
            panic!("the held sink-input should have been moved back onto the effects-hosted sink");
        }

        // Live param update: change the gain without any restart, verify via
        // pw-cli enum-params that the running node's control value actually
        // changed (not just that the command didn't error).
        let updated_config = EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: -4,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        if let Err(error) = engine.set_effect_chain_live_params(&device_id, &updated_config) {
            cleanup(&mut engine, &mut player);
            panic!("live param update failed: {error}");
        }

        let node_id = crate::pipewire::pw_cli::find_node_id_by_name(&created.system_name).ok().flatten();
        let live_value = node_id.and_then(|id| {
            let output = std::process::Command::new("pw-cli")
                .args(["enum-params", &id.to_string(), "Props"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            let idx = text.find("eq_bass:Gain")?;
            let after = &text[idx..];
            let value_line = after.lines().nth(1)?.trim();
            value_line.strip_prefix("Float ")?.parse::<f64>().ok()
        });

        cleanup(&mut engine, &mut player);

        assert_eq!(
            live_value,
            Some(-4.0),
            "expected the live-updated eq_bass:Gain to read back as -4.0"
        );
    }
}
