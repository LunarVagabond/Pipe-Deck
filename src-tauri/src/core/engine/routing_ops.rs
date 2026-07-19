use crate::config::ConfigStore;
use crate::core::models::ApplyResult;
use crate::core::rules;
use crate::core::routing::capture_routing_snapshot;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn set_stream_target(
        &mut self,
        stream_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        if let Some(stream) = self.graph.streams.iter().find(|stream| stream.id == stream_id) {
            self.cleared_stream_routes
                .remove(&crate::core::stream_identity::stream_identity_key(stream));
        }
        let snapshot = capture_routing_snapshot(&self.graph);
        let resolved_target = self.resolve_device_id(target_device_id);

        let apply_result = self
            .adapter
            .route_stream(&self.graph, stream_id, &resolved_target)
            .map_err(|error| EngineError::Routing(error.to_string()));

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if let Some(stream) = self.graph.streams.iter().find(|s| s.id == stream_id) {
            if let Some(target) = self
                .graph
                .devices
                .iter()
                .find(|device| device.id == resolved_target)
            {
                let _ = crate::core::routing_rules::save_stream_route_rule(stream, target);
            }
        }

        self.sync_manual_override_for_ids(stream_id, &resolved_target);

        self.rollback_stack.push(snapshot);
        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn set_stream_targets(
        &mut self,
        stream_id: &str,
        target_device_ids: &[String],
    ) -> Result<ApplyResult, EngineError> {
        let Some(primary) = target_device_ids.first() else {
            return Ok(ApplyResult {
                success: false,
                message: Some("at least one target is required".into()),
            });
        };
        self.set_stream_target(stream_id, primary)
    }

    pub fn set_device_route(
        &mut self,
        source_device_id: &str,
        target_device_id: &str,
    ) -> Result<ApplyResult, EngineError> {
        self.set_device_targets(source_device_id, &[target_device_id.to_string()])
    }

    pub fn set_device_targets(
        &mut self,
        source_device_id: &str,
        target_device_ids: &[String],
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);
        let resolved_targets: Vec<String> = target_device_ids
            .iter()
            .map(|id| self.resolve_device_id(id))
            .collect();
        let resolved_source = self.resolve_device_id(source_device_id);

        let apply_result = self
            .adapter
            .route_device(&self.graph, &resolved_source, &resolved_targets)
            .map_err(|error| EngineError::Routing(error.to_string()));

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        if let Some(source) = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == resolved_source)
        {
            let targets: Vec<_> = resolved_targets
                .iter()
                .filter_map(|id| self.graph.devices.iter().find(|d| d.id == *id).cloned())
                .collect();
            if targets.is_empty() {
                let _ = crate::core::routing_rules::clear_device_route_rule(source);
                self.cleared_device_routes.insert(resolved_source.clone());
            } else {
                let _ = crate::core::routing_rules::save_device_route_rule(source, &targets);
                self.cleared_device_routes.remove(&resolved_source);
            }
        }

        if resolved_targets.is_empty() {
            if let Some(device) = self
                .graph
                .devices
                .iter_mut()
                .find(|device| device.id == resolved_source)
            {
                device.current_target = None;
                device.current_targets.clear();
            }
        }

        self.rollback_stack.push(snapshot);
        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    pub fn clear_stream_target(
        &mut self,
        stream_id: &str,
        previous_target_device_id: Option<&str>,
    ) -> Result<ApplyResult, EngineError> {
        self.clear_last_error();
        let snapshot = capture_routing_snapshot(&self.graph);

        let stream_identity = self
            .graph
            .streams
            .iter()
            .find(|stream| stream.id == stream_id)
            .map(crate::core::stream_identity::stream_identity_key);
        let Some(stream_identity) = stream_identity else {
            return Err(EngineError::Routing(format!("stream not found: {stream_id}")));
        };

        let apply_result: Result<(), EngineError> = self
            .adapter
            .clear_stream_target(&self.graph, stream_id, previous_target_device_id)
            .map_err(|error| EngineError::Routing(error.to_string()));

        if let Err(error) = apply_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        self.cleared_stream_routes.insert(stream_identity);

        if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
            stream.current_target = None;
        }

        if let Some(stream) = self.graph.streams.iter().find(|stream| stream.id == stream_id) {
            let _ = crate::core::routing_rules::clear_stream_route_rule(stream);
            self.manual_overrides
                .remove(&crate::core::stream_identity::stream_identity_key(stream));
        }

        self.rollback_stack.push(snapshot);
        self.refresh_graph()?;
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

        let restore_result: Result<(), EngineError> = (|| {
            for intent in &snapshot.stream_intents {
                let target = intent
                    .target_device_id
                    .as_ref()
                    .or_else(|| intent.target_device_ids.first())
                    .ok_or_else(|| EngineError::Routing("routing intent has no target".into()))?;
                self.adapter
                    .route_stream(&self.graph, &intent.stream_id, target)
                    .map_err(|error| EngineError::Routing(error.to_string()))?;
            }
            for intent in &snapshot.device_intents {
                self.adapter
                    .route_device(&self.graph, &intent.source_device_id, &intent.target_ids())
                    .map_err(|error| EngineError::Routing(error.to_string()))?;
            }
            Ok(())
        })();

        if let Err(error) = restore_result {
            let message = error.to_string();
            self.last_error = Some(message.clone());
            return Ok(ApplyResult {
                success: false,
                message: Some(message),
            });
        }

        self.refresh_graph()?;
        Ok(ApplyResult {
            success: true,
            message: None,
        })
    }

    fn sync_manual_override_for_ids(&mut self, stream_id: &str, target_device_id: &str) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());
        let Some((stream, target_system_name)) = (|| {
            let stream = self
                .graph
                .streams
                .iter()
                .find(|stream| stream.id == stream_id)?
                .clone();
            let target_system_name = self
                .graph
                .devices
                .iter()
                .find(|device| device.id == target_device_id)?
                .system_name
                .clone();
            Some((stream, target_system_name))
        })() else {
            return;
        };

        let identity = crate::core::stream_identity::stream_identity_key(&stream);
        if rules::should_track_manual_override(
            &stream,
            &target_system_name,
            &config.rules,
            &config.routing_rules.stream_rules,
        ) {
            self.manual_overrides.insert(identity);
        } else {
            self.manual_overrides.remove(&identity);
        }
    }

    fn resolve_device_id(&self, device_id: &str) -> String {
        self.device_id_remap
            .get(device_id)
            .cloned()
            .unwrap_or_else(|| device_id.to_string())
    }
}
