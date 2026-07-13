use crate::config::ConfigStore;
use crate::core::models::{ApplyResult, DeviceDirection, DeviceKind, EffectChainConfig};
use crate::pipewire::fx_capability::{self, FxCapabilities};
use crate::pipewire::fx_validate::{self, PreflightResult};
use crate::pipewire::{filter_chain, pactl, pipewire_restart, pw_link};
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

        if pactl::virtual_device_in_use(&device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?
        {
            return Err(EngineError::InvalidInput(format!(
                "{} is currently carrying audio — stop or move apps off it before applying effects",
                device.label
            )));
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

        let apply_result = self.try_apply_structural(&device, &conf_path, &rendered, &downstream_targets);

        if let Err(error) = apply_result {
            let _ = fs::remove_file(&conf_path);
            let _ = pipewire_restart::restart_filter_chain_service();
            let _ = pactl::create_null_sink(&device.system_name, &device.label);
            let _ = crate::core::routing::apply_sink_targets(&self.graph, &device.id, &downstream_target_ids);
            let _ = self.refresh_graph();
            return Err(EngineError::Adapter(format!(
                "effects apply failed and was rolled back to no effects: {error}"
            )));
        }

        ConfigStore::new()
            .set_effect_chain(device_id, config)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: Some(format!("Effects applied to {}", device.label)),
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

        fs::remove_file(&conf_path)
            .map_err(|error| EngineError::Adapter(format!("failed to remove effects config: {error}")))?;
        pipewire_restart::restart_filter_chain_service()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        pactl::create_null_sink(&device.system_name, &device.label)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

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
        let _ = filter_chain::sync_all_effects(&active, &[])
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    fn active_effect_chains(
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
}
