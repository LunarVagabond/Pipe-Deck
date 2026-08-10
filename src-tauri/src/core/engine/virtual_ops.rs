use crate::config::ConfigStore;
use crate::core::models::{DeviceDirection, RuntimeGraph, VirtualDeviceResult};
use crate::core::restore::spec_from_create_result;
use std::collections::{HashMap, HashSet};

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Persists a device alias and, for Pipe Deck-owned virtual devices,
    /// syncs the feed sink and pactl module description to match. Moved
    /// here from the `set_device_alias` command handler, which used to call
    /// `backend::linux::pactl` directly instead of going through the engine.
    pub fn apply_device_alias(
        &mut self,
        system_name: &str,
        alias: &str,
    ) -> Result<(), EngineError> {
        ConfigStore::new()
            .set_device_alias(system_name, alias)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        if system_name.starts_with("pipe-deck-") && !system_name.starts_with("pipe-deck-feed-") {
            let _ = self.adapter.set_virtual_device_alias(system_name, alias);
        }

        Ok(())
    }

    pub fn create_virtual_output(
        &mut self,
        name: &str,
    ) -> Result<VirtualDeviceResult, EngineError> {
        self.create_virtual_output_with_mode(name, false)
    }

    pub fn create_virtual_multi_output(
        &mut self,
        name: &str,
    ) -> Result<VirtualDeviceResult, EngineError> {
        self.create_virtual_output_with_mode(name, true)
    }

    fn create_virtual_output_with_mode(
        &mut self,
        name: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, EngineError> {
        let result = self
            .adapter
            .create_virtual_output(name, multi)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .add_virtual_device(spec_from_create_result(
                    &result.device_id,
                    &result.system_name,
                    &result.label,
                    DeviceDirection::Output,
                    multi,
                ))
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn create_virtual_input(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
        let result = self
            .adapter
            .create_virtual_input(name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .add_virtual_device(spec_from_create_result(
                    &result.device_id,
                    &result.system_name,
                    &result.label,
                    DeviceDirection::Input,
                    false,
                ))
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn remove_virtual_device(&mut self, system_name: &str) -> Result<(), EngineError> {
        let device_id = self
            .graph
            .devices
            .iter()
            .find(|device| device.system_name == system_name)
            .map(|device| device.id.clone());

        if self.graph.data_source != "mock" {
            // A deleted device's live effects conf (if any) must go with it —
            // otherwise it's an orphan that `filter-chain.service` will keep
            // recreating a same-named ghost sink for on every future restart,
            // long after the device it belonged to is gone. Best-effort: the
            // device is about to be destroyed regardless, so a failed conf
            // cleanup here shouldn't block that.
            let _ = self.discard_effect_chain_conf(system_name);
            if let Some(device_id) = &device_id {
                let _ = ConfigStore::new().remove_effect_chain(device_id);
            }
        }

        // Streams still routed straight to this device would otherwise pause
        // outright (issue #208) once the module backing it disappears out from
        // under them — move them to a fallback target first. Reuses the same
        // adapter-level fallback resolution `clear_stream_target` already applies
        // when a route is cleared (default sink, else first physical output).
        if let Some(device_id) = &device_id {
            let stranded_stream_ids: Vec<String> = self
                .graph
                .streams
                .iter()
                .filter(|stream| stream.current_target.as_deref() == Some(device_id.as_str()))
                .map(|stream| stream.id.clone())
                .collect();
            for stream_id in stranded_stream_ids {
                let _ = self.adapter.clear_stream_target(
                    &self.graph,
                    &stream_id,
                    Some(device_id.as_str()),
                );
            }
        }

        self.adapter
            .remove_virtual_device(system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            let _ = self.adapter.disconnect_all_virtual_mic_mixes(system_name);
            ConfigStore::new()
                .remove_virtual_device(system_name)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }
        self.refresh_graph()?;
        Ok(())
    }
}

pub(super) fn merge_virtual_devices(
    graph: &mut RuntimeGraph,
    device_id_remap: &mut HashMap<String, String>,
    adapter: &dyn crate::backend::AudioBackend,
) {
    let multi_by_name: HashMap<String, bool> = ConfigStore::new()
        .virtual_devices()
        .into_iter()
        .map(|spec| (format!("pipe-deck-{}", spec.slug), spec.multi))
        .collect();

    let mut id_remap = HashMap::new();

    for entry in adapter.list_virtual_devices() {
        let sink_mode = if entry.direction == crate::core::models::DeviceDirection::Output {
            let multi = multi_by_name
                .get(&entry.system_name)
                .copied()
                .unwrap_or(entry.multi);
            Some(if multi {
                crate::core::models::SinkMode::Multi
            } else {
                crate::core::models::SinkMode::Single
            })
        } else {
            None
        };

        if let Some(device) = graph
            .devices
            .iter_mut()
            .find(|device| device.system_name == entry.system_name)
        {
            if device.id != entry.device_id {
                id_remap.insert(device.id.clone(), entry.device_id.clone());
            }
            device.id = entry.device_id.clone();
            device.label = entry.label.clone();
            device.kind = crate::core::models::DeviceKind::Virtual;
            device.direction = entry.direction.clone();
            device.sink_mode = sink_mode;
            if device.volume_percent.is_none() {
                device.volume_percent = Some(100);
            }
            if device.muted.is_none() {
                device.muted = Some(false);
            }
        } else {
            let mut device = entry.to_device();
            device.sink_mode = sink_mode;
            graph.devices.push(device);
        }
    }

    adapter.apply_device_aliases_and_levels(&mut graph.devices);

    for (old_id, new_id) in id_remap {
        device_id_remap.insert(old_id.clone(), new_id.clone());

        for stream in &mut graph.streams {
            if stream.current_target.as_deref() == Some(old_id.as_str()) {
                stream.current_target = Some(new_id.clone());
            }
        }

        for device in &mut graph.devices {
            if device.current_target.as_deref() == Some(old_id.as_str()) {
                device.current_target = Some(new_id.clone());
            }
        }

        for link in &mut graph.links {
            if link.source_id == old_id {
                link.source_id = new_id.clone();
            }
            if link.target_id == old_id {
                link.target_id = new_id.clone();
            }
        }
    }

    let mut seen_links = HashSet::new();
    graph
        .links
        .retain(|link| seen_links.insert((link.source_id.clone(), link.target_id.clone())));
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as
    //! `effects_ops::live_tests`. Creates and tears down its own disposable
    //! virtual device.
    use super::*;

    #[test]
    #[ignore]
    fn removing_a_device_with_live_effects_unloads_its_native_chain() {
        // Regression test (native-transport equivalent of the old
        // conf.d-orphan regression from before #149): `remove_virtual_device`
        // must unload a device's live effect chain, not just delete the
        // device — otherwise the native host keeps hosting a chain for a
        // system_name nothing in the UI knows about anymore.
        //
        // Uses a virtual *input* (mic) device, not output — device-attached
        // output effects (the old Bus mechanism) were retired alongside
        // `VirtualRole::Bus` (#287); `apply_effect_chain_structural` now
        // rejects anything but a virtual input device (see
        // `effects_ops.rs`'s `is_pipe_deck_device`/kind check). This test
        // used to target a virtual output and would panic on that check
        // before ever reaching `remove_virtual_device` — since a bare
        // `.expect()` panic skips whatever cleanup follows it, that left a
        // real orphaned "Pipe Deck Orphan Conf Test" sink in the live
        // session on every run. The `cleanup` closure below is the actual
        // fix: called from every fallible step so a future regression here
        // can't orphan a device again either.
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_input("Pipe Deck Orphan Conf Test")
            .expect("create disposable test device");

        let cleanup = |engine: &mut CoreEngine| {
            let _ = engine.remove_virtual_device(&created.system_name);
        };

        let config = crate::core::models::EffectChainConfig {
            stages: vec![crate::core::models::EffectStage::Eq5Band {
                id: "eq".to_string(),
                eq_bass: 5,
                eq_sub: 0,
                eq_mid: 0,
                eq_treble: 0,
                eq_air: 0,
                output_gain: 0,
            }],
            ..Default::default()
        };
        if let Err(error) = engine.apply_effect_chain_structural(&created.device_id, &config) {
            cleanup(&mut engine);
            panic!("structural apply should succeed: {error}");
        }

        if !engine.is_effect_chain_live(&created.device_id) {
            cleanup(&mut engine);
            panic!("chain should be live right after apply");
        }

        let system_name = created.system_name.clone();
        if let Err(error) = engine.remove_virtual_device(&system_name) {
            panic!("remove_virtual_device should succeed: {error}");
        }

        assert!(
            !crate::daemon::ipc::client::NativeHostClient::is_loaded(&system_name),
            "native chain should be unloaded along with the device, not left as an orphan"
        );
    }
}
