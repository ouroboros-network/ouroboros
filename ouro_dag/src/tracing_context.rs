// src/tracing_context.rs
//! Distributed Tracing with Correlation IDs
//!
//! Provides request tracing across the distributed system with unique
//! correlation IDs that propagate through all operations.
//!
//! Features:
//! - Unique trace ID generation (UUID v4)
//! - Thread-local context storage
//! - HTTP header propagation
//! - Structured logging integration
//! - Parent-child span relationships

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Trace context for a request or operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
 /// Unique trace ID for the entire request chain
 pub trace_id: String,

 /// Span ID for this specific operation
 pub span_id: String,

 /// Parent span ID (if this is a child operation)
 pub parent_span_id: Option<String>,

 /// Service name
 pub service_name: String,

 /// Operation name (e.g., "submit_transaction", "verify_signature")
 pub operation: String,

 /// Start timestamp (Unix millis)
 pub start_time_ms: u64,

 /// Additional metadata
 pub metadata: HashMap<String, String>,
}

impl TraceContext {
 /// Create a new root trace context
 pub fn new(service_name: &str, operation: &str) -> Self {
 let trace_id = Uuid::new_v4().to_string();
 let span_id = Uuid::new_v4().to_string();

 Self {
 trace_id,
 span_id,
 parent_span_id: None,
 service_name: service_name.to_string(),
 operation: operation.to_string(),
 start_time_ms: current_time_ms(),
 metadata: HashMap::new(),
 }
 }

 /// Create a child span from an existing trace context
 pub fn child_span(&self, operation: &str) -> Self {
 Self {
 trace_id: self.trace_id.clone(),
 span_id: Uuid::new_v4().to_string(),
 parent_span_id: Some(self.span_id.clone()),
 service_name: self.service_name.clone(),
 operation: operation.to_string(),
 start_time_ms: current_time_ms(),
 metadata: HashMap::new(),
 }
 }

 /// Parse trace context from HTTP headers
 pub fn from_headers(headers: &HashMap<String, String>, operation: &str) -> Self {
 if let Some(trace_id) = headers.get("x-trace-id") {
 // Continue existing trace
 Self {
 trace_id: trace_id.clone(),
 span_id: Uuid::new_v4().to_string(),
 parent_span_id: headers.get("x-span-id").cloned(),
 service_name: "ouro_dag".to_string(),
 operation: operation.to_string(),
 start_time_ms: current_time_ms(),
 metadata: HashMap::new(),
 }
 } else {
 // Start new trace
 Self::new("ouro_dag", operation)
 }
 }

 /// Export trace context as HTTP headers
 pub fn to_headers(&self) -> HashMap<String, String> {
 let mut headers = HashMap::new();
 headers.insert("x-trace-id".to_string(), self.trace_id.clone());
 headers.insert("x-span-id".to_string(), self.span_id.clone());
 if let Some(parent) = &self.parent_span_id {
 headers.insert("x-parent-span-id".to_string(), parent.clone());
 }
 headers
 }

 /// Add metadata to the trace
 pub fn add_metadata(&mut self, key: &str, value: &str) {
 self.metadata.insert(key.to_string(), value.to_string());
 }

 /// Get duration since span started
 pub fn duration_ms(&self) -> u64 {
 current_time_ms().saturating_sub(self.start_time_ms)
 }

 /// Log the trace context
 pub fn log(&self, level: LogLevel, message: &str) {
 let duration_ms = self.duration_ms();

 match level {
 LogLevel::Error => log::error!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Warn => log::warn!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Info => log::info!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Debug => log::debug!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 }
 }

 /// Log an event with additional context
 pub fn log_event(&self, level: LogLevel, event: &str, context: &HashMap<String, String>) {
 let duration_ms = self.duration_ms();
 let context_str = context
 .iter()
 .map(|(k, v)| format!("{}={}", k, v))
 .collect::<Vec<_>>()
 .join(" ");

 let message = format!("{} {}", event, context_str);

 match level {
 LogLevel::Error => log::error!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Warn => log::warn!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Info => log::info!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 LogLevel::Debug => log::debug!(
 "[trace_id={} span_id={} operation={} duration_ms={}] {}",
 self.trace_id,
 self.span_id,
 self.operation,
 duration_ms,
 message
 ),
 }
 }
}

/// Log level for trace events
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
 Error,
 Warn,
 Info,
 Debug,
}

/// Thread-local storage for current trace context
thread_local! {
 static CURRENT_TRACE: std::cell::RefCell<Option<TraceContext>> = std::cell::RefCell::new(None);
}

/// Set the current trace context for this thread
pub fn set_trace_context(ctx: TraceContext) {
 CURRENT_TRACE.with(|current| {
 *current.borrow_mut() = Some(ctx);
 });
}

/// Get the current trace context (if any)
pub fn get_trace_context() -> Option<TraceContext> {
 CURRENT_TRACE.with(|current| current.borrow().clone())
}

/// Clear the current trace context
pub fn clear_trace_context() {
 CURRENT_TRACE.with(|current| {
 *current.borrow_mut() = None;
 });
}

/// Execute a function with a trace context
pub fn with_trace<F, R>(ctx: TraceContext, f: F) -> R
where
 F: FnOnce() -> R,
{
 set_trace_context(ctx.clone());
 let result = f();
 clear_trace_context();
 result
}

/// Execute an async function with a trace context
pub async fn with_trace_async<F, R>(ctx: TraceContext, f: F) -> R
where
 F: std::future::Future<Output = R>,
{
 set_trace_context(ctx.clone());
 let result = f.await;
 clear_trace_context();
 result
}

/// Log a message with the current trace context
pub fn trace_log(level: LogLevel, message: &str) {
 if let Some(ctx) = get_trace_context() {
 ctx.log(level, message);
 } else {
 // No trace context - log without trace info
 match level {
 LogLevel::Error => log::error!("{}", message),
 LogLevel::Warn => log::warn!("{}", message),
 LogLevel::Info => log::info!("{}", message),
 LogLevel::Debug => log::debug!("{}", message),
 }
 }
}

/// Create a child span from the current trace context
pub fn child_span(operation: &str) -> Option<TraceContext> {
 get_trace_context().map(|ctx| ctx.child_span(operation))
}

/// Trace span guard (automatically logs duration on drop)
pub struct SpanGuard {
 context: TraceContext,
 log_on_drop: bool,
}

impl SpanGuard {
 /// Create a new span guard
 pub fn new(context: TraceContext) -> Self {
 context.log(LogLevel::Debug, "Span started");
 set_trace_context(context.clone());

 Self {
 context,
 log_on_drop: true,
 }
 }

 /// Get the trace context
 pub fn context(&self) -> &TraceContext {
 &self.context
 }

 /// Disable automatic logging on drop
 pub fn disable_auto_log(mut self) -> Self {
 self.log_on_drop = false;
 self
 }
}

impl Drop for SpanGuard {
 fn drop(&mut self) {
 if self.log_on_drop {
 self.context.log(
 LogLevel::Debug,
 &format!("Span completed (duration: {}ms)", self.context.duration_ms()),
 );
 }
 clear_trace_context();
 }
}

/// Get current timestamp in milliseconds
fn current_time_ms() -> u64 {
 std::time::SystemTime::now()
 .duration_since(std::time::UNIX_EPOCH)
 .expect("System time before UNIX epoch")
 .as_millis() as u64
}

/// Macro for creating a traced span
#[macro_export]
macro_rules! traced_span {
 ($operation:expr) => {
 if let Some(parent) = $crate::tracing_context::get_trace_context() {
 $crate::tracing_context::SpanGuard::new(parent.child_span($operation))
 } else {
 $crate::tracing_context::SpanGuard::new($crate::tracing_context::TraceContext::new(
 "ouro_dag",
 $operation,
 ))
 }
 };
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn test_trace_context_creation() {
 let ctx = TraceContext::new("test_service", "test_operation");

 assert_eq!(ctx.service_name, "test_service");
 assert_eq!(ctx.operation, "test_operation");
 assert!(ctx.parent_span_id.is_none());
 assert!(!ctx.trace_id.is_empty());
 assert!(!ctx.span_id.is_empty());
 }

 #[test]
 fn test_child_span() {
 let parent = TraceContext::new("test_service", "parent_op");
 let child = parent.child_span("child_op");

 assert_eq!(child.trace_id, parent.trace_id); // Same trace
 assert_ne!(child.span_id, parent.span_id); // Different span
 assert_eq!(child.parent_span_id, Some(parent.span_id.clone()));
 assert_eq!(child.operation, "child_op");
 }

 #[test]
 fn test_header_propagation() {
 let ctx = TraceContext::new("test_service", "test_op");
 let headers = ctx.to_headers();

 assert!(headers.contains_key("x-trace-id"));
 assert!(headers.contains_key("x-span-id"));
 assert_eq!(headers.get("x-trace-id").unwrap(), &ctx.trace_id);
 }

 #[test]
 fn test_thread_local_context() {
 let ctx = TraceContext::new("test_service", "test_op");
 set_trace_context(ctx.clone());

 let retrieved = get_trace_context();
 assert!(retrieved.is_some());
 assert_eq!(retrieved.unwrap().trace_id, ctx.trace_id);

 clear_trace_context();
 assert!(get_trace_context().is_none());
 }
}
