use crate::config::ConfigStore;
use crate::core::models::RuntimeGraph;
use crate::core::rules::{self, ApplyRulesContext};
use crate::core::stream_identity::stream_identity_key;
use std::collections::HashSet;
use tauri::{AppHandle, Emitter};

use super::virtual_ops::merge_virtual_devices;
use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn refresh_graph(&mut self) -> Result<(), EngineError> {
        self.graph = self
            .adapter
            .fetch_graph()
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        merge_virtual_devices(&mut self.graph, &mut self.device_id_remap, self.adapter.as_ref());
        self.sync_live_graph();
        self.finalize_graph_snapshot();
        self.apply_rules_for_new_streams();
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.push_graph(&self.graph);
        }
        self.apply_queued_plugin_effect_requests();
        Ok(())
    }

    pub fn apply_graph_update(&mut self, graph: RuntimeGraph) {
        self.graph = graph;
        merge_virtual_devices(&mut self.graph, &mut self.device_id_remap, self.adapter.as_ref());
        self.sync_live_graph();
        self.finalize_graph_snapshot();
        self.apply_rules_for_new_streams();
        if let Ok(mut plugins) = self.plugin_manager.lock() {
            plugins.push_graph(&self.graph);
        }
        self.apply_queued_plugin_effect_requests();
    }

    fn finalize_graph_snapshot(&mut self) {
        self.recent_streams.record_streams(&self.graph.streams);
        self.graph.recent_stream_identities = self.recent_streams.list(&self.graph.streams);
    }

    fn sync_live_graph(&mut self) {
        self.adapter.sync_live_routing_graph(&mut self.graph);
        self.adapter.apply_user_cleared_routes(
            &mut self.graph,
            &self.cleared_stream_routes,
            &self.cleared_device_routes,
        );
    }

    pub fn apply_desired_routing(&mut self) -> Result<(), EngineError> {
        self.manual_overrides.clear();
        self.device_manual_overrides.clear();
        self.cleared_stream_routes.clear();
        self.cleared_device_routes.clear();
        self.apply_routing_rules();
        Ok(())
    }

    fn apply_routing_rules(&mut self) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());

        rules::reconcile_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rules::reconcile_device_manual_overrides(
            &self.graph,
            &mut self.device_manual_overrides,
            &config.routing_rules.device_rules,
            self.adapter.as_ref(),
        );

        rules::detect_external_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rules::detect_external_device_manual_overrides(
            &self.graph,
            &mut self.device_manual_overrides,
            &config.routing_rules.device_rules,
            self.adapter.as_ref(),
        );

        let ctx = ApplyRulesContext {
            manual_overrides: &self.manual_overrides,
            device_manual_overrides: &self.device_manual_overrides,
            dry_run: false,
            mock_graph_only: self.graph.data_source == "mock",
            limit_to_identities: None,
            backend: self.adapter.as_ref(),
        };
        self.adapter.apply_graph_routing(&mut self.graph, &ctx);
    }

    fn apply_rules_for_new_streams(&mut self) {
        let config = ConfigStore::new()
            .load_config()
            .unwrap_or_else(|_| ConfigStore::default_config());
        if !config.preferences.auto_apply_rules {
            return;
        }

        let mut new_identities = HashSet::new();
        for stream in &self.graph.streams {
            if stream.is_system {
                continue;
            }
            let key = stream_identity_key(stream);
            if !self.seen_stream_identities.contains(&key) {
                new_identities.insert(key);
            }
        }

        if new_identities.is_empty() {
            return;
        }

        rules::reconcile_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );
        rules::detect_external_manual_overrides(
            &self.graph,
            &mut self.manual_overrides,
            &config.rules,
            &config.routing_rules.stream_rules,
        );

        let ctx = ApplyRulesContext {
            manual_overrides: &self.manual_overrides,
            device_manual_overrides: &self.device_manual_overrides,
            dry_run: false,
            mock_graph_only: self.graph.data_source == "mock",
            limit_to_identities: Some(&new_identities),
            backend: self.adapter.as_ref(),
        };
        let _ = rules::apply_routing_rules_with_explanations(&mut self.graph, &ctx);

        for key in new_identities {
            self.seen_stream_identities.insert(key);
        }
    }

    pub fn emit_graph_update(&self, app: &AppHandle) {
        let _ = app.emit("graph-updated", self.graph.clone());
    }
}
