//! WebSocket node discovery and registry.
//!
//! Stubs — full implementation deferred.

use axum::{
    response::IntoResponse,
};
use parking_lot::RwLock;
use std::collections::HashMap;

/// Information about a connected node.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub connected_at: String,
}

/// Registry of dynamically connected nodes.
pub struct NodeRegistry {
    nodes: RwLock<HashMap<String, NodeInfo>>,
    _max_nodes: usize,
}

impl NodeRegistry {
    pub fn new(max_nodes: usize) -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            _max_nodes: max_nodes,
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.read().is_empty()
    }
}

/// GET /ws/nodes — WebSocket node discovery
pub async fn handle_ws_nodes() -> impl IntoResponse {
    (axum::http::StatusCode::NOT_IMPLEMENTED, "Node discovery WebSocket not yet implemented")
}
