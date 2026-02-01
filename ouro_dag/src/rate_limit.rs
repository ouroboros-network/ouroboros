// src/rate_limit.rs
//! Rate limiting middleware for API endpoints
//!
//! Prevents DoS attacks by limiting requests per IP address.

use axum::{
 extract::{Request, ConnectInfo},
 http::StatusCode,
 middleware::Next,
 response::{Response, IntoResponse},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Rate limiter configuration
#[derive(Clone)]
pub struct RateLimiterConfig {
 /// Maximum requests per window
 pub max_requests: usize,

 /// Time window duration
 pub window_duration: Duration,

 /// Whether to enable rate limiting
 pub enabled: bool,
}

impl Default for RateLimiterConfig {
 fn default() -> Self {
 Self {
 max_requests: 100,
 window_duration: Duration::from_secs(60),
 enabled: true,
 }
 }
}

impl RateLimiterConfig {
 /// Create configuration from environment variables
 pub fn from_env() -> Self {
 let enabled = std::env::var("RATE_LIMIT_ENABLED")
 .unwrap_or_else(|_| "true".to_string())
 .parse()
 .unwrap_or(true);

 let max_requests = std::env::var("RATE_LIMIT_MAX_REQUESTS")
 .ok()
 .and_then(|s| s.parse().ok())
 .unwrap_or(100);

 let window_secs = std::env::var("RATE_LIMIT_WINDOW_SECS")
 .ok()
 .and_then(|s| s.parse().ok())
 .unwrap_or(60);

 Self {
 max_requests,
 window_duration: Duration::from_secs(window_secs),
 enabled,
 }
 }
}

/// Request tracking entry
#[derive(Debug, Clone)]
struct RequestEntry {
 /// Number of requests in current window
 count: usize,

 /// Window start time
 window_start: Instant,
}

/// Rate limiter state
pub struct RateLimiter {
 /// Configuration
 config: RateLimiterConfig,

 /// Request counts per IP address
 entries: Arc<Mutex<HashMap<SocketAddr, RequestEntry>>>,
}

impl RateLimiter {
 /// Create new rate limiter with configuration
 pub fn new(config: RateLimiterConfig) -> Self {
 let limiter = Self {
 config,
 entries: Arc::new(Mutex::new(HashMap::new())),
 };

 // Spawn cleanup task to remove expired entries
 limiter.spawn_cleanup_task();

 limiter
 }

 /// Create rate limiter from environment variables
 pub fn from_env() -> Self {
 Self::new(RateLimiterConfig::from_env())
 }

 /// Check if request is allowed for given IP
 async fn is_allowed(&self, addr: SocketAddr) -> bool {
 if !self.config.enabled {
 return true;
 }

 let mut entries = self.entries.lock().await;
 let now = Instant::now();

 let entry = entries.entry(addr).or_insert_with(|| RequestEntry {
 count: 0,
 window_start: now,
 });

 // Check if window expired
 if now.duration_since(entry.window_start) > self.config.window_duration {
 // Reset window
 entry.count = 1;
 entry.window_start = now;
 return true;
 }

 // Increment count
 entry.count += 1;

 // Check if limit exceeded
 if entry.count > self.config.max_requests {
 log::warn!(
 "CRITICAL: RATE LIMIT EXCEEDED: {} ({} requests in {} seconds)",
 addr,
 entry.count,
 self.config.window_duration.as_secs()
 );
 false
 } else {
 log::debug!(
 "Rate limit check: {} ({}/{} requests)",
 addr,
 entry.count,
 self.config.max_requests
 );
 true
 }
 }

 /// Spawn background task to clean up expired entries
 fn spawn_cleanup_task(&self) {
 let entries = Arc::clone(&self.entries);
 let window_duration = self.config.window_duration;

 tokio::spawn(async move {
 loop {
 // Sleep for cleanup interval (2x window duration)
 tokio::time::sleep(window_duration * 2).await;

 // Remove expired entries
 let mut map = entries.lock().await;
 let now = Instant::now();

 map.retain(|addr, entry| {
 let expired = now.duration_since(entry.window_start) > window_duration * 2;
 if expired {
 log::debug!("Cleaning up expired rate limit entry for {}", addr);
 }
 !expired
 });

 log::debug!("Rate limiter cleanup: {} active entries", map.len());
 }
 });
 }

 /// Get current statistics
 pub async fn get_stats(&self) -> RateLimiterStats {
 let entries = self.entries.lock().await;

 RateLimiterStats {
 enabled: self.config.enabled,
 max_requests: self.config.max_requests,
 window_secs: self.config.window_duration.as_secs(),
 active_ips: entries.len(),
 }
 }
}

/// Rate limiter statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimiterStats {
 pub enabled: bool,
 pub max_requests: usize,
 pub window_secs: u64,
 pub active_ips: usize,
}

/// Axum middleware for rate limiting
pub async fn rate_limit_middleware(
 ConnectInfo(addr): ConnectInfo<SocketAddr>,
 request: Request,
 next: Next,
) -> Result<Response, StatusCode> {
 // Get rate limiter from request extensions
 let limiter = request
 .extensions()
 .get::<Arc<RateLimiter>>()
 .cloned();

 if let Some(limiter) = limiter {
 if !limiter.is_allowed(addr).await {
 return Err(StatusCode::TOO_MANY_REQUESTS);
 }
 }

 Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn test_rate_limiter_basic() {
 let config = RateLimiterConfig {
 max_requests: 5,
 window_duration: Duration::from_secs(60),
 enabled: true,
 };

 let limiter = RateLimiter::new(config);
 let addr = "127.0.0.1:8000".parse().unwrap();

 // First 5 requests should be allowed
 for i in 1..=5 {
 assert!(
 limiter.is_allowed(addr).await,
 "Request {} should be allowed",
 i
 );
 }

 // 6th request should be denied
 assert!(
 !limiter.is_allowed(addr).await,
 "Request 6 should be denied"
 );
 }

 #[tokio::test]
 async fn test_rate_limiter_disabled() {
 let config = RateLimiterConfig {
 max_requests: 5,
 window_duration: Duration::from_secs(60),
 enabled: false,
 };

 let limiter = RateLimiter::new(config);
 let addr = "127.0.0.1:8000".parse().unwrap();

 // All requests should be allowed when disabled
 for _ in 1..=100 {
 assert!(limiter.is_allowed(addr).await);
 }
 }

 #[tokio::test]
 async fn test_rate_limiter_window_reset() {
 let config = RateLimiterConfig {
 max_requests: 2,
 window_duration: Duration::from_millis(100),
 enabled: true,
 };

 let limiter = RateLimiter::new(config);
 let addr = "127.0.0.1:8000".parse().unwrap();

 // Use up limit
 assert!(limiter.is_allowed(addr).await);
 assert!(limiter.is_allowed(addr).await);
 assert!(!limiter.is_allowed(addr).await);

 // Wait for window to expire
 tokio::time::sleep(Duration::from_millis(150)).await;

 // Should be allowed again
 assert!(limiter.is_allowed(addr).await);
 }
}
