//! Fraud Detection and Monitoring Service
//!
//! Continuous monitoring system for detecting suspicious activity
//! in cross-chain transfers and microchain operations.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::storage::RocksDb;

pub mod patterns;
pub mod alerts;
pub mod api;

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
 Low,
 Medium,
 High,
 Critical,
}

/// Fraud detection alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudAlert {
 pub alert_id: String,
 pub severity: AlertSeverity,
 pub alert_type: AlertType,
 pub entity: String, // relayer, operator, or user
 pub description: String,
 pub evidence: Vec<u8>,
 pub timestamp: u64,
 pub auto_action: Option<AutoAction>,
}

/// Types of fraud alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
 /// Suspicious relay pattern detected
 SuspiciousRelayPattern,
 /// Multiple failed fraud proofs
 RepeatedFailedProofs,
 /// High value transfer without proper validation
 HighValueUnvalidated,
 /// Operator not anchoring state
 MissingStateAnchor,
 /// Invalid state transition detected
 InvalidStateTransition,
 /// Double spend attempt
 DoubleSpendAttempt,
 /// Unusual transaction volume
 AbnormalVolume,
 /// Rapid withdrawal pattern
 RapidWithdrawal,
}

/// Automated actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoAction {
 /// Submit fraud proof automatically
 SubmitFraudProof,
 /// Pause relayer
 PauseRelayer,
 /// Alert administrator
 AlertAdmin,
 /// Increase monitoring
 IncreaseMonitoring,
}

/// Activity monitoring statistics
#[derive(Debug, Clone)]
struct ActivityStats {
 total_relays: u64,
 successful_relays: u64,
 failed_relays: u64,
 total_volume: u64,
 last_activity: u64,
 suspicious_count: u64,
}

/// Fraud detection service
pub struct FraudDetectionService {
    /// Recent alerts
    alerts: Arc<RwLock<VecDeque<FraudAlert>>>,
    /// Activity statistics per entity
    activity_stats: Arc<RwLock<HashMap<String, ActivityStats>>>,
    /// Blacklisted entities
    blacklist: Arc<RwLock<HashMap<String, BlacklistEntry>>>,
    /// Monitoring rules
    rules: Arc<RwLock<Vec<MonitoringRule>>>,
    /// Alert thresholds
    thresholds: AlertThresholds,
    /// Database handle for persistence
    db: Option<Arc<RocksDb>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlacklistEntry {
    reason: String,
    timestamp: u64,
    permanent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActivityStatsSerializable {
    total_relays: u64,
    successful_relays: u64,
    failed_relays: u64,
    total_volume: u64,
    last_activity: u64,
    suspicious_count: u64,
}

#[derive(Debug, Clone)]
struct MonitoringRule {
 name: String,
 condition: RuleCondition,
 action: AutoAction,
}

#[derive(Debug, Clone)]
enum RuleCondition {
 FailureRateAbove(f64),
 VolumeAbove(u64),
 RapidTransactions(u64, u64), // count, time_window
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
 pub max_failure_rate: f64,
 pub max_volume_per_hour: u64,
 pub max_rapid_transactions: u64,
 pub min_anchor_frequency: u64,
}

impl Default for AlertThresholds {
 fn default() -> Self {
 Self {
 max_failure_rate: 0.1, // 10% failure rate triggers alert
 max_volume_per_hour: 10_000_000_000, // 100 OURO per hour
 max_rapid_transactions: 100, // 100 transactions in short period
 min_anchor_frequency: 3600, // 1 hour
 }
 }
}

impl FraudDetectionService {
    /// Create new fraud detection service
    pub fn new(thresholds: AlertThresholds) -> Self {
        Self {
            alerts: Arc::new(RwLock::new(VecDeque::new())),
            activity_stats: Arc::new(RwLock::new(HashMap::new())),
            blacklist: Arc::new(RwLock::new(HashMap::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
            thresholds,
            db: None,
        }
    }

    /// Create with database persistence
    pub fn with_db(thresholds: AlertThresholds, db: Arc<RocksDb>) -> Self {
        let mut service = Self {
            alerts: Arc::new(RwLock::new(VecDeque::new())),
            activity_stats: Arc::new(RwLock::new(HashMap::new())),
            blacklist: Arc::new(RwLock::new(HashMap::new())),
            rules: Arc::new(RwLock::new(Vec::new())),
            thresholds,
            db: Some(db),
        };
        // Load existing data from database
        if let Err(e) = service.load_from_db() {
            log::warn!("Failed to load fraud detection data from DB: {}", e);
        }
        service
    }

    /// Load data from database
    fn load_from_db(&mut self) -> Result<(), String> {
        let db = self.db.as_ref().ok_or("No database configured")?;

        // Load alerts
        if let Ok(Some(alerts)) = crate::storage::get::<_, Vec<FraudAlert>>(db, b"fraud_alerts") {
            let mut alert_queue = self.alerts.write().unwrap_or_else(|p| p.into_inner());
            for alert in alerts {
                alert_queue.push_back(alert);
            }
            log::info!("Loaded {} fraud alerts from database", alert_queue.len());
        }

        // Load blacklist
        if let Ok(Some(blacklist)) = crate::storage::get::<_, HashMap<String, BlacklistEntry>>(db, b"fraud_blacklist") {
            let mut bl = self.blacklist.write().unwrap_or_else(|p| p.into_inner());
            *bl = blacklist;
            log::info!("Loaded {} blacklisted entities from database", bl.len());
        }

        // Load activity stats
        if let Ok(Some(stats)) = crate::storage::get::<_, HashMap<String, ActivityStatsSerializable>>(db, b"fraud_activity_stats") {
            let mut activity = self.activity_stats.write().unwrap_or_else(|p| p.into_inner());
            for (entity, s) in stats {
                activity.insert(entity, ActivityStats {
                    total_relays: s.total_relays,
                    successful_relays: s.successful_relays,
                    failed_relays: s.failed_relays,
                    total_volume: s.total_volume,
                    last_activity: s.last_activity,
                    suspicious_count: s.suspicious_count,
                });
            }
            log::info!("Loaded activity stats for {} entities from database", activity.len());
        }

        Ok(())
    }

    /// Persist alerts to database
    fn persist_alerts(&self) {
        if let Some(db) = &self.db {
            let alerts = self.alerts.read().unwrap_or_else(|p| p.into_inner());
            let alert_vec: Vec<FraudAlert> = alerts.iter().cloned().collect();
            if let Err(e) = crate::storage::put(db, b"fraud_alerts", &alert_vec) {
                log::error!("Failed to persist fraud alerts: {}", e);
            }
        }
    }

    /// Persist blacklist to database
    fn persist_blacklist(&self) {
        if let Some(db) = &self.db {
            let blacklist = self.blacklist.read().unwrap_or_else(|p| p.into_inner());
            if let Err(e) = crate::storage::put(db, b"fraud_blacklist", &*blacklist) {
                log::error!("Failed to persist blacklist: {}", e);
            }
        }
    }

    /// Persist activity stats to database
    fn persist_activity_stats(&self) {
        if let Some(db) = &self.db {
            let stats = self.activity_stats.read().unwrap_or_else(|p| p.into_inner());
            let serializable: HashMap<String, ActivityStatsSerializable> = stats.iter()
                .map(|(k, v)| (k.clone(), ActivityStatsSerializable {
                    total_relays: v.total_relays,
                    successful_relays: v.successful_relays,
                    failed_relays: v.failed_relays,
                    total_volume: v.total_volume,
                    last_activity: v.last_activity,
                    suspicious_count: v.suspicious_count,
                }))
                .collect();
            if let Err(e) = crate::storage::put(db, b"fraud_activity_stats", &serializable) {
                log::error!("Failed to persist activity stats: {}", e);
            }
        }
    }

 /// Monitor cross-chain relay
    pub fn monitor_relay(
        &self,
        relayer: String,
        amount: u64,
        success: bool,
        current_time: u64,
    ) -> Option<FraudAlert> {
        // Update activity stats
        let mut stats_map = self.activity_stats.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let stats = stats_map.entry(relayer.clone()).or_insert(ActivityStats {
            total_relays: 0,
            successful_relays: 0,
            failed_relays: 0,
            total_volume: 0,
            last_activity: current_time,
            suspicious_count: 0,
        });

        stats.total_relays += 1;
        if success {
            stats.successful_relays += 1;
        } else {
            stats.failed_relays += 1;
        }
        stats.total_volume += amount;
        stats.last_activity = current_time;

        // Clone stats for checks after releasing lock
        let total_relays = stats.total_relays;
        let failed_relays = stats.failed_relays;
        drop(stats_map);

        // Persist activity stats
        self.persist_activity_stats();

        // Check if relayer is blacklisted
        let blacklist = self.blacklist.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        if blacklist.contains_key(&relayer) {
            drop(blacklist);
            return Some(self.create_alert(
                AlertSeverity::Critical,
                AlertType::SuspiciousRelayPattern,
                relayer.clone(),
                "Blacklisted entity attempting relay".to_string(),
                current_time,
                Some(AutoAction::PauseRelayer),
            ));
        }
        drop(blacklist);

        // Check failure rate
        if total_relays >= 10 {
            let failure_rate = failed_relays as f64 / total_relays as f64;
            if failure_rate > self.thresholds.max_failure_rate {
                // Update suspicious count
                let mut stats_map = self.activity_stats.write().unwrap_or_else(|p| p.into_inner());
                if let Some(stats) = stats_map.get_mut(&relayer) {
                    stats.suspicious_count += 1;
                }
                drop(stats_map);
                self.persist_activity_stats();

                return Some(self.create_alert(
                    AlertSeverity::High,
                    AlertType::SuspiciousRelayPattern,
                    relayer.clone(),
                    format!("High failure rate: {:.2}%", failure_rate * 100.0),
                    current_time,
                    Some(AutoAction::IncreaseMonitoring),
                ));
            }
        }

        // Check for high value unvalidated transfers
        if amount > 100_000_000_000 && !success { // > 1000 OURO failed
            return Some(self.create_alert(
                AlertSeverity::Critical,
                AlertType::HighValueUnvalidated,
                relayer,
                format!("High value relay failed: {} OURO", amount / 100_000_000),
                current_time,
                Some(AutoAction::AlertAdmin),
            ));
        }

        None
    }

 /// Monitor microchain operator
 pub fn monitor_operator(
 &self,
 operator: String,
 microchain_id: String,
 last_anchor_time: u64,
 current_time: u64,
 ) -> Option<FraudAlert> {
 // Check if operator is anchoring regularly
 let time_since_anchor = current_time - last_anchor_time;

 if time_since_anchor > self.thresholds.min_anchor_frequency * 2 {
 return Some(self.create_alert(
 AlertSeverity::High,
 AlertType::MissingStateAnchor,
 operator,
 format!(
 "Microchain {} has not anchored state in {} hours",
 microchain_id,
 time_since_anchor / 3600
 ),
 current_time,
 Some(AutoAction::AlertAdmin),
 ));
 }

 None
 }

 /// Monitor transaction patterns for double spend
 pub fn monitor_transactions(
 &self,
 user: String,
 transactions: Vec<(u64, u64)>, // (nonce, timestamp)
 current_time: u64,
 ) -> Option<FraudAlert> {
 // Check for duplicate nonces
 let mut nonces = HashMap::new();
 for (nonce, _timestamp) in &transactions {
 if nonces.contains_key(nonce) {
 return Some(self.create_alert(
 AlertSeverity::Critical,
 AlertType::DoubleSpendAttempt,
 user.clone(),
 format!("Duplicate nonce detected: {}", nonce),
 current_time,
 Some(AutoAction::SubmitFraudProof),
 ));
 }
 nonces.insert(nonce, true);
 }

 // Check for rapid transaction pattern
 if transactions.len() as u64 > self.thresholds.max_rapid_transactions {
 let time_window = if let (Some(last), Some(first)) = (transactions.last(), transactions.first()) {
 last.1 - first.1
 } else {
 return None; // Should never happen if len > 0, but handle gracefully
 };
 if time_window < 60 { // Within 1 minute
 return Some(self.create_alert(
 AlertSeverity::Medium,
 AlertType::RapidWithdrawal,
 user,
 format!("Rapid transactions detected: {} in {} seconds", transactions.len(), time_window),
 current_time,
 Some(AutoAction::IncreaseMonitoring),
 ));
 }
 }

 None
 }

 /// Check volume patterns
 pub fn check_volume_anomaly(
 &self,
 entity: String,
 current_time: u64,
 ) -> Option<FraudAlert> {
 let stats = self.activity_stats.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 if let Some(activity) = stats.get(&entity) {
 // Check if volume in last hour exceeds threshold
 if activity.total_volume > self.thresholds.max_volume_per_hour {
 return Some(self.create_alert(
 AlertSeverity::High,
 AlertType::AbnormalVolume,
 entity,
 format!(
 "Abnormal volume: {} OURO",
 activity.total_volume / 100_000_000
 ),
 current_time,
 Some(AutoAction::IncreaseMonitoring),
 ));
 }
 }

 None
 }

    /// Add entity to blacklist
    pub fn blacklist_entity(&self, entity: String, reason: String, permanent: bool, current_time: u64) {
        let mut blacklist = self.blacklist.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        blacklist.insert(entity.clone(), BlacklistEntry {
            reason: reason.clone(),
            timestamp: current_time,
            permanent,
        });
        drop(blacklist);

        // Persist to database
        self.persist_blacklist();

        log::warn!("BLOCKED Entity blacklisted: {}", entity);
        log::warn!(" Reason: {}", reason);
        log::warn!(" Permanent: {}", permanent);
    }

    /// Remove entity from blacklist
    pub fn unblacklist_entity(&self, entity: &str) -> Result<(), String> {
        let mut blacklist = self.blacklist.write().unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(entry) = blacklist.get(entity) {
            if entry.permanent {
                return Err("Cannot unblacklist permanent entry".to_string());
            }
        }

        blacklist.remove(entity);
        drop(blacklist);

        // Persist to database
        self.persist_blacklist();

        log::info!("Entity removed from blacklist: {}", entity);
        Ok(())
    }

 /// Check if entity is blacklisted
 pub fn is_blacklisted(&self, entity: &str) -> bool {
 let blacklist = self.blacklist.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 blacklist.contains_key(entity)
 }

 /// Get recent alerts
 pub fn get_recent_alerts(&self, count: usize) -> Vec<FraudAlert> {
 let alerts = self.alerts.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 alerts.iter().take(count).cloned().collect()
 }

 /// Get alerts by severity
 pub fn get_alerts_by_severity(&self, severity: AlertSeverity) -> Vec<FraudAlert> {
 let alerts = self.alerts.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 alerts
 .iter()
 .filter(|a| a.severity == severity)
 .cloned()
 .collect()
 }

 /// Get activity statistics
 pub fn get_activity_stats(&self, entity: &str) -> Option<(u64, u64, u64, u64)> {
 let stats = self.activity_stats.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 stats.get(entity).map(|s| (
 s.total_relays,
 s.successful_relays,
 s.failed_relays,
 s.total_volume,
 ))
 }

 /// Generate monitoring report
 pub fn generate_report(&self) -> MonitoringReport {
 let alerts = self.alerts.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let stats = self.activity_stats.read().unwrap_or_else(|poisoned| poisoned.into_inner());
 let blacklist = self.blacklist.read().unwrap_or_else(|poisoned| poisoned.into_inner());

 let critical_alerts = alerts.iter().filter(|a| a.severity == AlertSeverity::Critical).count();
 let high_alerts = alerts.iter().filter(|a| a.severity == AlertSeverity::High).count();
 let total_entities = stats.len();
 let blacklisted_entities = blacklist.len();

 MonitoringReport {
 total_alerts: alerts.len(),
 critical_alerts,
 high_alerts,
 total_entities,
 blacklisted_entities,
 timestamp: Self::current_timestamp(),
 }
 }

 /// Clear old alerts (keep last 1000)
 pub fn cleanup_old_alerts(&self) {
 let mut alerts = self.alerts.write().unwrap_or_else(|poisoned| poisoned.into_inner());
 while alerts.len() > 1000 {
 alerts.pop_back();
 }
 }

 // Private helper methods

    fn create_alert(
        &self,
        severity: AlertSeverity,
        alert_type: AlertType,
        entity: String,
        description: String,
        timestamp: u64,
        auto_action: Option<AutoAction>,
    ) -> FraudAlert {
        let alert_id = format!("alert_{}_{}", entity, timestamp);

        let alert = FraudAlert {
            alert_id: alert_id.clone(),
            severity: severity.clone(),
            alert_type: alert_type.clone(),
            entity: entity.clone(),
            description: description.clone(),
            evidence: vec![],
            timestamp,
            auto_action: auto_action.clone(),
        };

        // Add to alerts queue
        let mut alerts = self.alerts.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        alerts.push_front(alert.clone());
        drop(alerts);

        // Persist alerts to database
        self.persist_alerts();

        // Log alert
        let severity_str = match severity {
            AlertSeverity::Critical => "CRITICAL",
            AlertSeverity::High => "HIGH",
            AlertSeverity::Medium => "MEDIUM",
            AlertSeverity::Low => "LOW",
        };

        log::warn!("[{}] FRAUD ALERT [{}]", severity_str, alert_id);
        log::warn!("  Entity: {}", entity);
        log::warn!("  Type: {:?}", alert_type);
        log::warn!("  Description: {}", description);
        if let Some(action) = &auto_action {
            log::warn!("  Auto-action: {:?}", action);
        }

        alert
    }

 fn current_timestamp() -> u64 {
 SystemTime::now()
 .duration_since(UNIX_EPOCH)
 .expect("System time is before UNIX epoch")
 .as_secs()
 }
}

/// Monitoring report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringReport {
 pub total_alerts: usize,
 pub critical_alerts: usize,
 pub high_alerts: usize,
 pub total_entities: usize,
 pub blacklisted_entities: usize,
 pub timestamp: u64,
}

impl MonitoringReport {
    pub fn print(&self) {
        log::info!("============================================================");
        log::info!("  FRAUD DETECTION MONITORING REPORT");
        log::info!("============================================================");
        log::info!("Summary:");
        log::info!("  Total Alerts: {}", self.total_alerts);
        log::info!("  Critical: {}", self.critical_alerts);
        log::info!("  High: {}", self.high_alerts);
        log::info!("Entities:");
        log::info!("  Total Monitored: {}", self.total_entities);
        log::info!("  Blacklisted: {}", self.blacklisted_entities);
        log::info!("============================================================");
    }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_monitor_relay_success() {
 let service = FraudDetectionService::new(AlertThresholds::default());

 let alert = service.monitor_relay(
 "relayer1".to_string(),
 1_000_000,
 true,
 1000,
 );

 assert!(alert.is_none());
 }

 #[test]
 fn test_high_failure_rate() {
 let service = FraudDetectionService::new(AlertThresholds::default());

 // Submit 10 failed relays
 for i in 0..10 {
 service.monitor_relay(
 "relayer1".to_string(),
 1_000_000,
 false,
 1000 + i,
 );
 }

 // 11th relay should trigger alert
 let alert = service.monitor_relay(
 "relayer1".to_string(),
 1_000_000,
 false,
 1011,
 );

 assert!(alert.is_some());
 let alert = alert.unwrap();
 assert_eq!(alert.severity, AlertSeverity::High);
 assert!(matches!(alert.alert_type, AlertType::SuspiciousRelayPattern));
 }

 #[test]
 fn test_blacklist() {
 let service = FraudDetectionService::new(AlertThresholds::default());

 service.blacklist_entity(
 "malicious_relayer".to_string(),
 "Repeated fraud attempts".to_string(),
 false,
 1000,
 );

 assert!(service.is_blacklisted("malicious_relayer"));

 let alert = service.monitor_relay(
 "malicious_relayer".to_string(),
 1_000_000,
 true,
 1100,
 );

 assert!(alert.is_some());
 assert_eq!(alert.unwrap().severity, AlertSeverity::Critical);
 }

 #[test]
 fn test_double_spend_detection() {
 let service = FraudDetectionService::new(AlertThresholds::default());

 let transactions = vec![
 (1, 1000),
 (2, 1001),
 (1, 1002), // Duplicate nonce
 ];

 let alert = service.monitor_transactions(
 "user1".to_string(),
 transactions,
 1003,
 );

 assert!(alert.is_some());
 let alert = alert.unwrap();
 assert_eq!(alert.severity, AlertSeverity::Critical);
 assert!(matches!(alert.alert_type, AlertType::DoubleSpendAttempt));
 }

 #[test]
 fn test_monitoring_report() {
 let service = FraudDetectionService::new(AlertThresholds::default());

 // Generate some alerts
 for i in 0..5 {
 service.monitor_relay("relayer1".to_string(), 1_000_000, false, 1000 + i);
 }

 let report = service.generate_report();
 report.print();

 assert!(report.total_entities > 0);
 }
}
