use crate::config::ConfigStore;
use crate::core::models::{DeviceDirection, RuntimeGraph, VirtualDeviceResult};
use crate::core::restore::spec_from_create_result;
use crate::pipewire::virtual_devices::VirtualDeviceRegistry;
use std::collections::{HashMap, HashSet};

use super::{CoreEngine, EngineError};

impl CoreEngine {
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
        if self.graph.data_source == "mock" {
            let slug = name.to_lowercase().replace(' ', "-");
            let system_name = format!("pipe-deck-{slug}");
            self.graph.devices.push(crate::core::models::Device {
                id: format!("virtual-{slug}"),
                system_name: system_name.clone(),
                label: name.to_string(),
                kind: crate::core::models::DeviceKind::Virtual,
                direction: crate::core::models::DeviceDirection::Output,
                sink_mode: Some(if multi {
                    crate::core::models::SinkMode::Multi
                } else {
                    crate::core::models::SinkMode::Single
                }),
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
                multi,
            });
        }

        let result = if multi {
            self.virtual_registry
                .create_multi_output(name)
                .map_err(|error| EngineError::Adapter(error.to_string()))?
        } else {
            self.virtual_registry
                .create_output(name)
                .map_err(|error| EngineError::Adapter(error.to_string()))?
        };
        ConfigStore::new()
            .add_virtual_device(spec_from_create_result(
                &result.device_id,
                &result.system_name,
                &result.label,
                DeviceDirection::Output,
                multi,
            ))
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn create_virtual_input(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
        if self.graph.data_source == "mock" {
            let slug = name.to_lowercase().replace(' ', "-");
            let system_name = format!("pipe-deck-{slug}");
            self.graph.devices.push(crate::core::models::Device {
                id: format!("virtual-{slug}"),
                system_name: system_name.clone(),
                label: name.to_string(),
                kind: crate::core::models::DeviceKind::Virtual,
                direction: crate::core::models::DeviceDirection::Input,
                sink_mode: None,
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
                current_targets: Vec::new(),
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
                multi: false,
            });
        }

        let result = self
            .virtual_registry
            .create_input(name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        ConfigStore::new()
            .add_virtual_device(spec_from_create_result(
                &result.device_id,
                &result.system_name,
                &result.label,
                DeviceDirection::Input,
                false,
            ))
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(result)
    }

    pub fn remove_virtual_device(&mut self, system_name: &str) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            self.graph
                .devices
                .retain(|device| device.system_name != system_name);
            return Ok(());
        }

        self.virtual_registry
            .remove_device(system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        ConfigStore::new()
            .remove_virtual_device(system_name)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }
}

pub(super) fn merge_virtual_devices(
    graph: &mut RuntimeGraph,
    registry: &VirtualDeviceRegistry,
    device_id_remap: &mut HashMap<String, String>,
) {
    let multi_by_name: HashMap<String, bool> = ConfigStore::new()
        .virtual_devices()
        .into_iter()
        .map(|spec| (format!("pipe-deck-{}", spec.slug), spec.multi))
        .collect();

    let mut id_remap = HashMap::new();

    for entry in registry.list_devices() {
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

    crate::pipewire::live::apply_device_aliases(&mut graph.devices);
    crate::pipewire::live::apply_device_levels(&mut graph.devices);

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
