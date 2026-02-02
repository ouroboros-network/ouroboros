use crate::PgPool;
// src/bft/slashing.rs
// Slashing mechanism for Byzantine validators
//
// Punishes validators who:
// - Sign invalid votes
// - Equivocate (double-vote in same view)
// - Submit fraudulent data
// - Fail to participate in consensus

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Slashing reasons
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SlashingReason {
    /// Validator signed invalid vote (failed cryptographic verification)
    InvalidSignature,

    /// Validator voted for multiple blocks in same view (equivocation)
    Equivocation,

    /// Validator submitted fraudulent batch/proof
    FraudulentData,

    /// Validator failed to participate for extended period
    Inactivity,

    /// Validator violated protocol rules
    ProtocolViolation,
}

/// Slashing severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlashingSeverity {
    /// Minor violation: 5% stake penalty
    Minor,

    /// Moderate violation: 20% stake penalty
    Moderate,

    /// Major violation: 50% stake penalty
    Major,

    /// Critical violation: 100% stake penalty (full slashing)
    Critical,
}

impl SlashingSeverity {
    /// Get penalty percentage
    pub fn penalty_percentage(&self) -> u64 {
        match self {
            SlashingSeverity::Minor => 5,
            SlashingSeverity::Moderate => 20,
            SlashingSeverity::Major => 50,
            SlashingSeverity::Critical => 100,
        }
    }
}

/// Slashing event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashingEvent {
    /// Validator being slashed
    pub validator_id: String,

    /// Reason for slashing
    pub reason: SlashingReason,

    /// Severity of violation
    pub severity: SlashingSeverity,

    /// Stake before slashing
    pub stake_before: u64,

    /// Amount slashed
    pub slashed_amount: u64,

    /// Stake after slashing
    pub stake_after: u64,

    /// When slashing occurred
    pub slashed_at: DateTime<Utc>,

    /// Evidence (block ID, view number, etc.)
    pub evidence: String,
}

/// Slashing manager
pub struct SlashingManager {
    db: PgPool, // RocksDB storage
}

impl SlashingManager {
    /// Create new slashing manager
    pub fn new(pool: PgPool) -> Self {
        Self { db: pool }
    }

    /// Slash a validator
    ///
    /// # Arguments
    /// * `validator_id` - Validator to slash
    /// * `reason` - Reason for slashing
    /// * `severity` - Severity of violation
    /// * `evidence` - Evidence of violation
    ///
    /// # Returns
    /// Slashing event with details
    pub async fn slash_validator(
        &self,
        validator_id: &str,
        reason: SlashingReason,
        severity: SlashingSeverity,
        evidence: &str,
    ) -> Result<SlashingEvent> {
        // Get current stake
        let stake_before = self.get_validator_stake(validator_id).await?;

        if stake_before == 0 {
            anyhow::bail!("Validator {} has no stake to slash", validator_id);
        }

        // Calculate slashing amount
        let penalty_pct = severity.penalty_percentage();
        let slashed_amount = (stake_before * penalty_pct) / 100;
        let stake_after = stake_before.saturating_sub(slashed_amount);

        log::error!(
            " SLASHING VALIDATOR: {} for {:?} (severity: {:?})",
            validator_id,
            reason,
            severity
        );
        log::error!(
            " Stake before: {} units, slashing: {} units ({}%), remaining: {} units",
            stake_before,
            slashed_amount,
            penalty_pct,
            stake_after
        );
        log::error!(" Evidence: {}", evidence);

        // Update validator stake in database - CRITICAL: Actually persist the reduced stake
        let stake_key = format!("validator_stake:{}", validator_id);
        crate::storage::put_str(&self.db, &stake_key, &stake_after.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to update validator stake: {}", e))?;

        // Record slashing event
        let event = SlashingEvent {
            validator_id: validator_id.to_string(),
            reason: reason.clone(),
            severity: severity.clone(),
            stake_before,
            slashed_amount,
            stake_after,
            slashed_at: Utc::now(),
            evidence: evidence.to_string(),
        };

        // Persist slashing event to database
        self.persist_slashing_event(&event).await?;

        // Emit alert
        log::error!(
            "WARNING SLASHING EVENT RECORDED: validator={}, reason={:?}, amount={} units",
            validator_id,
            reason,
            slashed_amount
        );

        Ok(event)
    }

    /// Get validator stake
    async fn get_validator_stake(&self, validator_id: &str) -> Result<u64> {
        // Query validator stake from RocksDB
        let key = format!("validator_stake:{}", validator_id);
        match crate::storage::get_str::<String>(&self.db, &key).map_err(|e| anyhow::anyhow!(e))? {
            Some(stake_str) => Ok(stake_str.parse().unwrap_or(0)),
            None => Ok(0),
        }
    }

    /// Persist a slashing event to the database
    async fn persist_slashing_event(&self, event: &SlashingEvent) -> Result<()> {
        // Store individual event with timestamp-based key for ordering
        let event_key = format!(
            "slashing_event:{}:{}",
            event.slashed_at.timestamp_millis(),
            event.validator_id
        );
        crate::storage::put(&self.db, event_key.as_bytes(), event)
            .map_err(|e| anyhow::anyhow!("Failed to store slashing event: {}", e))?;

        // Also append to validator's slashing history
        let history_key = format!("slashing_history:{}", event.validator_id);
        let mut history: Vec<SlashingEvent> = crate::storage::get(&self.db, history_key.as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?
            .unwrap_or_default();
        history.push(event.clone());
        crate::storage::put(&self.db, history_key.as_bytes(), &history)
            .map_err(|e| anyhow::anyhow!("Failed to update slashing history: {}", e))?;

        // Update global slashing event index for recent queries
        let index_key = "slashing_events_index";
        let mut index: Vec<String> = crate::storage::get(&self.db, index_key.as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?
            .unwrap_or_default();
        index.push(event_key.clone());
        // Keep only last 1000 events in index
        if index.len() > 1000 {
            index = index.split_off(index.len() - 1000);
        }
        crate::storage::put(&self.db, index_key.as_bytes(), &index)
            .map_err(|e| anyhow::anyhow!("Failed to update slashing index: {}", e))?;

        log::info!("Slashing event persisted: {}", event_key);
        Ok(())
    }

    pub async fn get_recent_slashing_events(&self, limit: i64) -> Result<Vec<SlashingEvent>> {
        // Query slashing events index from RocksDB
        let index_key = "slashing_events_index";
        let index: Vec<String> = crate::storage::get(&self.db, index_key.as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?
            .unwrap_or_default();

        // Get the most recent events up to limit
        let mut events = Vec::new();
        let start = if index.len() as i64 > limit {
            index.len() - limit as usize
        } else {
            0
        };

        for event_key in index.iter().skip(start).rev() {
            if let Ok(Some(event)) =
                crate::storage::get::<_, SlashingEvent>(&self.db, event_key.as_bytes())
            {
                events.push(event);
                if events.len() >= limit as usize {
                    break;
                }
            }
        }

        Ok(events)
    }

    /// Get slashing history for a specific validator
    pub async fn get_slashing_history(&self, validator_id: &str) -> Result<Vec<SlashingEvent>> {
        // Query validator-specific slashing history from RocksDB
        let key = format!("slashing_history:{}", validator_id);
        match crate::storage::get::<_, Vec<SlashingEvent>>(&self.db, key.as_bytes())
            .map_err(|e| anyhow::anyhow!(e))?
        {
            Some(events) => Ok(events),
            None => Ok(Vec::new()),
        }
    }

    /// Set validator stake (for initialization or recovery)
    pub async fn set_validator_stake(&self, validator_id: &str, stake: u64) -> Result<()> {
        let stake_key = format!("validator_stake:{}", validator_id);
        crate::storage::put_str(&self.db, &stake_key, &stake.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to set validator stake: {}", e))?;
        log::info!("Validator {} stake set to {}", validator_id, stake);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_penalties() {
        assert_eq!(SlashingSeverity::Minor.penalty_percentage(), 5);
        assert_eq!(SlashingSeverity::Moderate.penalty_percentage(), 20);
        assert_eq!(SlashingSeverity::Major.penalty_percentage(), 50);
        assert_eq!(SlashingSeverity::Critical.penalty_percentage(), 100);
    }

    #[test]
    fn test_slashing_reasons() {
        let reasons = vec![
            SlashingReason::InvalidSignature,
            SlashingReason::Equivocation,
            SlashingReason::FraudulentData,
            SlashingReason::Inactivity,
            SlashingReason::ProtocolViolation,
        ];

        for reason in reasons {
            let json = serde_json::to_string(&reason).unwrap();
            let parsed: SlashingReason = serde_json::from_str(&json).unwrap();
            assert_eq!(reason, parsed);
        }
    }
}
