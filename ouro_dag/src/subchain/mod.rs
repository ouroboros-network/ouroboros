// src/subchain/mod.rs
pub mod store;
pub mod messages;
pub mod poster;
pub mod anchor;
pub mod manager;
pub mod aggregator;
pub mod relayer;
pub mod api;
pub mod poster_worker;
pub mod registry; // Subchain registry and rent system
pub mod rent_collector; // Background rent collection task

// Phase 6: Fraud proof system for verifiable batch anchoring
pub mod fraud;

pub use rent_collector::{RentCollector, RentCollectorConfig, RentCollectionStats};
