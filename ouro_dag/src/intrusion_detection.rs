// src/intrusion_detection.rs
//! Intrusion Detection System (IDS) for Ouroboros
//!
//! Monitors security events and detects patterns of malicious behavior.
//! Alerts operators to potential attacks before they cause damage.
//!
//! Monitored threats:
//! - Failed authentication attempts (brute force attacks)
//! - Rate limit violations (DoS attempts)
//! - Validator misbehavior (slashing events)
//! - Invalid transaction patterns
//! - Suspicious network activity

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Severity levels for security alerts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertSeverity {
 /// Low severity - monitoring only
 Low,
 /// Medium severity - investigate if repeated
 Medium,
 /// High severity - immediate attention needed
 High,
 /// Critical severity - active attack in progress
 Critical,
}

/// Types of security threats
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThreatType {
 /// Failed authentication attempts
 AuthenticationFailure,
 /// Rate limit violations
 RateLimitViolation,
 /// Validator misbehavior (slashing)
 ValidatorMisbehavior,
 /// Invalid transaction submissions
 InvalidTransaction,
 /// Suspicious network patterns
 SuspiciousNetwork,
 /// Multiple failed signature verifications
 SignatureFailure,
 /// Repeated equivocation attempts
 EquivocationAttempt,
}

/// Security alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAlert {
 /// Alert ID
 pub id: uuid::Uuid,
 /// Threat type
 pub threat_type: ThreatType,
 /// Severity level
 pub severity: AlertSeverity,
 /// Source (IP address, validator ID, etc.)
 pub source: String,
 /// Alert timestamp
 pub timestamp: DateTime<Utc>,
 /// Alert description
 pub description: String,
 /// Event count (how many times this pattern occurred)
 pub event_count: u32,
}

/// Threat pattern tracking
struct ThreatPattern {
 /// First occurrence
 first_seen: Instant,
 /// Last occurrence
 last_seen: Instant,
 /// Number of occurrences
 count: u32,
 /// Alert already sent
 alerted: bool,
}

/// Intrusion Detection System
pub struct IntrusionDetectionSystem {
 /// Tracked threat patterns: (source, threat_type) -> pattern
 patterns: Arc<Mutex<HashMap<(String, ThreatType), ThreatPattern>>>,
 /// Alert history
 alerts: Arc<Mutex<Vec<SecurityAlert>>>,
 /// Detection thresholds
 config: IDSConfig,
}

/// IDS configuration
#[derive(Clone)]
pub struct IDSConfig {
 /// Window for pattern detection
 pub detection_window: Duration,
 /// Threshold for failed auth before alert
 pub auth_failure_threshold: u32,
 /// Threshold for rate limit violations
 pub rate_limit_threshold: u32,
 /// Threshold for invalid transactions
 pub invalid_tx_threshold: u32,
 /// Threshold for signature failures
 pub signature_failure_threshold: u32,
}

impl Default for IDSConfig {
 fn default() -> Self {
 Self {
 detection_window: Duration::from_secs(300), // 5 minutes
 auth_failure_threshold: 5,
 rate_limit_threshold: 10,
 invalid_tx_threshold: 20,
 signature_failure_threshold: 3,
 }
 }
}

impl IntrusionDetectionSystem {
 /// Create new IDS with default configuration
 pub fn new() -> Self {
 Self::with_config(IDSConfig::default())
 }

 /// Create new IDS with custom configuration
 pub fn with_config(config: IDSConfig) -> Self {
 Self {
 patterns: Arc::new(Mutex::new(HashMap::new())),
 alerts: Arc::new(Mutex::new(Vec::new())),
 config,
 }
 }

 /// Record a security event
 ///
 /// # Arguments
 /// * `source` - Source identifier (IP, validator ID, etc.)
 /// * `threat_type` - Type of threat detected
 /// * `description` - Event description
 pub fn record_event(&self, source: &str, threat_type: ThreatType, description: &str) {
 let key = (source.to_string(), threat_type.clone());
 let now = Instant::now();

 let mut patterns = self.patterns.lock().unwrap_or_else(|poisoned| {
 log::warn!("IDS patterns mutex poisoned - recovering");
 poisoned.into_inner()
 });

 // Get or create pattern
 let pattern = patterns.entry(key.clone()).or_insert_with(|| ThreatPattern {
 first_seen: now,
 last_seen: now,
 count: 0,
 alerted: false,
 });

 // Update pattern
 pattern.last_seen = now;
 pattern.count += 1;

 // Check if we should raise an alert
 let should_alert = self.should_alert(&threat_type, pattern);

 if should_alert && !pattern.alerted {
 pattern.alerted = true;
 let severity = self.calculate_severity(&threat_type, pattern.count);

 // Create alert
 let alert = SecurityAlert {
 id: uuid::Uuid::new_v4(),
 threat_type: threat_type.clone(),
 severity: severity.clone(),
 source: source.to_string(),
 timestamp: Utc::now(),
 description: format!("{} (count: {})", description, pattern.count),
 event_count: pattern.count,
 };

 // Log alert
 match severity {
 AlertSeverity::Critical => {
 log::error!(
 "CRITICAL: CRITICAL SECURITY ALERT: {:?} from {} - {}",
 threat_type, source, description
 );
 }
 AlertSeverity::High => {
 log::error!(
 "WARNING HIGH SECURITY ALERT: {:?} from {} - {}",
 threat_type, source, description
 );
 }
 AlertSeverity::Medium => {
 log::warn!(
 "WARNING MEDIUM SECURITY ALERT: {:?} from {} - {}",
 threat_type, source, description
 );
 }
 AlertSeverity::Low => {
 log::info!(
 "INFO:LOW SECURITY ALERT: {:?} from {} - {}",
 threat_type, source, description
 );
 }
 }

 // Store alert
 let mut alerts = self.alerts.lock().unwrap_or_else(|poisoned| {
 log::warn!("IDS alerts mutex poisoned - recovering");
 poisoned.into_inner()
 });
 alerts.push(alert);

 // Keep only last 1000 alerts
 if alerts.len() > 1000 {
 alerts.drain(0..alerts.len() - 1000);
 }
 }

 // Clean up old patterns
 self.cleanup_old_patterns();
 }

 /// Check if an alert should be raised
 fn should_alert(&self, threat_type: &ThreatType, pattern: &ThreatPattern) -> bool {
 // Check if within detection window
 if pattern.last_seen.duration_since(pattern.first_seen) > self.config.detection_window {
 return false;
 }

 // Check threat-specific thresholds
 match threat_type {
 ThreatType::AuthenticationFailure => {
 pattern.count >= self.config.auth_failure_threshold
 }
 ThreatType::RateLimitViolation => {
 pattern.count >= self.config.rate_limit_threshold
 }
 ThreatType::InvalidTransaction => {
 pattern.count >= self.config.invalid_tx_threshold
 }
 ThreatType::SignatureFailure => {
 pattern.count >= self.config.signature_failure_threshold
 }
 ThreatType::ValidatorMisbehavior | ThreatType::EquivocationAttempt => {
 // Alert on first occurrence for validator misbehavior
 pattern.count >= 1
 }
 ThreatType::SuspiciousNetwork => {
 pattern.count >= 5
 }
 }
 }

 /// Calculate alert severity based on threat type and count
 fn calculate_severity(&self, threat_type: &ThreatType, count: u32) -> AlertSeverity {
 match threat_type {
 ThreatType::EquivocationAttempt | ThreatType::ValidatorMisbehavior => {
 // Validator misbehavior is always critical
 AlertSeverity::Critical
 }
 ThreatType::SignatureFailure => {
 if count >= 10 {
 AlertSeverity::Critical
 } else if count >= 5 {
 AlertSeverity::High
 } else {
 AlertSeverity::Medium
 }
 }
 ThreatType::AuthenticationFailure => {
 if count >= 20 {
 AlertSeverity::Critical
 } else if count >= 10 {
 AlertSeverity::High
 } else {
 AlertSeverity::Medium
 }
 }
 ThreatType::RateLimitViolation => {
 if count >= 50 {
 AlertSeverity::High
 } else if count >= 20 {
 AlertSeverity::Medium
 } else {
 AlertSeverity::Low
 }
 }
 ThreatType::InvalidTransaction => {
 if count >= 50 {
 AlertSeverity::High
 } else {
 AlertSeverity::Medium
 }
 }
 ThreatType::SuspiciousNetwork => {
 if count >= 20 {
 AlertSeverity::High
 } else {
 AlertSeverity::Medium
 }
 }
 }
 }

 /// Clean up patterns outside the detection window
 fn cleanup_old_patterns(&self) {
 let mut patterns = match self.patterns.lock() {
 Ok(p) => p,
 Err(_) => return, // Skip cleanup if mutex is poisoned
 };

 let now = Instant::now();
 patterns.retain(|_, pattern| {
 now.duration_since(pattern.last_seen) < self.config.detection_window * 2
 });
 }

 /// Get recent alerts
 pub fn get_recent_alerts(&self, limit: usize) -> Vec<SecurityAlert> {
 let alerts = self.alerts.lock().unwrap_or_else(|poisoned| {
 log::warn!("IDS alerts mutex poisoned - recovering");
 poisoned.into_inner()
 });

 let start = if alerts.len() > limit {
 alerts.len() - limit
 } else {
 0
 };

 alerts[start..].to_vec()
 }

 /// Get alerts by severity
 pub fn get_alerts_by_severity(&self, severity: AlertSeverity) -> Vec<SecurityAlert> {
 let alerts = self.alerts.lock().unwrap_or_else(|poisoned| {
 log::warn!("IDS alerts mutex poisoned - recovering");
 poisoned.into_inner()
 });

 alerts
 .iter()
 .filter(|a| a.severity == severity)
 .cloned()
 .collect()
 }

 /// Get active threats
 pub fn get_active_threats(&self) -> Vec<(String, ThreatType, u32)> {
 let patterns = self.patterns.lock().unwrap_or_else(|poisoned| {
 log::warn!("IDS patterns mutex poisoned - recovering");
 poisoned.into_inner()
 });

 let now = Instant::now();
 patterns
 .iter()
 .filter(|(_, pattern)| {
 now.duration_since(pattern.last_seen) < self.config.detection_window
 })
 .map(|((source, threat_type), pattern)| {
 (source.clone(), threat_type.clone(), pattern.count)
 })
 .collect()
 }

 /// Clear all patterns and alerts (for testing)
 #[cfg(test)]
 pub fn clear(&self) {
 if let Ok(mut patterns) = self.patterns.lock() {
 patterns.clear();
 }
 if let Ok(mut alerts) = self.alerts.lock() {
 alerts.clear();
 }
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_auth_failure_detection() {
 let ids = IntrusionDetectionSystem::new();

 // Record 4 failures - should not alert
 for _ in 0..4 {
 ids.record_event("192.168.1.100", ThreatType::AuthenticationFailure, "Invalid API key");
 }

 let alerts = ids.get_recent_alerts(10);
 assert_eq!(alerts.len(), 0);

 // 5th failure should trigger alert
 ids.record_event("192.168.1.100", ThreatType::AuthenticationFailure, "Invalid API key");

 let alerts = ids.get_recent_alerts(10);
 assert_eq!(alerts.len(), 1);
 assert_eq!(alerts[0].threat_type, ThreatType::AuthenticationFailure);
 assert_eq!(alerts[0].event_count, 5);
 }

 #[test]
 fn test_validator_misbehavior_immediate_alert() {
 let ids = IntrusionDetectionSystem::new();

 // First equivocation attempt should immediately alert
 ids.record_event("validator-1", ThreatType::EquivocationAttempt, "Double vote detected");

 let alerts = ids.get_recent_alerts(10);
 assert_eq!(alerts.len(), 1);
 assert_eq!(alerts[0].severity, AlertSeverity::Critical);
 }

 #[test]
 fn test_severity_escalation() {
 let ids = IntrusionDetectionSystem::new();

 // Trigger initial alert at threshold
 for _ in 0..5 {
 ids.record_event("192.168.1.100", ThreatType::SignatureFailure, "Invalid signature");
 }

 let alerts = ids.get_recent_alerts(10);
 assert_eq!(alerts.len(), 1);
 assert_eq!(alerts[0].severity, AlertSeverity::Medium);
 }

 #[test]
 fn test_active_threats() {
 let ids = IntrusionDetectionSystem::new();

 ids.record_event("192.168.1.100", ThreatType::AuthenticationFailure, "Test");
 ids.record_event("192.168.1.101", ThreatType::RateLimitViolation, "Test");

 let active = ids.get_active_threats();
 assert!(active.len() >= 2);
 }
}
