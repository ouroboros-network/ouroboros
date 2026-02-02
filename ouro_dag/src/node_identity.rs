use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::Path;

/// Represents a unique node identity in the Ouroboros network
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeIdentity {
    /// Unique node number (0-999,999)
    pub node_number: u64,

    /// UUID-based node identifier
    pub node_id: String,

    /// Timestamp when node first joined network
    pub first_joined: String,

    /// Optional human-readable name for the node
    pub public_name: Option<String>,

    /// Total uptime in seconds (updated periodically)
    #[serde(default)]
    pub total_uptime_secs: u64,

    /// Last time node was started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_started: Option<String>,
}

impl NodeIdentity {
    /// Load existing identity from file, or create a new one
    pub fn load_or_create(path: &Path) -> Result<Self, Box<dyn Error>> {
        if path.exists() {
            let json = fs::read_to_string(path)?;
            let mut identity: NodeIdentity = serde_json::from_str(&json)?;

            // Update last_started timestamp
            identity.last_started = Some(Utc::now().to_rfc3339());
            identity.save(path)?;

            Ok(identity)
        } else {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let identity = Self::generate_new();
            identity.save(path)?;
            Ok(identity)
        }
    }

    /// Generate a new node identity
    fn generate_new() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            node_number: Self::assign_node_number(),
            node_id: uuid::Uuid::new_v4().to_string(),
            first_joined: now.clone(),
            public_name: None,
            total_uptime_secs: 0,
            last_started: Some(now),
        }
    }

    /// Assign a node number based on deterministic hash
    /// This ensures the same machine gets the same number (roughly)
    fn assign_node_number() -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Try to use machine-specific identifiers for consistency
        let machine_id = Self::get_machine_identifier();

        let mut hasher = DefaultHasher::new();
        machine_id.hash(&mut hasher);

        // Generate number in range 0-999,999
        hasher.finish() % 1_000_000
    }

    /// Get a machine-specific identifier (best effort)
    fn get_machine_identifier() -> String {
        // Use whoami to get hostname (fallible API for better error handling)
        whoami::fallible::hostname().unwrap_or_else(|_| "unknown-host".to_string())
    }

    /// Save identity to disk
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Update the node's public name
    pub fn set_public_name(&mut self, name: String, path: &Path) -> Result<(), Box<dyn Error>> {
        self.public_name = Some(name);
        self.save(path)?;
        Ok(())
    }

    /// Update total uptime
    pub fn update_uptime(
        &mut self,
        additional_secs: u64,
        path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        self.total_uptime_secs += additional_secs;
        self.save(path)?;
        Ok(())
    }

    /// Get formatted uptime as human-readable string
    pub fn formatted_uptime(&self) -> String {
        let days = self.total_uptime_secs / 86400;
        let hours = (self.total_uptime_secs % 86400) / 3600;
        let minutes = (self.total_uptime_secs % 3600) / 60;

        if days > 0 {
            format!("{}d {}h {}m", days, hours, minutes)
        } else if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        }
    }

    /// Get short node ID (first 8 characters)
    pub fn short_id(&self) -> String {
        self.node_id.chars().take(8).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_create_new_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("node_identity.json");

        let identity = NodeIdentity::load_or_create(&path).unwrap();

        assert!(identity.node_number < 1_000_000);
        assert!(!identity.node_id.is_empty());
        assert!(!identity.first_joined.is_empty());
        assert_eq!(identity.public_name, None);
        assert_eq!(identity.total_uptime_secs, 0);
    }

    #[test]
    fn test_load_existing_identity() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("node_identity.json");

        // Create first
        let identity1 = NodeIdentity::load_or_create(&path).unwrap();
        let node_num = identity1.node_number;
        let node_id = identity1.node_id.clone();

        // Load existing
        let identity2 = NodeIdentity::load_or_create(&path).unwrap();

        assert_eq!(identity2.node_number, node_num);
        assert_eq!(identity2.node_id, node_id);
    }

    #[test]
    fn test_set_public_name() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("node_identity.json");

        let mut identity = NodeIdentity::load_or_create(&path).unwrap();
        identity
            .set_public_name("Alice's Node".to_string(), &path)
            .unwrap();

        // Reload and verify
        let reloaded = NodeIdentity::load_or_create(&path).unwrap();
        assert_eq!(reloaded.public_name, Some("Alice's Node".to_string()));
    }

    #[test]
    fn test_formatted_uptime() {
        let identity = NodeIdentity {
            node_number: 42,
            node_id: "test".to_string(),
            first_joined: "2025-01-01".to_string(),
            public_name: None,
            total_uptime_secs: 90061, // 1 day, 1 hour, 1 minute, 1 second
            last_started: None,
        };

        let formatted = identity.formatted_uptime();
        assert_eq!(formatted, "1d 1h 1m");
    }
}
