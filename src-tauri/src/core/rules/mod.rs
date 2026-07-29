mod evaluation;
mod manual_overrides;
mod matching;

pub use evaluation::*;
pub use manual_overrides::*;
pub use matching::*;

use std::collections::HashSet;

#[derive(Clone)]
pub struct ApplyRulesContext<'a> {
    pub manual_overrides: &'a HashSet<crate::core::stream_identity::StreamIdentityKey>,
    pub dry_run: bool,
    pub mock_graph_only: bool,
    /// When set, only streams whose `Stream.id` (the PipeWire node id, i.e.
    /// this specific stream instance) appears here are eligible for apply.
    pub limit_to_stream_ids: Option<&'a HashSet<String>>,
    /// Live routing-state fallback for rule matching (e.g. monitor-route
    /// discovery when `RuntimeGraph.current_targets` is stale/missing) — see
    /// `core/rules/matching.rs`.
    pub backend: &'a dyn crate::backend::AudioBackend,
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateRule {
    pub key: String,
    pub rule_id: Option<String>,
    pub target_system_names: Vec<String>,
    pub match_reasons: Vec<String>,
    pub priority: i32,
    pub source: crate::core::models::RouteSource,
    pub fallback_policy: crate::core::models::FallbackPolicy,
}
