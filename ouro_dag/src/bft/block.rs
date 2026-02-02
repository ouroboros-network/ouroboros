use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub id: Uuid,
    pub proposer: String,
    pub view: u64,
    pub parent: Option<Uuid>,
    pub tx_ids: Vec<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub sig: Option<String>,               // proposer signature (placeholder)
    pub validator_signatures: Vec<String>, // collected validator sigs (for QC / evidence)
}

impl Block {
    pub fn new(proposer: String, view: u64, parent: Option<Uuid>, tx_ids: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            proposer,
            view,
            parent,
            tx_ids,
            timestamp: Utc::now(),
            sig: None,
            validator_signatures: vec![],
        }
    }
}
