use crate::config::ConfigStore;
use crate::core::models::{
    ApplyResult, DeviceDirection, DeviceKind, MixSource, MixSourceSpec, RuntimeGraph,
    VirtualDeviceResult,
};
use crate::core::restore::spec_from_create_result;
use std::collections::{HashMap, HashSet};

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Persists a device alias and, for Pipe Deck-owned virtual devices,
    /// syncs the feed sink and pactl module description to match. Moved
    /// here from the `set_device_alias` command handler, which used to call
    /// `backend::linux::pactl` directly instead of going through the engine.
    pub fn apply_device_alias(&mut self, system_name: &str, alias: &str) -> Result<(), EngineError> {
        ConfigStore::new()
            .set_device_alias(system_name, alias)
            .map_err(|error| EngineError::Config(error.to_string()))?;

        if system_name.starts_with("pipe-deck-") && !system_name.starts_with("pipe-deck-feed-") {
            let _ = self.adapter.set_virtual_device_alias(system_name, alias);
        }

        Ok(())
    }

    pub fn create_virtual_output(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
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
        if self.graph.data_source != "mock" {
            // A deleted device's live effects conf (if any) must go with it —
            // otherwise it's an orphan that `filter-chain.service` will keep
            // recreating a same-named ghost sink for on every future restart,
            // long after the device it belonged to is gone. Best-effort: the
            // device is about to be destroyed regardless, so a failed conf
            // cleanup here shouldn't block that.
            let _ = self.discard_effect_chain_conf(system_name);
            if let Some(device_id) = self
                .graph
                .devices
                .iter()
                .find(|device| device.system_name == system_name)
                .map(|device| device.id.clone())
            {
                let _ = ConfigStore::new().remove_effect_chain(&device_id);
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

    pub fn set_virtual_mic_mix(
        &mut self,
        virtual_mic_device_id: &str,
        mix_sources: &[MixSource],
    ) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .cloned()
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;

        if virtual_mic.kind != DeviceKind::Virtual || virtual_mic.direction == DeviceDirection::Duplex
        {
            return Err(EngineError::InvalidInput(
                "target must be a virtual input or virtual output".to_string(),
            ));
        }

        let mut mix_source_specs = Vec::new();
        for mix_source in mix_sources {
            let source = self
                .graph
                .devices
                .iter()
                .find(|device| device.id == mix_source.device_id)
                .ok_or_else(|| {
                    EngineError::NotFound(format!("device not found: {}", mix_source.device_id))
                })?;

            let is_physical_mic = source.kind == DeviceKind::Physical && source.direction == DeviceDirection::Input;
            let is_virtual_output = source.kind == DeviceKind::Virtual && source.direction == DeviceDirection::Output;
            if !is_physical_mic && !is_virtual_output {
                return Err(EngineError::InvalidInput(format!(
                    "{} is not a physical input or virtual output",
                    source.label
                )));
            }

            mix_source_specs.push(MixSourceSpec {
                system_name: source.system_name.clone(),
                volume_percent: mix_source.volume_percent,
                muted: mix_source.muted,
            });
        }

        self.adapter
            .apply_virtual_mic_mix(&virtual_mic, &mix_source_specs)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            ConfigStore::new()
                .set_virtual_mic_mix_sources(&virtual_mic.system_name, &mix_source_specs)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: Some(format!(
                "Mixed {} source(s) into {}",
                mix_source_specs.len(),
                virtual_mic.label
            )),
        })
    }

    /// Adds one source to a mic's mix, computing the resulting full list from
    /// this engine's own (server-authoritative) graph rather than trusting a
    /// frontend-supplied "existing + new" snapshot. `set_virtual_mic_mix`
    /// replaces the whole mix wholesale, so if the frontend's copy of
    /// `mix_sources` were even slightly stale — e.g. a second drag fired
    /// before the graph update from the first one arrived — it would silently
    /// drop whatever it didn't know about. Reading fresh here closes that
    /// race entirely.
    pub fn add_mix_source(
        &mut self,
        virtual_mic_device_id: &str,
        source_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;

        let mut updated_sources = virtual_mic.mix_sources.clone();
        if updated_sources.iter().any(|source| source.device_id == source_device_id) {
            return Ok(ApplyResult {
                success: false,
                message: Some("This device is already mixed into this device.".to_string()),
            });
        }
        updated_sources.push(MixSource {
            device_id: source_device_id.to_string(),
            volume_percent: 100,
            muted: false,
        });

        self.set_virtual_mic_mix(virtual_mic_device_id, &updated_sources)
    }

    /// Removes one source from a mic's mix; same server-authoritative-list
    /// reasoning as `add_mix_source`.
    pub fn remove_mix_source(
        &mut self,
        virtual_mic_device_id: &str,
        source_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;

        let updated_sources: Vec<MixSource> = virtual_mic
            .mix_sources
            .iter()
            .filter(|source| source.device_id != source_device_id)
            .cloned()
            .collect();

        self.set_virtual_mic_mix(virtual_mic_device_id, &updated_sources)
    }

    /// Live gain adjustment for one already-mixed source — no relinking, so
    /// this is safe to call at high frequency for a slider drag.
    pub fn set_mix_source_volume(
        &mut self,
        virtual_mic_device_id: &str,
        source_device_id: &str,
        percent: u8,
    ) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;
        let source = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == source_device_id)
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {source_device_id}")))?;

        self.adapter
            .set_mix_source_volume(&virtual_mic.system_name, &source.system_name, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            let virtual_mic_system_name = virtual_mic.system_name.clone();
            let source_system_name = source.system_name.clone();
            ConfigStore::new()
                .update_mix_source_volume(&virtual_mic_system_name, &source_system_name, percent)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    /// Mutes/unmutes one already-mixed source without touching its link — the
    /// feed sink and its port connections stay exactly as they are.
    pub fn set_mix_source_mute(
        &mut self,
        virtual_mic_device_id: &str,
        source_device_id: &str,
        muted: bool,
    ) -> Result<ApplyResult, EngineError> {
        let virtual_mic = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == virtual_mic_device_id)
            .ok_or_else(|| EngineError::NotFound("virtual mic not found".to_string()))?;
        let source = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == source_device_id)
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {source_device_id}")))?;

        self.adapter
            .set_mix_source_mute(&virtual_mic.system_name, &source.system_name, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        if self.graph.data_source != "mock" {
            let virtual_mic_system_name = virtual_mic.system_name.clone();
            let source_system_name = source.system_name.clone();
            ConfigStore::new()
                .update_mix_source_mute(&virtual_mic_system_name, &source_system_name, muted)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
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
    graph.links.retain(|link| seen_links.insert((link.source_id.clone(), link.target_id.clone())));
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as
    //! `effects_ops::live_tests`. Creates and tears down its own disposable
    //! virtual mic; only *reads* the real physical mic, never mutates it.
    use super::*;

    #[test]
    #[ignore]
    fn mixes_a_real_physical_mic_into_a_disposable_virtual_mic() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let physical_mic = engine
            .runtime_graph()
            .devices
            .iter()
            .find(|device| device.kind == DeviceKind::Physical && device.direction == DeviceDirection::Input)
            .cloned();
        let Some(physical_mic) = physical_mic else {
            panic!("no physical input device found on this system to test with");
        };

        let created = engine
            .create_virtual_input("Pipe Deck Live Mix Test")
            .expect("create disposable virtual mic");

        let cleanup = |engine: &mut CoreEngine| {
            let _ = engine.remove_virtual_device(&created.system_name);
        };

        let mix_sources = vec![MixSource {
            device_id: physical_mic.id.clone(),
            volume_percent: 80,
            muted: false,
        }];

        let result = engine.set_virtual_mic_mix(&created.device_id, &mix_sources);
        if let Err(error) = &result {
            cleanup(&mut engine);
            panic!("set_virtual_mic_mix failed: {error}");
        }

        engine.refresh_graph().expect("refresh after mix");
        let mic_after = engine
            .runtime_graph()
            .devices
            .iter()
            .find(|device| device.system_name == created.system_name)
            .cloned();

        cleanup(&mut engine);

        let mic_after = mic_after.expect("virtual mic should still exist in graph");
        assert_eq!(mic_after.mix_sources.len(), 1, "expected exactly one mix source to be discovered");
        assert_eq!(mic_after.mix_sources[0].device_id, physical_mic.id);
    }

    #[test]
    #[ignore]
    fn removing_a_device_with_live_effects_unloads_its_native_chain() {
        // Regression test (native-transport equivalent of the old
        // conf.d-orphan regression from before #149): `remove_virtual_device`
        // must unload a device's live effect chain, not just delete the
        // device — otherwise the native host keeps hosting a chain for a
        // system_name nothing in the UI knows about anymore.
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_output("Pipe Deck Orphan Conf Test")
            .expect("create disposable test device");

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
        engine
            .apply_effect_chain_structural(&created.device_id, &config)
            .expect("structural apply should succeed");

        assert!(
            engine.is_effect_chain_live(&created.device_id),
            "chain should be live right after apply"
        );

        let system_name = created.system_name.clone();
        engine
            .remove_virtual_device(&system_name)
            .expect("remove_virtual_device should succeed");

        assert!(
            !crate::daemon::ipc::client::NativeHostClient::is_loaded(&system_name),
            "native chain should be unloaded along with the device, not left as an orphan"
        );
    }
}
