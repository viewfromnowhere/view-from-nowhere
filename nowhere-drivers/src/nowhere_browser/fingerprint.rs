use crate::nowhere_browser::stealth::StealthProfile;
use rand::prelude::SliceRandom;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Snapshot of user agent, viewport, and locale characteristics.
pub struct UserAgentProfile {
    pub user_agent: String,
    pub viewport: (u32, u32),
    pub platform: String,
    pub languages: Vec<String>,
    pub timezone: String,
}

#[derive(Debug, Clone)]
/// Maintains a small pool of plausible desktop fingerprint profiles.
pub struct UserAgentManager {
    desktop_profiles: Vec<UserAgentProfile>,
    current_session_profile: Option<UserAgentProfile>,
}

impl UserAgentManager {
    /// Create a new manager with built‑in desktop profiles.
    pub fn new() -> Self {
        Self {
            desktop_profiles: vec![
                UserAgentProfile {
                    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
                    viewport: (1920, 1080),
                    platform: "Win32".to_string(),
                    languages: vec!["en-US".to_string(),"en".to_string()],
                    timezone: "America/New_York".to_string(),
                },
                UserAgentProfile {
                    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".to_string(),
                    viewport: (1440, 900),
                    platform: "MacIntel".to_string(),
                    languages: vec!["en-US".to_string(),"en".to_string()],
                    timezone: "America/Los_Angeles".to_string(),
                },
            ],
            current_session_profile: None,
        }
    }

    /// Get (or lazily select) the current session profile.
    pub fn get_session_profile(&mut self, _: &StealthProfile) -> &UserAgentProfile {
        if self.current_session_profile.is_none() {
            let mut rng = rand::thread_rng();
            let p = self.desktop_profiles.choose(&mut rng).unwrap().clone();
            self.current_session_profile = Some(p);
        }
        self.current_session_profile.as_ref().unwrap()
    }
}

#[derive(Debug, Clone)]
/// Placeholder for more advanced, per‑session fingerprint controls.
pub struct FingerprintManager {}

impl FingerprintManager {
    /// Create a new fingerprint manager.
    pub fn new() -> Self {
        Self {}
    }
}
