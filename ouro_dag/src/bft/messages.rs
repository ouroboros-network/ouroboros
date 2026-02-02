// src/bft/messages.rs
// NOTE: This file contains alternative message definitions with Vec<u8> signatures.
// Currently NOT USED - active definitions are in consensus.rs with String (hex) signatures.
// TODO: Migrate to these definitions for better efficiency (binary sigs vs hex strings)

use crate::bft::consensus::{NodeId, View};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type BlockId = Uuid;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Proposal {
    pub block_id: BlockId,
    pub parent_id: Option<BlockId>,
    pub view: View,
    pub proposer: NodeId,
    pub sig: Vec<u8>, // signature bytes
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Vote {
    pub block_id: BlockId,
    pub view: View,
    pub voter: NodeId,
    pub sig: Vec<u8>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct QuorumCertificate {
    pub block_id: BlockId,
    pub view: View,
    pub signers: Vec<NodeId>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ViewChange {
    pub from: NodeId,
    pub from_view: View,
    pub highest_qc_block: Option<BlockId>,
    pub highest_qc_view: Option<View>,
    pub sig: Vec<u8>,
}
