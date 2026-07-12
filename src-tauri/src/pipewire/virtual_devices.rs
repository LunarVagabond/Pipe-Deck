use crate::core::models::{Device, DeviceDirection, DeviceKind, SinkMode, VirtualDeviceResult};
use crate::pipewire::adapter::AdapterError;
use crate::pipewire::pactl;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct VirtualDeviceEntry {
    pub module_id: String,
    pub device_id: String,
    pub system_name: String,
    pub label: String,
    pub direction: DeviceDirection,
    pub multi: bool,
}

#[derive(Default)]
pub struct VirtualDeviceRegistry {
    devices: Mutex<HashMap<String, VirtualDeviceEntry>>,
}

impl VirtualDeviceRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn discover_from_pactl(self: &Arc<Self>) -> Result<(), AdapterError> {
        let modules = pactl::list_pipe_deck_modules()?;
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| AdapterError::Message("virtual registry lock poisoned".into()))?;

        devices.retain(|name, _| !name.starts_with("pipe-deck-feed-"));

        for module in modules {
            if module.system_name.starts_with("pipe-deck-feed-") {
                continue;
            }
            devices
                .entry(module.system_name.clone())
                .or_insert_with(|| VirtualDeviceEntry {
                    module_id: module.module_id,
                    device_id: module.device_id,
                    system_name: module.system_name,
                    label: module.label,
                    direction: module.direction,
                    multi: module.multi,
                });
        }

        Ok(())
    }

    pub fn list_devices(&self) -> Vec<VirtualDeviceEntry> {
        self.devices
            .lock()
            .map(|devices| devices.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn set_label(&self, system_name: &str, label: &str) -> Result<(), AdapterError> {
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| AdapterError::Message("virtual registry lock poisoned".into()))?;
        let Some(entry) = devices.get_mut(system_name) else {
            return Ok(());
        };
        entry.label = label.to_string();
        Ok(())
    }

    pub fn get(&self, system_name: &str) -> Option<VirtualDeviceEntry> {
        self.devices
            .lock()
            .ok()
            .and_then(|devices| devices.get(system_name).cloned())
    }

    /// Updates the tracked module id after a virtual device's live node was
    /// unloaded and recreated (e.g. to change its description) — `remove_device`
    /// unloads by module id, so this must stay in sync with whatever module is
    /// currently backing `system_name`.
    pub fn set_module_id(&self, system_name: &str, module_id: &str) -> Result<(), AdapterError> {
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| AdapterError::Message("virtual registry lock poisoned".into()))?;
        let Some(entry) = devices.get_mut(system_name) else {
            return Ok(());
        };
        entry.module_id = module_id.to_string();
        Ok(())
    }

    pub fn create_output(self: &Arc<Self>, name: &str) -> Result<VirtualDeviceResult, AdapterError> {
        self.create_output_with_mode(name, false)
    }

    pub fn create_multi_output(self: &Arc<Self>, name: &str) -> Result<VirtualDeviceResult, AdapterError> {
        self.create_output_with_mode(name, true)
    }

    fn create_output_with_mode(
        self: &Arc<Self>,
        name: &str,
        multi: bool,
    ) -> Result<VirtualDeviceResult, AdapterError> {
        let slug = slugify(name);
        let system_name = format!("pipe-deck-{slug}");
        let module_id = pactl::create_null_sink(&system_name, name)?;
        let entry = VirtualDeviceEntry {
            module_id,
            device_id: format!("virtual-{slug}"),
            system_name: system_name.clone(),
            label: name.to_string(),
            direction: DeviceDirection::Output,
            multi,
        };
        self.insert_entry(entry.clone())?;
        Ok(entry.into_result())
    }

    pub fn create_input(self: &Arc<Self>, name: &str) -> Result<VirtualDeviceResult, AdapterError> {
        let slug = slugify(name);
        let system_name = format!("pipe-deck-{slug}");
        let module_id = pactl::create_virtual_source(&system_name, name)?;
        let entry = VirtualDeviceEntry {
            module_id,
            device_id: format!("virtual-{slug}"),
            system_name: system_name.clone(),
            label: name.to_string(),
            direction: DeviceDirection::Input,
            multi: false,
        };
        self.insert_entry(entry.clone())?;
        Ok(entry.into_result())
    }

    pub fn remove_device(self: &Arc<Self>, system_name: &str) -> Result<(), AdapterError> {
        let removed = {
            let mut devices = self
                .devices
                .lock()
                .map_err(|_| AdapterError::Message("virtual registry lock poisoned".into()))?;

            if let Some(entry) = devices.remove(system_name) {
                Some(entry)
            } else if let Some((key, entry)) = devices
                .iter()
                .find(|(_, entry)| entry.system_name == system_name)
                .map(|(key, entry)| (key.clone(), entry.clone()))
            {
                devices.remove(&key);
                Some(entry)
            } else {
                None
            }
        };

        if let Some(entry) = removed {
            if entry.direction == DeviceDirection::Input {
                let _ = pactl::remove_feed_sink_for_virtual_input(&entry.system_name);
            }
            let _ = pactl::unload_module(&entry.module_id);
            let _ = crate::pipewire::pw_link::disconnect_sink_monitor(&entry.system_name);
            return Ok(());
        }

        let sink_name = system_name.strip_suffix(".monitor").unwrap_or(system_name);
        if let Some(module_id) = pactl::find_module_id_by_sink_name(sink_name)? {
            pactl::unload_module(&module_id)?;
            let _ = crate::pipewire::pw_link::disconnect_sink_monitor(sink_name);
            return Ok(());
        }

        Err(AdapterError::Message(format!(
            "no tracked virtual device for {system_name}"
        )))
    }

    fn insert_entry(&self, entry: VirtualDeviceEntry) -> Result<(), AdapterError> {
        self.devices
            .lock()
            .map_err(|_| AdapterError::Message("virtual registry lock poisoned".into()))?
            .insert(entry.system_name.clone(), entry);
        Ok(())
    }
}

impl VirtualDeviceEntry {
    fn into_result(self) -> VirtualDeviceResult {
        VirtualDeviceResult {
            device_id: self.device_id,
            system_name: self.system_name,
            label: self.label,
            multi: self.multi,
        }
    }

    pub fn to_device(&self) -> Device {
        Device {
            id: self.device_id.clone(),
            system_name: self.system_name.clone(),
            label: self.label.clone(),
            kind: DeviceKind::Virtual,
            direction: self.direction.clone(),
            sink_mode: match self.direction {
                DeviceDirection::Output | DeviceDirection::Duplex => Some(if self.multi {
                    SinkMode::Multi
                } else {
                    SinkMode::Single
                }),
                DeviceDirection::Input => None,
            },
            volume_percent: Some(100),
            muted: Some(false),
            current_target: None,
            current_targets: Vec::new(),
            mix_source_ids: Vec::new(),
        }
    }
}

pub fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "device".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> VirtualDeviceEntry {
        VirtualDeviceEntry {
            module_id: "42".into(),
            device_id: "virtual-mic".into(),
            system_name: "pipe-deck-mic".into(),
            label: "Mic".into(),
            direction: DeviceDirection::Input,
            multi: false,
        }
    }

    #[test]
    fn set_module_id_updates_tracked_entry() {
        let registry = VirtualDeviceRegistry::new();
        registry.insert_entry(sample_entry()).unwrap();

        registry.set_module_id("pipe-deck-mic", "99").unwrap();

        let entry = registry.get("pipe-deck-mic").expect("entry should exist");
        assert_eq!(entry.module_id, "99");
    }

    #[test]
    fn set_module_id_on_unknown_device_is_a_no_op() {
        let registry = VirtualDeviceRegistry::new();

        assert!(registry.set_module_id("unknown", "99").is_ok());
        assert!(registry.get("unknown").is_none());
    }

    #[test]
    fn get_returns_cloned_entry() {
        let registry = VirtualDeviceRegistry::new();
        registry.insert_entry(sample_entry()).unwrap();

        let entry = registry.get("pipe-deck-mic").expect("entry should exist");
        assert_eq!(entry.label, "Mic");
        assert_eq!(entry.direction, DeviceDirection::Input);
    }
}
