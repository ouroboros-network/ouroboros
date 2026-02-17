// src/alerts.rs
// Simple alerting system for critical failures

use log::{error, warn};
use serde::Serialize;
use std::env;

#[derive(Serialize, Clone)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub component: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Critical,
    Error,
    Warning,
}

impl Alert {
    pub fn critical(component: &str, message: &str) -> Self {
        Self {
            severity: AlertSeverity::Critical,
            component: component.to_string(),
            message: message.to_string(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }
}

/// Send alert to configured monitoring/alerting systems
///
/// Supports multiple alert destinations:
/// 1. Structured logging (always enabled)
/// 2. Webhook POST (if ALERT_WEBHOOK_URL configured)
/// 3. Email (if ALERT_EMAIL configured - future)
///
/// Environment variables:
/// - ALERT_WEBHOOK_URL: POST alerts as JSON to this URL
/// - ALERT_ENABLED: Set to "false" to disable alerts (default: true)
pub async fn send_alert(alert: Alert) {
    // Check if alerts are enabled
    if env::var("ALERT_ENABLED")
        .unwrap_or_else(|_| "true".to_string())
        .to_lowercase()
        == "false"
    {
        return;
    }

    // Always log to stderr with structured format
    let severity_str = match alert.severity {
        AlertSeverity::Critical => "CRITICAL",
        AlertSeverity::Error => "ERROR",
        AlertSeverity::Warning => "WARNING",
    };

    error!(
        "CRITICAL: ALERT [{}] {}: {} {}",
        severity_str,
        alert.component,
        alert.message,
        alert.details.as_deref().unwrap_or("")
    );

    // Send to webhook if configured
    if let Ok(webhook_url) = env::var("ALERT_WEBHOOK_URL") {
        send_webhook_alert(&webhook_url, &alert).await;
    }

    // Future: Email, Slack, PagerDuty integrations can be added here
}

async fn send_webhook_alert(url: &str, alert: &Alert) {
    use reqwest::Client;

    let client = Client::new();
    match client
        .post(url)
        .json(&alert)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => {
            if !response.status().is_success() {
                warn!(
                    "WARNING Alert webhook returned non-success status: {}",
                    response.status()
                );
            }
        }
        Err(e) => {
            warn!("WARNING Failed to send alert webhook: {}", e);
        }
    }
}
