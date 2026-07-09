use crate::core::models::RuntimeGraph;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("{0}")]
    Message(String),
}

pub type GraphListener = Box<dyn Fn(RuntimeGraph) + Send + Sync>;

pub trait PipeWireAdapter: Send + Sync {
    fn fetch_graph(&self) -> Result<RuntimeGraph, AdapterError>;
    fn subscribe(&self, listener: GraphListener) -> Result<(), AdapterError>;
}
