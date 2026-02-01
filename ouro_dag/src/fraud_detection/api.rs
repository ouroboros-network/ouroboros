//! Fraud Detection API Endpoints
//!
//! HTTP endpoints for querying fraud detection status and reports.

use axum::{
 extract::State,
 http::StatusCode,
 response::Json,
 routing::{get, post},
 Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{FraudDetectionService, AlertSeverity, MonitoringReport};

/// Fraud detection API state
#[derive(Clone)]
pub struct FraudApiState {
 pub fraud_service: Arc<FraudDetectionService>,
}

/// Create fraud detection API router
pub fn fraud_detection_routes(fraud_service: Arc<FraudDetectionService>) -> Router {
 let state = FraudApiState { fraud_service };

 Router::new()
 .route("/fraud/status", get(get_fraud_status))
 .route("/fraud/report", get(get_fraud_report))
 .route("/fraud/alerts", get(get_recent_alerts))
 .route("/fraud/alerts/critical", get(get_critical_alerts))
 .route("/fraud/blacklist/:entity", get(check_blacklist))
 .route("/fraud/blacklist/:entity", post(add_to_blacklist))
 .route("/fraud/stats/:entity", get(get_entity_stats))
 .with_state(state)
}

/// Fraud detection status response
#[derive(Serialize)]
struct FraudStatusResponse {
 enabled: bool,
 version: String,
 uptime_seconds: u64,
 total_alerts: usize,
 critical_alerts: usize,
 blacklisted_entities: usize,
}

/// Get fraud detection system status
async fn get_fraud_status(
 State(state): State<FraudApiState>,
) -> Result<Json<FraudStatusResponse>, StatusCode> {
 let report = state.fraud_service.generate_report();

 Ok(Json(FraudStatusResponse {
 enabled: true,
 version: "v0.4.0".to_string(),
 uptime_seconds: 0, // TODO: Track actual uptime
 total_alerts: report.total_alerts,
 critical_alerts: report.critical_alerts,
 blacklisted_entities: report.blacklisted_entities,
 }))
}

/// Get monitoring report
async fn get_fraud_report(
 State(state): State<FraudApiState>,
) -> Result<Json<MonitoringReport>, StatusCode> {
 let report = state.fraud_service.generate_report();
 Ok(Json(report))
}

/// Alert response
#[derive(Serialize)]
struct AlertResponse {
 alert_id: String,
 severity: String,
 alert_type: String,
 entity: String,
 description: String,
 timestamp: u64,
 auto_action: Option<String>,
}

/// Get recent alerts
async fn get_recent_alerts(
 State(state): State<FraudApiState>,
) -> Result<Json<Vec<AlertResponse>>, StatusCode> {
 let alerts = state.fraud_service.get_recent_alerts(50);

 let response: Vec<AlertResponse> = alerts
 .into_iter()
 .map(|alert| AlertResponse {
 alert_id: alert.alert_id,
 severity: format!("{:?}", alert.severity),
 alert_type: format!("{:?}", alert.alert_type),
 entity: alert.entity,
 description: alert.description,
 timestamp: alert.timestamp,
 auto_action: alert.auto_action.map(|a| format!("{:?}", a)),
 })
 .collect();

 Ok(Json(response))
}

/// Get critical alerts only
async fn get_critical_alerts(
 State(state): State<FraudApiState>,
) -> Result<Json<Vec<AlertResponse>>, StatusCode> {
 let alerts = state.fraud_service.get_alerts_by_severity(AlertSeverity::Critical);

 let response: Vec<AlertResponse> = alerts
 .into_iter()
 .map(|alert| AlertResponse {
 alert_id: alert.alert_id,
 severity: format!("{:?}", alert.severity),
 alert_type: format!("{:?}", alert.alert_type),
 entity: alert.entity,
 description: alert.description,
 timestamp: alert.timestamp,
 auto_action: alert.auto_action.map(|a| format!("{:?}", a)),
 })
 .collect();

 Ok(Json(response))
}

/// Blacklist check response
#[derive(Serialize)]
struct BlacklistResponse {
 entity: String,
 is_blacklisted: bool,
}

/// Check if entity is blacklisted
async fn check_blacklist(
 State(state): State<FraudApiState>,
 axum::extract::Path(entity): axum::extract::Path<String>,
) -> Result<Json<BlacklistResponse>, StatusCode> {
 let is_blacklisted = state.fraud_service.is_blacklisted(&entity);

 Ok(Json(BlacklistResponse {
 entity,
 is_blacklisted,
 }))
}

/// Blacklist request
#[derive(Deserialize)]
struct BlacklistRequest {
 reason: String,
 permanent: bool,
}

/// Add entity to blacklist
async fn add_to_blacklist(
 State(state): State<FraudApiState>,
 axum::extract::Path(entity): axum::extract::Path<String>,
 Json(request): Json<BlacklistRequest>,
) -> Result<StatusCode, StatusCode> {
 state.fraud_service.blacklist_entity(
 entity,
 request.reason,
 request.permanent,
 current_timestamp(),
 );

 Ok(StatusCode::OK)
}

/// Entity statistics response
#[derive(Serialize)]
struct EntityStatsResponse {
 entity: String,
 total_relays: u64,
 successful_relays: u64,
 failed_relays: u64,
 total_volume: u64,
 success_rate: f64,
}

/// Get entity statistics
async fn get_entity_stats(
 State(state): State<FraudApiState>,
 axum::extract::Path(entity): axum::extract::Path<String>,
) -> Result<Json<EntityStatsResponse>, StatusCode> {
 if let Some((total, successful, failed, volume)) = state.fraud_service.get_activity_stats(&entity) {
 let success_rate = if total > 0 {
 (successful as f64 / total as f64) * 100.0
 } else {
 0.0
 };

 Ok(Json(EntityStatsResponse {
 entity,
 total_relays: total,
 successful_relays: successful,
 failed_relays: failed,
 total_volume: volume,
 success_rate,
 }))
 } else {
 Err(StatusCode::NOT_FOUND)
 }
}

fn current_timestamp() -> u64 {
 use std::time::{SystemTime, UNIX_EPOCH};
 SystemTime::now()
 .duration_since(UNIX_EPOCH)
 .unwrap()
 .as_secs()
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::fraud_detection::AlertThresholds;
 use axum::http::Request;
 use tower::ServiceExt;

 #[tokio::test]
 async fn test_fraud_status_endpoint() {
 let fraud_service = Arc::new(FraudDetectionService::new(AlertThresholds::default()));
 let app = fraud_detection_routes(fraud_service);

 let response = app
 .oneshot(
 Request::builder()
 .uri("/fraud/status")
 .body(axum::body::Body::empty())
 .unwrap(),
 )
 .await
 .unwrap();

 assert_eq!(response.status(), StatusCode::OK);
 }

 #[tokio::test]
 async fn test_fraud_report_endpoint() {
 let fraud_service = Arc::new(FraudDetectionService::new(AlertThresholds::default()));
 let app = fraud_detection_routes(fraud_service);

 let response = app
 .oneshot(
 Request::builder()
 .uri("/fraud/report")
 .body(axum::body::Body::empty())
 .unwrap(),
 )
 .await
 .unwrap();

 assert_eq!(response.status(), StatusCode::OK);
 }
}
