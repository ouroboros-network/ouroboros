// src/subchain/mod.rs
pub mod aggregator;
pub mod anchor;
pub mod api;
pub mod manager;
pub mod messages;
pub mod poster;
pub mod poster_worker;
pub mod registry; // Subchain registry and rent system
pub mod relayer;
pub mod rent_collector;
pub mod store; // Background rent collection task

// Phase 6: Fraud proof system for verifiable batch anchoring
pub mod fraud;

pub use rent_collector::{RentCollectionStats, RentCollector, RentCollectorConfig};
