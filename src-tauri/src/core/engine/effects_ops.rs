use crate::config::ConfigStore;
use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, EffectChainConfig};
use crate::pipewire::fx_capability::{self, FxCapabilities};
use crate::pipewire::fx_validate::{self, PreflightResult};
use crate::pipewire::{filter_chain, pipewire_restart, pw_cli};
use crate::backend::linux::{pactl, pw_link};
use std::collections::HashMap;
use std::fs;
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
        fx_capability::probe_capabilities()
    }

    /// Validates a candidate chain against the v1 safety contract without
    /// writing anything or touching PipeWire — safe to call on every slider
    /// change so the UI can show blocking reasons before the user ever hits
    /// Apply.
    pub fn preflight_effect_chain(&self, config: &EffectChainConfig) -> PreflightResult {
        let capabilities = fx_capability::probe_capabilities();
        fx_validate::preflight(config, &capabilities)
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
        filter_chain::conf_path_for_device(&device.system_name)
            .is_some_and(|path| path.is_file())
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

        if !filter_chain::is_pipe_deck_device(&device.system_name) {
            return Err(EngineError::Adapter(
                "effects may only be applied to pipe-deck virtual devices".into(),
            ));
        }

        let store = ConfigStore::new();
        if config.is_active() {
            store
                .set_effect_chain(device_id, &config)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        } else {
            store
                .remove_effect_chain(device_id)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        if self.graph.data_source == "mock" {
            return Ok(ApplyResult {
                success: true,
                message: None,
            });
        }

        let chains = store
            .effect_chains()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let active = self.active_effect_chains(&chains);
        let deactivated = if config.is_active() {
            Vec::new()
        } else {
            vec![device.system_name.clone()]
        };

        filter_chain::sync_all_effects(&active, &deactivated)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Structural Apply: the rare, explicit, restart-carrying path — writes a
    /// namespaced filter-chain conf.d drop-in, restarts *only*
    /// `filter-chain.service` (a dedicated daemon, never the main PipeWire
    /// graph — see `pipewire::pipewire_restart`), verifies the effects sink
    /// actually reappeared, and re-links whatever the device was already
    /// routed to. Any failure automatically rolls back to the plain sink so
    /// the device is never left missing or broken.
    ///
    /// Scope for this pass: EQ + master gain only, on `pipe-deck-*` virtual
    /// **output** devices not currently carrying audio. Dynamics stages are
    /// rejected by `fx_validate::preflight` unless a real backing plugin is
    /// confirmed present (currently: none are, on any known PipeWire version
    /// for limiter/compressor, and only if a LADSPA plugin is installed for
    /// noise gate).
    pub fn apply_effect_chain_structural(
        &mut self,
        device_id: &str,
        config: &EffectChainConfig,
    ) -> Result<ApplyResult, EngineError> {
        if self.graph.data_source == "mock" {
            return Ok(ApplyResult {
                success: true,
                message: Some("effects applied (mock)".to_string()),
            });
        }

        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        if !filter_chain::is_pipe_deck_device(&device.system_name) {
            return Err(EngineError::InvalidInput(
                "effects may only be applied to pipe-deck virtual devices".to_string(),
            ));
        }
        if device.kind != DeviceKind::Virtual || device.direction != DeviceDirection::Output {
            return Err(EngineError::InvalidInput(
                "live effects currently only support virtual output devices".to_string(),
            ));
        }

        let capabilities = fx_capability::probe_capabilities();
        let preflight = fx_validate::preflight(config, &capabilities);
        if !preflight.ok {
            return Err(EngineError::InvalidInput(preflight.blocking_reasons.join("; ")));
        }

        let conf_path = filter_chain::conf_path_for_device(&device.system_name)
            .ok_or_else(|| EngineError::Adapter("could not resolve HOME for effects config".to_string()))?;
        let rendered = fx_validate::render_conf(&device.system_name, config);

        if conf_path.is_file() {
            if let Ok(existing) = fs::read_to_string(&conf_path) {
                if existing == rendered {
                    return Ok(ApplyResult {
                        success: true,
                        message: Some("no change".to_string()),
                    });
                }
            }
        }

        let downstream_target_ids = if device.current_targets.is_empty() {
            device.current_target.clone().into_iter().collect::<Vec<_>>()
        } else {
            device.current_targets.clone()
        };
        let downstream_targets: Vec<_> = downstream_target_ids
            .iter()
            .filter_map(|id| self.graph.devices.iter().find(|d| &d.id == id).cloned())
            .collect();

        // The device may currently be carrying audio (apps actively playing
        // into it). Rather than refusing to apply effects at all, briefly
        // hold those streams on a scratch sink for the swap and move them
        // back once the effects-hosted sink is confirmed up — a short
        // glitch on the affected streams instead of a hard block.
        let held_sink_inputs = pactl::sink_input_indices_on(&device.system_name);
        if !held_sink_inputs.is_empty() {
            pactl::ensure_holding_sink().map_err(|error| EngineError::Adapter(error.to_string()))?;
            for index in &held_sink_inputs {
                let _ = pactl::move_sink_input_to_sink_name(*index, pactl::HOLDING_SINK_NAME);
            }
        }

        let apply_result = self.try_apply_structural(&device, &conf_path, &rendered, &downstream_targets);

        if let Err(error) = apply_result {
            let _ = fs::remove_file(&conf_path);
            let _ = pipewire_restart::restart_filter_chain_service();
            let _ = pactl::create_null_sink(&device.system_name, &device.label);
            let _ = crate::core::routing::apply_sink_targets(&self.graph, &device.id, &downstream_target_ids);
            for index in &held_sink_inputs {
                let _ = pactl::move_sink_input_to_sink_name(*index, &device.system_name);
            }
            let _ = pactl::remove_holding_sink();
            let _ = self.refresh_graph();
            return Err(EngineError::Adapter(format!(
                "effects apply failed and was rolled back to no effects: {error}"
            )));
        }

        for index in &held_sink_inputs {
            let _ = pactl::move_sink_input_to_sink_name(*index, &device.system_name);
        }
        let _ = pactl::remove_holding_sink();

        ConfigStore::new()
            .set_effect_chain(device_id, config)
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

        let capabilities = fx_capability::probe_capabilities();
        let preflight = fx_validate::preflight(config, &capabilities);
        if !preflight.ok {
            return Ok(ApplyResult {
                success: false,
                message: Some(preflight.blocking_reasons.join("; ")),
            });
        }

        let Some(node_id) = pw_cli::find_node_id_by_name(&device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?
        else {
            return Ok(ApplyResult {
                success: false,
                message: Some("Live effects aren't enabled yet for this device".to_string()),
            });
        };

        pw_cli::set_params(node_id, &fx_validate::live_params(config))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        ConfigStore::new()
            .set_effect_chain(device_id, config)
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
        if self.graph.data_source == "mock" {
            return Ok(ApplyResult {
                success: true,
                message: Some("effects removed (mock)".to_string()),
            });
        }

        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {device_id}")))?;

        let conf_path = filter_chain::conf_path_for_device(&device.system_name)
            .ok_or_else(|| EngineError::Adapter("could not resolve HOME for effects config".to_string()))?;

        if !conf_path.is_file() {
            return Ok(ApplyResult {
                success: true,
                message: Some("no live effects to remove".to_string()),
            });
        }

        let downstream_target_ids = if device.current_targets.is_empty() {
            device.current_target.clone().into_iter().collect::<Vec<_>>()
        } else {
            device.current_targets.clone()
        };

        let held_sink_inputs = pactl::sink_input_indices_on(&device.system_name);
        if !held_sink_inputs.is_empty() {
            pactl::ensure_holding_sink().map_err(|error| EngineError::Adapter(error.to_string()))?;
            for index in &held_sink_inputs {
                let _ = pactl::move_sink_input_to_sink_name(*index, pactl::HOLDING_SINK_NAME);
            }
        }

        fs::remove_file(&conf_path)
            .map_err(|error| EngineError::Adapter(format!("failed to remove effects config: {error}")))?;
        pipewire_restart::restart_filter_chain_service()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        pactl::create_null_sink(&device.system_name, &device.label)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        for index in &held_sink_inputs {
            let _ = pactl::move_sink_input_to_sink_name(*index, &device.system_name);
        }
        let _ = pactl::remove_holding_sink();

        self.refresh_graph()?;
        crate::core::routing::apply_sink_targets(&self.graph, &device.id, &downstream_target_ids)
            .map_err(|error| EngineError::Routing(error.to_string()))?;

        ConfigStore::new()
            .remove_effect_chain(device_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: Some(format!("Effects removed from {}", device.label)),
        })
    }

    fn try_apply_structural(
        &self,
        device: &crate::core::models::Device,
        conf_path: &std::path::Path,
        rendered: &str,
        downstream_targets: &[crate::core::models::Device],
    ) -> Result<(), EngineError> {
        if let Some(dir) = filter_chain::filter_chain_conf_dir() {
            fs::create_dir_all(&dir)
                .map_err(|error| EngineError::Adapter(format!("failed to create effects config dir: {error}")))?;
        }

        if let Some(module_id) = pactl::find_module_id_by_sink_name(&device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?
        {
            pactl::unload_module(&module_id).map_err(|error| EngineError::Adapter(error.to_string()))?;
        }

        fs::write(conf_path, rendered)
            .map_err(|error| EngineError::Adapter(format!("failed to write effects config: {error}")))?;

        pipewire_restart::restart_filter_chain_service()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        filter_chain::wait_for_sink(&device.system_name, Duration::from_secs(5))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        filter_chain::wait_for_effect_output_ports(&device.system_name, Duration::from_secs(5))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        let effect_output_name = filter_chain::effect_output_name_for_device(&device.system_name);
        for target in downstream_targets {
            let is_virtual_input = target.kind == DeviceKind::Virtual && target.direction == DeviceDirection::Input;
            let result = if is_virtual_input {
                pw_link::link_capture_source_to_virtual_input(&effect_output_name, &target.system_name)
            } else {
                pw_link::link_capture_source_to_sink(&effect_output_name, &target.system_name)
            };
            result.map_err(|error| EngineError::Adapter(format!(
                "effects sink came up but could not be re-linked to {}: {error}",
                target.label
            )))?;
        }

        Ok(())
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
        let _ = filter_chain::sync_all_effects(&active, &[])
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    /// Re-establishes live processing (Structural Apply) for any device that
    /// already has a live conf file on disk from before — that file is the
    /// signal the user had previously confirmed "Enable live effects" for
    /// this device, so silently restoring it on app-boot/profile-swap restore
    /// isn't turning on live processing that was never explicitly approved
    /// (PD-017 §1). A chain that's configured but was never applied stays
    /// persist-only, same as before this existed. `apply_effect_chain_structural`
    /// is already idempotent (no-ops without a restart if the rendered conf
    /// is unchanged), so this is safe to call on every restore/swap. Each
    /// device is independent — one failing must not block the rest of
    /// restore, per #20's acceptance criteria.
    pub(super) fn reapply_previously_live_effect_chains(&mut self, active: &[(String, EffectChainConfig)]) {
        for (system_name, config) in active {
            let was_live = filter_chain::conf_path_for_device(system_name)
                .is_some_and(|path| path.is_file());
            if !was_live {
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
                if !filter_chain::is_pipe_deck_device(&system_name) {
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
            eq_bass: 6,
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
            eq_bass: 6,
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
            eq_bass: -4,
            ..Default::default()
        };
        if let Err(error) = engine.set_effect_chain_live_params(&device_id, &updated_config) {
            cleanup(&mut engine, &mut player);
            panic!("live param update failed: {error}");
        }

        let node_id = pw_cli::find_node_id_by_name(&created.system_name).ok().flatten();
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
