//! Fraud detection and intrusion detection module
//!
//! Provides network-level security monitoring, threat classification,
//! and automated response to suspicious activity on the node.
//! Integrates with the alerts system for webhook notifications.

pub mod api;

use crate::alerts::{Alert, AlertSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Threat type classification for security events
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThreatType {
    /// IP exceeded rate limits
    RateLimitViolation,
    /// Failed authentication attempt
    AuthenticationFailure,
    /// Unusual API access pattern
    SuspiciousActivity,
    /// Invalid transaction submission
    InvalidTransaction,
    /// Peer protocol violation
    ProtocolViolation,
}

impl std::fmt::Display for ThreatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatType::RateLimitViolation => write!(f, "rate_limit_violation"),
            ThreatType::AuthenticationFailure => write!(f, "authentication_failure"),
            ThreatType::SuspiciousActivity => write!(f, "suspicious_activity"),
            ThreatType::InvalidTransaction => write!(f, "invalid_transaction"),
            ThreatType::ProtocolViolation => write!(f, "protocol_violation"),
        }
    }
}

/// Severity level for detected threats
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// A recorded security event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    /// Unique event ID
    pub id: String,
    /// Source IP or peer address
    pub source: String,
    /// Type of threat
    pub threat_type: ThreatType,
    /// Severity level
    pub severity: ThreatSeverity,
    /// Human-readable description
    pub description: String,
    /// When the event was recorded
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Number of occurrences from this source
    pub event_count: usize,
}

/// Active threat tracking
#[derive(Debug, Clone)]
struct ThreatTracker {
    source: String,
    threat_type: ThreatType,
    count: usize,
    first_seen: Instant,
    last_seen: Instant,
}

/// Thresholds for automatic escalation
pub struct EscalationConfig {
    /// Number of events before escalating to alert
    pub alert_threshold: usize,
    /// Number of events before auto-blocking
    pub block_threshold: usize,
    /// Window duration for counting events
    pub window: Duration,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            alert_threshold: 10,
            block_threshold: 50,
            window: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Fraud detection engine
///
/// Monitors security events, tracks threat patterns, and triggers
/// alerts when thresholds are exceeded.
pub struct FraudDetector {
    /// Active threat trackers by source
    threats: Arc<Mutex<HashMap<String, ThreatTracker>>>,
    /// Recent security events
    events: Arc<Mutex<Vec<SecurityEvent>>>,
    /// Escalation configuration
    config: EscalationConfig,
    /// Maximum events to keep in memory
    max_events: usize,
}

impl FraudDetector {
    /// Create a new fraud detector with default config
    pub fn new() -> Self {
        Self::with_config(EscalationConfig::default())
    }

    /// Create a new fraud detector with custom config
    pub fn with_config(config: EscalationConfig) -> Self {
        Self {
            threats: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            config,
            max_events: 10_000,
        }
    }

    /// Record a security event
    pub fn record_event(
        &self,
        source: &str,
        threat_type: ThreatType,
        severity: ThreatSeverity,
        description: &str,
    ) {
        let event = SecurityEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source: source.to_string(),
            threat_type,
            severity,
            description: description.to_string(),
            timestamp: chrono::Utc::now(),
            event_count: 1,
        };

        // Track the threat
        let should_alert = {
            let mut threats = self.threats.lock().unwrap();
            let tracker = threats
                .entry(source.to_string())
                .or_insert(ThreatTracker {
                    source: source.to_string(),
                    threat_type,
                    count: 0,
                    first_seen: Instant::now(),
                    last_seen: Instant::now(),
                });

            // Reset if outside window
            if tracker.first_seen.elapsed() > self.config.window {
                tracker.count = 0;
                tracker.first_seen = Instant::now();
            }

            tracker.count += 1;
            tracker.last_seen = Instant::now();

            tracker.count == self.config.alert_threshold
        };

        // Store event
        {
            let mut events = self.events.lock().unwrap();
            events.push(event);
            if events.len() > self.max_events {
                let excess = events.len() - self.max_events;
                events.drain(..excess);
            }
        }

        // Trigger alert if threshold reached
        if should_alert {
            let alert = Alert::critical(
                "fraud_detection",
                &format!(
                    "Threat threshold reached: {} events from {} (type: {})",
                    self.config.alert_threshold, source, threat_type
                ),
            );
            tokio::spawn(async move {
                crate::alerts::send_alert(alert).await;
            });
        }
    }

    /// Get recent events, optionally filtered by severity
    pub fn get_events(&self, severity_filter: Option<ThreatSeverity>, limit: usize) -> Vec<SecurityEvent> {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .rev()
            .filter(|e| severity_filter.map_or(true, |s| e.severity >= s))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get active threats (sources with recent activity)
    pub fn get_active_threats(&self) -> Vec<(String, ThreatType, usize)> {
        let threats = self.threats.lock().unwrap();
        threats
            .values()
            .filter(|t| t.last_seen.elapsed() < self.config.window)
            .map(|t| (t.source.clone(), t.threat_type, t.count))
            .collect()
    }

    /// Check if a source is currently blocked
    pub fn is_blocked(&self, source: &str) -> bool {
        let threats = self.threats.lock().unwrap();
        threats
            .get(source)
            .map_or(false, |t| {
                t.count >= self.config.block_threshold
                    && t.first_seen.elapsed() < self.config.window
            })
    }
}

impl Default for FraudDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_retrieve_events() {
        let detector = FraudDetector::new();
        detector.record_event(
            "192.168.1.1",
            ThreatType::RateLimitViolation,
            ThreatSeverity::Medium,
            "Too many requests",
        );

        let events = detector.get_events(None, 10);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "192.168.1.1");
    }

    #[tokio::test]
    async fn test_threat_tracking() {
        let config = EscalationConfig {
            alert_threshold: 3,
            block_threshold: 5,
            window: Duration::from_secs(60),
        };
        let detector = FraudDetector::with_config(config);

        for _ in 0..5 {
            detector.record_event(
                "attacker",
                ThreatType::AuthenticationFailure,
                ThreatSeverity::High,
                "Bad auth",
            );
        }

        assert!(detector.is_blocked("attacker"));
        assert!(!detector.is_blocked("innocent"));
    }
}
