use crate::core::models::RuntimeGraph;
use crate::pipewire::adapter::PipeWireAdapter;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("pipewire adapter error: {0}")]
    Adapter(String),
}

pub struct CoreEngine {
    graph: RuntimeGraph,
    adapter: Box<dyn PipeWireAdapter>,
}

impl CoreEngine {
    pub fn new() -> Self {
        Self {
            graph: RuntimeGraph::default(),
            adapter: create_adapter(),
        }
    }

    pub fn runtime_graph(&self) -> &RuntimeGraph {
        &self.graph
    }

    pub async fn initialize(
        &mut self,
        app: &AppHandle,
        engine_ref: Arc<RwLock<CoreEngine>>,
    ) -> Result<(), EngineError> {
        self.refresh_graph()?;
        self.emit_graph_update(app);

        let app_handle = app.clone();
        self.adapter
            .subscribe(Box::new(move |graph| {
                let app_handle = app_handle.clone();
                let engine_ref = engine_ref.clone();
                tauri::async_runtime::spawn(async move {
                    engine_ref.write().await.apply_graph_update(graph.clone());
                    let _ = app_handle.emit("graph-updated", graph);
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
        Ok(())
    }

    pub fn apply_graph_update(&mut self, graph: RuntimeGraph) {
        self.graph = graph;
    }

    pub fn emit_graph_update(&self, app: &AppHandle) {
        let _ = app.emit("graph-updated", self.graph.clone());
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
