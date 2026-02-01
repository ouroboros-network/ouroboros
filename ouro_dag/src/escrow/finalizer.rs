// src/escrow/finalizer.rs
// Escrow finalizer - migrated from PostgreSQL to RocksDB

use crate::escrow::EscrowManager;
use crate::storage::{self, RocksDb};
use crate::PgPool;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;
use uuid::Uuid;
use chrono::Utc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProvisionalBalance {
    id: Uuid,
    microchain_id: String,
    account: String,
    amount: i64,
    created_at: i64,
}

/// Simple periodic finalizer that looks for provisional balances and finalizes them.
pub async fn run_finalizer(db: PgPool, _escrow_mgr: EscrowManager) -> Result<()> {
    let threshold_seconds = 10 * 60; // 10 minutes

    loop {
        let now = Utc::now().timestamp();

        // Find provisional balances older than threshold from RocksDB
        let balances: Vec<ProvisionalBalance> = storage::iter_prefix::<ProvisionalBalance>(&db, b"provisional_balance:")
            .unwrap_or_default()
            .into_iter()
            .filter(|b| now - b.created_at > threshold_seconds)
            .take(100)
            .collect();

        for balance in balances {
            // Get current balance for the account
            let balance_key = format!("balance:{}", balance.account);
            let current: i64 = storage::get_str(&db, &balance_key)
                .unwrap_or(None)
                .unwrap_or(0);

            // Add the provisional amount
            let new_balance = current + balance.amount;

            // Update balance
            if let Err(e) = storage::put_str(&db, &balance_key, &new_balance) {
                log::error!("Failed to update balance for {}: {}", balance.account, e);
                continue;
            }

            // Remove provisional record
            let prov_key = format!("provisional_balance:{}", balance.id);
            let _ = db.delete(prov_key.as_bytes());

            log::info!(
                "Finalized provisional balance: {} -> {} (+{})",
                balance.account,
                new_balance,
                balance.amount
            );
        }

        sleep(Duration::from_secs(30)).await;
    }
}
