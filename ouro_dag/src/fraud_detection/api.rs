//! API endpoints for fraud detection / security monitoring
//!
//! Provides REST endpoints to query security events, active threats,
//! and manage blocking rules.

use super::{FraudDetector, ThreatSeverity};
use axum::extract::Extension;
use axum::response::{IntoResponse, Json};
use axum::http::StatusCode;
use std::sync::Arc;

/// GET /security/events - List recent security events
pub async fn get_security_events(
    Extension(detector): Extension<Arc<FraudDetector>>,
) -> impl IntoResponse {
    let events = detector.get_events(None, 100);
    (StatusCode::OK, Json(serde_json::json!({ "events": events })))
}

/// GET /security/threats - List active threats
pub async fn get_active_threats(
    Extension(detector): Extension<Arc<FraudDetector>>,
) -> impl IntoResponse {
    let threats = detector.get_active_threats();
    let threat_list: Vec<serde_json::Value> = threats
        .iter()
        .map(|(source, threat_type, count)| {
            serde_json::json!({
                "source": source,
                "threat_type": format!("{}", threat_type),
                "event_count": count,
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!({ "active_threats": threat_list })))
}

/// GET /security/alerts - List high-severity events
pub async fn get_security_alerts(
    Extension(detector): Extension<Arc<FraudDetector>>,
) -> impl IntoResponse {
    let alerts = detector.get_events(Some(ThreatSeverity::High), 50);
    (StatusCode::OK, Json(serde_json::json!({ "alerts": alerts })))
}
