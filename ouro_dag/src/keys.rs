// src/keys.rs
use std::fs;

/// Load a secret by checking, in order:
/// 1) Docker secret file at /run/secrets/<name>
/// 2) Environment variable <name>
///
/// Returns `Some(String)` if found, otherwise `None`.
pub fn load_secret(name: &str) -> Option<String> {
    // Docker secrets path
    let secret_path = format!("/run/secrets/{}", name);
    if let Ok(s) = fs::read_to_string(&secret_path) {
        let s = s.trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }

    // Fallback to environment variable
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}
