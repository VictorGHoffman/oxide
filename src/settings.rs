//! Settings that change how requests are sent.

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Seconds to wait for a response; 0 = no limit.
    pub timeout_secs: u64,
    /// Check the server's TLS certificate (turn off only for servers you trust).
    pub verify_tls: bool,
    pub follow_redirects: bool,
}

impl Default for Settings {
    fn default() -> Self {
        DEFAULT
    }
}

const DEFAULT: Settings = Settings {
    timeout_secs: 120,
    verify_tls: true,
    follow_redirects: true,
};

/// The longest timeout that can be typed.
pub const MAX_TIMEOUT: u64 = 3600;

static ACTIVE: RwLock<Settings> = RwLock::new(DEFAULT);

pub fn set_active(settings: &Settings) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = settings.clone();
    }
}

pub fn current() -> Settings {
    ACTIVE.read().map(|s| s.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_the_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"verify_tls":false}"#).unwrap();
        assert!(!settings.verify_tls);
        assert_eq!(settings.timeout_secs, 120);
        assert!(settings.follow_redirects);
    }
}
