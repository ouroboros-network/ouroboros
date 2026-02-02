// src/subchain/messages.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type NodeId = String;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MicroAnchorLeaf {
    pub microchain_id: Uuid,
    pub height: u64,
    pub micro_root: Vec<u8>,
    pub timestamp: i64,
    pub sig_micro: Vec<u8>, // signature bytes
    pub archive_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchAnchor {
    pub batch_root: Vec<u8>,
    pub aggregator_id: NodeId,
    pub leaf_count: usize,
    pub canonical_order: String,
}
