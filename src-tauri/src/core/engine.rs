use crate::config::profile_store::{import_profile_archive, ProfileStore};
use crate::config::ConfigStore;
use crate::core::models::{
    ApplyResult, DeviceRouteIntent, Profile, ProfileIndexEntry, RoutingIntent, RuntimeGraph,
    VirtualDeviceResult,
};
use crate::core::profile::{capture_profile_from_graph, update_profile_timestamp};
use crate::core::routing::{
    apply_device_route_intent, apply_profile_routing, apply_profile_volumes, apply_routing_intent,
    capture_routing_snapshot, restore_routing_snapshot, RoutingSnapshot,
};
use crate::pipewire::adapter::PipeWireAdapter;
use crate::pipewire::pactl;
use crate::pipewire::virtual_devices::VirtualDeviceRegistry;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("pipewire adapter error: {0}")]
    Adapter(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("profile error: {0}")]
    Profile(String),
    #[error("routing error: {0}")]
    Routing(String),
}

pub struct CoreEngine {
    graph: RuntimeGraph,
    adapter: Box<dyn PipeWireAdapter>,
    rollback_stack: Vec<RoutingSnapshot>,
    virtual_registry: Arc<VirtualDeviceRegistry>,
    device_id_remap: HashMap<String, String>,
    last_error: Option<String>,
}

impl CoreEngine {
    pub fn new() -> Self {
        Self {
            graph: RuntimeGraph::default(),
            adapter: create_adapter(),
            rollback_stack: Vec::new(),
            virtual_registry: VirtualDeviceRegistry::new(),
            device_id_remap: HashMap::new(),
            last_error: None,
        }
    }

    pub fn runtime_graph(&self) -> &RuntimeGraph {
        &self.graph
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn can_undo_routing(&self) -> bool {
        !self.rollback_stack.is_empty()
    }

    pub fn clear_last_error(&mut self) {
        self.last_error = None;
    }

    pub async fn initialize(
        &mut self,
        app: &AppHandle,
        engine_ref: Arc<RwLock<CoreEngine>>,
    ) -> Result<(), EngineError> {
        ConfigStore::new()
            .ensure_layout()
            .map_err(|error| EngineError::Config(error.to_string()))?;

        if std::env::var("PIPE_DECK_USE_MOCK").as_deref() != Ok("1") {
            self.virtual_registry
                .discover_from_pactl()
                .map_err(|error| EngineError::Adapter(error.to_string()))?;
        }

        self.refresh_graph()?;
        self.emit_graph_update(app);

        let app_handle = app.clone();
        self.adapter
            .subscribe(Box::new(move |graph| {
                let app_handle = app_handle.clone();
                let engine_ref = engine_ref.clone();
                tauri::async_runtime::spawn(async move {
                    let mut engine = engine_ref.write().await;
                    engine.apply_graph_update(graph);
                    let _ = app_handle.emit("graph-updated", engine.runtime_graph().clone());
                });
            }))
            .map_err(|error| EngineError::Adapter(error.to_string()))?;

        Ok(())
    }

    pub fn refresh_graph(&mut self) -> Result<(), EngineError> {
        self.graph = self
            .adapter
            .fetch_graph()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        merge_virtual_devices(
            &mut self.graph,
            &self.virtual_registry,
            &mut self.device_id_remap,
        );
        if self.graph.data_source != "mock" {
            crate::pipewire::live::apply_graph_routing(&mut self.graph);
        }
        Ok(())
    }

    pub fn apply_graph_update(&mut self, graph: RuntimeGraph) {
        self.graph = graph;
        merge_virtual_devices(
            &mut self.graph,
            &self.virtual_registry,
            &mut self.device_id_remap,
        );
        if self.graph.data_source != "mock" {
            crate::pipewire::live::apply_graph_routing(&mut self.graph);
        }
    }

    fn resolve_device_id(&self, device_id: &str) -> String {
        self.device_id_remap
            .get(device_id)
            .cloned()
            .unwrap_or_else(|| device_id.to_string())
    }

    pub fn emit_graph_update(&self, app: &AppHandle) {
        let _ = app.emit("graph-updated", self.graph.clone());
    }

    pub fn get_profile(&self, profile_id: &str) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        profile_store
            .load_profile_by_id(profile_id, &config.profile_index)
            .map_err(|error| EngineError::Profile(error.to_string()))
    }

    pub fn save_profile(
        &mut self,
        profile_id: &str,
        name: Option<String>,
    ) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());

        let entry = config
            .profile_index
            .iter()
            .find(|entry| entry.id == profile_id)
            .cloned()
            .ok_or_else(|| EngineError::Profile(format!("profile not found: {profile_id}")))?;

        let display_name = name.unwrap_or_else(|| entry.name.clone());
        let mut profile = capture_profile_from_graph(&self.graph, profile_id, &display_name);

        if let Ok(existing) = profile_store.load_profile(&entry) {
            profile.created = existing.created;
        }
        update_profile_timestamp(&mut profile);

        profile_store
            .save_profile(&entry, &profile)
            .map_err(|error| EngineError::Profile(error.to_string()))?;

        if display_name != entry.name {
            let updated_entry = ProfileIndexEntry {
                id: entry.id,
                name: display_name.clone(),
                file: entry.file,
            };
            store
                .add_profile_to_index(updated_entry)
                .map_err(|error| EngineError::Config(error.to_string()))?;
        }

        profile.name = display_name;
        Ok(profile)
    }

    pub fn save_profile_as(
        &mut self,
        profile_id: &str,
        name: &str,
    ) -> Result<Profile, EngineError> {
        let store = ConfigStore::new();
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let profile = capture_profile_from_graph(&self.graph, profile_id, name);
        let entry = profile_store
            .save_profile_as(profile_id, name, &profile)
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(profile)
    }

    pub fn import_profile(&self, source_path: &str) -> Result<ProfileIndexEntry, EngineError> {
        let store = ConfigStore::new();
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let entry = profile_store
            .import_profile_file(Path::new(source_path))
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry.clone())
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(entry)
    }

    pub fn import_profile_archive(&self, source_path: &str) -> Result<ProfileIndexEntry, EngineError> {
        let store = ConfigStore::new();
        let profiles_dir = store.config_dir().join("profiles");
        let entry = import_profile_archive(Path::new(source_path), &profiles_dir)
            .map_err(|error| EngineError::Profile(error.to_string()))?;
        store
            .add_profile_to_index(entry.clone())
            .map_err(|error| EngineError::Config(error.to_string()))?;
        Ok(entry)
    }

    pub fn export_profile(&self, profile_id: &str, destination: &str) -> Result<(), EngineError> {
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let entry = config
            .profile_index
            .iter()
            .find(|entry| entry.id == profile_id)
            .cloned()
            .ok_or_else(|| EngineError::Profile(format!("profile not found: {profile_id}")))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        profile_store
            .export_profile_archive(&entry, Path::new(destination))
            .map_err(|error| EngineError::Profile(error.to_string()))
    }

    pub fn set_stream_target(
        &mut self,
        stream_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);
        let intent = RoutingIntent {
            stream_id: stream_id.to_string(),
            target_device_id: self.resolve_device_id(target_device_id),
        };

        let apply_result = if self.graph.data_source == "mock" {
            apply_mock_routing(&mut self.graph, &intent)
        } else {
            apply_routing_intent(&self.graph, &intent).map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            if let (Some(stream), Some(target)) = (
                self.graph
                    .streams
                    .iter()
                    .find(|stream| stream.id == intent.stream_id),
                self.graph
                    .devices
                    .iter()
                    .find(|device| device.id == intent.target_device_id),
            ) {
                let _ = crate::core::routing_rules::save_stream_route_rule(stream, target);
            }
        }

        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn set_device_route(
        &mut self,
        source_device_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);
        let intent = DeviceRouteIntent {
            source_device_id: self.resolve_device_id(source_device_id),
            target_device_id: self.resolve_device_id(target_device_id),
        };

        let apply_result = if self.graph.data_source == "mock" {
            apply_mock_device_route(&mut self.graph, &intent)
        } else {
            apply_device_route_intent(&self.graph, &intent)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            if let (Some(source), Some(target)) = (
                self.graph
                    .devices
                    .iter()
                    .find(|device| device.id == intent.source_device_id),
                self.graph
                    .devices
                    .iter()
                    .find(|device| device.id == intent.target_device_id),
            ) {
                let _ = crate::core::routing_rules::save_device_route_rule(source, target);
            }
        }

        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn undo_last_routing(&mut self) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let Some(snapshot) = self.rollback_stack.pop() else {
            return Ok(ApplyResult {
                success: false,
                message: Some("nothing to undo".into()),
            });
        };

        let restore_result = if self.graph.data_source == "mock" {
            apply_mock_snapshot(&mut self.graph, &snapshot)
        } else {
            restore_routing_snapshot(&self.graph, &snapshot)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = restore_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn swap_profile(&mut self, profile_id: &str) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let store = ConfigStore::new();
        let config = store
            .load_config()
            .map_err(|error| EngineError::Config(error.to_string()))?;
        let profile_store = ProfileStore::new(store.config_dir().clone());
        let profile = profile_store
            .load_profile_by_id(profile_id, &config.profile_index)
            .map_err(|error| EngineError::Profile(error.to_string()))?;

        let snapshot = capture_routing_snapshot(&self.graph);

        let routing_result = if self.graph.data_source == "mock" {
            apply_mock_profile(&mut self.graph, &profile)
        } else {
            apply_profile_routing(&self.graph, &profile)
                .map_err(|error| EngineError::Routing(error.to_string()))
        };

        if let Err(error) = routing_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            let _ = if self.graph.data_source == "mock" {
                apply_mock_snapshot(&mut self.graph, &snapshot)
            } else {
                restore_routing_snapshot(&self.graph, &snapshot)
                    .map_err(|error| EngineError::Routing(error.to_string()))
            };
            if self.graph.data_source != "mock" {
                self.refresh_graph()?;
            }
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if self.graph.data_source != "mock" {
            if let Err(error) = apply_profile_volumes(&self.graph, &profile) {
                let message = error.to_string();
                self.last_error = Some(message.clone());
                let _ = restore_routing_snapshot(&self.graph, &snapshot);
                self.refresh_graph()?;
                return Ok(ApplyResult {
                    success: false,
                    message: Some(message),
                });
            }
        } else {
            apply_mock_profile_volumes(&mut self.graph, &profile);
        }

        store
            .set_active_profile(profile_id)
            .map_err(|error| EngineError::Config(error.to_string()))?;
        self.rollback_stack.push(snapshot);
        if self.graph.data_source != "mock" {
            self.refresh_graph()?;
        }
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn set_device_volume(&mut self, device_id: &str, percent: u8) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.volume_percent = Some(percent.min(100));
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        pactl::set_device_volume(device_id, &self.graph, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.muted = Some(muted);
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        pactl::set_device_mute(device_id, &self.graph, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn create_virtual_output(&mut self, name: &str) -> Result<VirtualDeviceResult, EngineError> {
        if self.graph.data_source == "mock" {
            let slug = name.to_lowercase().replace(' ', "-");
            let system_name = format!("pipe-deck-{slug}");
            self.graph.devices.push(crate::core::models::Device {
                id: format!("virtual-{slug}"),
                system_name: system_name.clone(),
                label: name.to_string(),
                kind: crate::core::models::DeviceKind::Virtual,
                direction: crate::core::models::DeviceDirection::Output,
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
            });
        }

        let result = self
            .virtual_registry
            .create_output(name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
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
                volume_percent: Some(100),
                muted: Some(false),
                current_target: None,
            });
            return Ok(VirtualDeviceResult {
                device_id: format!("virtual-{slug}"),
                system_name,
                label: name.to_string(),
            });
        }

        let result = self
            .virtual_registry
            .create_input(name)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
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
        self.refresh_graph()?;
        Ok(())
    }
}

fn merge_virtual_devices(
    graph: &mut RuntimeGraph,
    registry: &VirtualDeviceRegistry,
    device_id_remap: &mut HashMap<String, String>,
) {
    let mut id_remap = HashMap::new();

    for entry in registry.list_devices() {
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
            if device.volume_percent.is_none() {
                device.volume_percent = Some(100);
            }
            if device.muted.is_none() {
                device.muted = Some(false);
            }
        } else {
            graph.devices.push(entry.to_device());
        }
    }

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

fn apply_mock_routing(
    graph: &mut RuntimeGraph,
    intent: &RoutingIntent,
) -> Result<(), EngineError> {
    let stream = graph
        .streams
        .iter_mut()
        .find(|stream| stream.id == intent.stream_id)
        .ok_or_else(|| EngineError::Routing(format!("stream not found: {}", intent.stream_id)))?;
    if !graph
        .devices
        .iter()
        .any(|device| device.id == intent.target_device_id)
    {
        return Err(EngineError::Routing(format!(
            "target device not found: {}",
            intent.target_device_id
        )));
    }
    stream.current_target = Some(intent.target_device_id.clone());
    Ok(())
}

fn apply_mock_snapshot(
    graph: &mut RuntimeGraph,
    snapshot: &RoutingSnapshot,
) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
    }
    for device in &mut graph.devices {
        device.current_target = None;
    }
    for intent in &snapshot.stream_intents {
        apply_mock_routing(graph, intent)?;
    }
    for intent in &snapshot.device_intents {
        apply_mock_device_route(graph, intent)?;
    }
    Ok(())
}

fn apply_mock_device_route(
    graph: &mut RuntimeGraph,
    intent: &DeviceRouteIntent,
) -> Result<(), EngineError> {
    if !graph
        .devices
        .iter()
        .any(|device| device.id == intent.source_device_id)
    {
        return Err(EngineError::Routing(format!(
            "source device not found: {}",
            intent.source_device_id
        )));
    }
    if !graph
        .devices
        .iter()
        .any(|device| device.id == intent.target_device_id)
    {
        return Err(EngineError::Routing(format!(
            "target device not found: {}",
            intent.target_device_id
        )));
    }

    let device = graph
        .devices
        .iter_mut()
        .find(|device| device.id == intent.source_device_id)
        .expect("source device exists");
    device.current_target = Some(intent.target_device_id.clone());
    Ok(())
}

fn apply_mock_profile(graph: &mut RuntimeGraph, profile: &Profile) -> Result<(), EngineError> {
    for stream in &mut graph.streams {
        stream.current_target = None;
    }
    for intent in &profile.routing_intents {
        apply_mock_routing(graph, intent)?;
    }
    Ok(())
}

fn apply_mock_profile_volumes(graph: &mut RuntimeGraph, profile: &Profile) {
    for (device_id, state) in &profile.volume_state {
        if let Some(device) = graph.devices.iter_mut().find(|device| device.id == *device_id) {
            device.volume_percent = Some(state.volume_percent);
            device.muted = Some(state.muted);
        }
    }
}

fn create_adapter() -> Box<dyn PipeWireAdapter> {
    if std::env::var("PIPE_DECK_USE_MOCK").as_deref() == Ok("1") {
        return Box::new(crate::pipewire::mock::MockPipeWireAdapter::new());
    }

    match crate::pipewire::live::LivePipeWireAdapter::new() {
        Ok(adapter) => Box::new(adapter),
        Err(error) => {
            eprintln!("PipeWire enumeration unavailable: {error}");
            Box::new(EmptyPipeWireAdapter {
                notice: format!("PipeWire unavailable: {error}"),
            })
        }
    }
}

struct EmptyPipeWireAdapter {
    notice: String,
}

impl PipeWireAdapter for EmptyPipeWireAdapter {
    fn fetch_graph(&self) -> Result<RuntimeGraph, crate::pipewire::adapter::AdapterError> {
        Ok(RuntimeGraph {
            notice: Some(self.notice.clone()),
            ..RuntimeGraph::default()
        })
    }

    fn subscribe(
        &self,
        _listener: crate::pipewire::adapter::GraphListener,
    ) -> Result<(), crate::pipewire::adapter::AdapterError> {
        Ok(())
    }
}
