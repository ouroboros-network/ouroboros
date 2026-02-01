//! Alert Management and Notification System

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Alert notification system
pub struct AlertNotifier {
 channels: Vec<NotificationChannel>,
 pending_notifications: VecDeque<Notification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationChannel {
 /// Log to console
 Console,
 /// Send email
 Email { address: String },
 /// Webhook callback
 Webhook { url: String },
 /// SMS notification
 Sms { phone: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
 pub alert_id: String,
 pub message: String,
 pub priority: u8,
 pub timestamp: u64,
}

impl AlertNotifier {
 pub fn new() -> Self {
 Self {
 channels: vec![NotificationChannel::Console],
 pending_notifications: VecDeque::new(),
 }
 }

 pub fn add_channel(&mut self, channel: NotificationChannel) {
 self.channels.push(channel);
 }

 pub fn send_notification(&mut self, notification: Notification) {
 for channel in &self.channels {
 match channel {
 NotificationChannel::Console => {
 println!(" NOTIFICATION: {}", notification.message);
 }
 NotificationChannel::Email { address } => {
 println!(" Email to {}: {}", address, notification.message);
 }
 NotificationChannel::Webhook { url } => {
 println!(" Webhook to {}: {}", url, notification.message);
 }
 NotificationChannel::Sms { phone } => {
 println!(" SMS to {}: {}", phone, notification.message);
 }
 }
 }

 self.pending_notifications.push_back(notification);
 }
}

impl Default for AlertNotifier {
 fn default() -> Self {
 Self::new()
 }
}
