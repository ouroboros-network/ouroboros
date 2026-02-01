// src/metrics.rs
//! Prometheus-compatible metrics for monitoring
//!
//! Exposes metrics in Prometheus format for scraping by monitoring systems.
//! Tracks:
//! - HTTP request metrics (count, latency, status codes)
//! - Security events (auth failures, rate limits, slashing)
//! - System health (mempool size, validator count, etc.)

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crate::storage::RocksDb;

/// Metrics collector
pub struct Metrics {
 // HTTP metrics
 pub http_requests_total: Arc<AtomicU64>,
 pub http_requests_failed: Arc<AtomicU64>,
 pub http_request_duration_ms: Arc<AtomicU64>, // Sum of durations

 // Security metrics
 pub auth_failures_total: Arc<AtomicU64>,
 pub rate_limit_violations_total: Arc<AtomicU64>,
 pub slashing_events_total: Arc<AtomicU64>,
 pub invalid_tx_total: Arc<AtomicU64>,

 // System metrics
 pub mempool_size: Arc<AtomicU64>,
 pub active_validators: Arc<AtomicU64>,
 pub total_transactions: Arc<AtomicU64>,

 // Database reference (for future persistence)
 #[allow(dead_code)]
 db: RocksDb,
}

impl Metrics {
 /// Create new metrics collector
 pub fn new(db: RocksDb) -> Self {
 Self {
 http_requests_total: Arc::new(AtomicU64::new(0)),
 http_requests_failed: Arc::new(AtomicU64::new(0)),
 http_request_duration_ms: Arc::new(AtomicU64::new(0)),

 auth_failures_total: Arc::new(AtomicU64::new(0)),
 rate_limit_violations_total: Arc::new(AtomicU64::new(0)),
 slashing_events_total: Arc::new(AtomicU64::new(0)),
 invalid_tx_total: Arc::new(AtomicU64::new(0)),

 mempool_size: Arc::new(AtomicU64::new(0)),
 active_validators: Arc::new(AtomicU64::new(0)),
 total_transactions: Arc::new(AtomicU64::new(0)),

 db,
 }
 }

 /// Increment HTTP request counter
 pub fn inc_http_requests(&self) {
 self.http_requests_total.fetch_add(1, Ordering::Relaxed);
 }

 /// Increment failed HTTP request counter
 pub fn inc_http_failures(&self) {
 self.http_requests_failed.fetch_add(1, Ordering::Relaxed);
 }

 /// Record HTTP request duration
 pub fn record_request_duration(&self, duration_ms: u64) {
 self.http_request_duration_ms.fetch_add(duration_ms, Ordering::Relaxed);
 }

 /// Increment auth failure counter
 pub fn inc_auth_failures(&self) {
 self.auth_failures_total.fetch_add(1, Ordering::Relaxed);
 }

 /// Increment rate limit violation counter
 pub fn inc_rate_limit_violations(&self) {
 self.rate_limit_violations_total.fetch_add(1, Ordering::Relaxed);
 }

 /// Increment slashing events counter
 pub fn inc_slashing_events(&self) {
 self.slashing_events_total.fetch_add(1, Ordering::Relaxed);
 }

 /// Increment invalid transaction counter
 pub fn inc_invalid_tx(&self) {
 self.invalid_tx_total.fetch_add(1, Ordering::Relaxed);
 }

 /// Update mempool size
 pub fn set_mempool_size(&self, size: u64) {
 self.mempool_size.store(size, Ordering::Relaxed);
 }

 /// Update active validators count
 pub fn set_active_validators(&self, count: u64) {
 self.active_validators.store(count, Ordering::Relaxed);
 }

 /// Increment total transactions
 pub fn inc_total_transactions(&self) {
 self.total_transactions.fetch_add(1, Ordering::Relaxed);
 }

 /// Export metrics in Prometheus format
 pub fn export_prometheus(&self) -> String {
 let mut output = String::new();

 // HTTP metrics
 output.push_str("# HELP http_requests_total Total number of HTTP requests\n");
 output.push_str("# TYPE http_requests_total counter\n");
 output.push_str(&format!(
 "http_requests_total {}\n",
 self.http_requests_total.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP http_requests_failed Total number of failed HTTP requests\n");
 output.push_str("# TYPE http_requests_failed counter\n");
 output.push_str(&format!(
 "http_requests_failed {}\n",
 self.http_requests_failed.load(Ordering::Relaxed)
 ));

 let total_requests = self.http_requests_total.load(Ordering::Relaxed);
 let total_duration = self.http_request_duration_ms.load(Ordering::Relaxed);
 let avg_latency = if total_requests > 0 {
 total_duration as f64 / total_requests as f64
 } else {
 0.0
 };

 output.push_str("# HELP http_request_duration_ms_avg Average HTTP request duration in milliseconds\n");
 output.push_str("# TYPE http_request_duration_ms_avg gauge\n");
 output.push_str(&format!("http_request_duration_ms_avg {:.2}\n", avg_latency));

 // Security metrics
 output.push_str("# HELP auth_failures_total Total number of authentication failures\n");
 output.push_str("# TYPE auth_failures_total counter\n");
 output.push_str(&format!(
 "auth_failures_total {}\n",
 self.auth_failures_total.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP rate_limit_violations_total Total number of rate limit violations\n");
 output.push_str("# TYPE rate_limit_violations_total counter\n");
 output.push_str(&format!(
 "rate_limit_violations_total {}\n",
 self.rate_limit_violations_total.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP slashing_events_total Total number of validator slashing events\n");
 output.push_str("# TYPE slashing_events_total counter\n");
 output.push_str(&format!(
 "slashing_events_total {}\n",
 self.slashing_events_total.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP invalid_tx_total Total number of invalid transactions\n");
 output.push_str("# TYPE invalid_tx_total counter\n");
 output.push_str(&format!(
 "invalid_tx_total {}\n",
 self.invalid_tx_total.load(Ordering::Relaxed)
 ));

 // System metrics
 output.push_str("# HELP mempool_size Current number of transactions in mempool\n");
 output.push_str("# TYPE mempool_size gauge\n");
 output.push_str(&format!(
 "mempool_size {}\n",
 self.mempool_size.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP active_validators Number of active validators\n");
 output.push_str("# TYPE active_validators gauge\n");
 output.push_str(&format!(
 "active_validators {}\n",
 self.active_validators.load(Ordering::Relaxed)
 ));

 output.push_str("# HELP total_transactions_processed Total number of transactions processed\n");
 output.push_str("# TYPE total_transactions_processed counter\n");
 output.push_str(&format!(
 "total_transactions_processed {}\n",
 self.total_transactions.load(Ordering::Relaxed)
 ));

 output
 }
}

impl Default for Metrics {
 fn default() -> Self {
 Self::new()
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_metrics_increment() {
 let metrics = Metrics::new();

 assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 0);

 metrics.inc_http_requests();
 assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 1);

 metrics.inc_http_requests();
 assert_eq!(metrics.http_requests_total.load(Ordering::Relaxed), 2);
 }

 #[test]
 fn test_metrics_set() {
 let metrics = Metrics::new();

 metrics.set_mempool_size(42);
 assert_eq!(metrics.mempool_size.load(Ordering::Relaxed), 42);

 metrics.set_active_validators(10);
 assert_eq!(metrics.active_validators.load(Ordering::Relaxed), 10);
 }

 #[test]
 fn test_prometheus_export() {
 let metrics = Metrics::new();

 metrics.inc_http_requests();
 metrics.inc_auth_failures();
 metrics.set_mempool_size(100);

 let output = metrics.export_prometheus();

 assert!(output.contains("http_requests_total 1"));
 assert!(output.contains("auth_failures_total 1"));
 assert!(output.contains("mempool_size 100"));
 assert!(output.contains("# HELP"));
 assert!(output.contains("# TYPE"));
 }

 #[test]
 fn test_average_latency() {
 let metrics = Metrics::new();

 metrics.inc_http_requests();
 metrics.record_request_duration(100);

 metrics.inc_http_requests();
 metrics.record_request_duration(200);

 let output = metrics.export_prometheus();

 // Average should be 150ms
 assert!(output.contains("http_request_duration_ms_avg 150.00"));
 }
}
