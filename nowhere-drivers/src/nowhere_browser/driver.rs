use crate::nowhere_browser::{
    behavioral::BehavioralEngine,
    fingerprint::{FingerprintManager, UserAgentManager},
    page::NowherePage,
    stealth::{build_stealth_arguments, StealthProfile},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use fantoccini::{Client, ClientBuilder};
use nowhere_common::StealthLevel;
use serde_json::json;
use std::collections::HashMap;
use url::Url;
use webdriver::capabilities::Capabilities;

/// Thin wrapper around a `fantoccini` WebDriver client with stealth and
/// behavioral helpers.
pub struct NowhereDriver {
    pub client: Client,
    pub behavioral_engine: BehavioralEngine,
    pub user_agent_manager: UserAgentManager,
    pub stealth_profile: StealthProfile,
}

impl NowhereDriver {
    /// Create a new driver connected to a running WebDriver service.
    ///
    /// Default: connects to `http://localhost:9515` (Chromedriver).
    ///
    /// FIXME(config): respect `NOWHERE_WEBDRIVER_URL` if set to support Gecko
    /// or remote endpoints, aligning docs with behavior.
    pub async fn new(headless: bool, stealth_profile: StealthProfile) -> Result<Self> {
        let mut caps = Capabilities::new();
        let mut chrome_opts = HashMap::new();
        let mut user_agent_manager = UserAgentManager::new();
        let user_agent_profile = user_agent_manager.get_session_profile(&stealth_profile);

        let args = build_stealth_arguments(&stealth_profile, user_agent_profile);
        chrome_opts.insert("args".to_string(), json!(args));

        if headless {
            if let Some(args) = chrome_opts.get_mut("args") {
                if let Some(args_vec) = args.as_array_mut() {
                    args_vec.push(json!("--headless"));
                    args_vec.push(json!("--disable-gpu"));
                }
            }
        }

        caps.insert("goog:chromeOptions".to_string(), json!(chrome_opts));

        let client = ClientBuilder::native()
            .capabilities(caps)
            .connect("http://localhost:9515")
            .await?;

        let behavioral_engine = BehavioralEngine::new();

        Ok(Self {
            client,
            behavioral_engine,
            user_agent_manager,
            stealth_profile,
        })
    }

    /// Navigate to `url` and return a [`NowherePage`] with stealth/fingerprint
    /// scripts applied.
    pub async fn goto(&mut self, url: &str) -> Result<NowherePage> {
        let mut page = NowherePage::new(
            self.client.clone(),
            self.stealth_profile.clone(),
            self.user_agent_manager.clone(),
            self.behavioral_engine.clone(),
        );
        // Navigate via NowherePage so stealth/fingerprint scripts are applied consistently
        page.goto(url).await?;
        Ok(page)
    }

    /// Close the underlying browser session.
    pub async fn close(self) -> Result<()> {
        self.client.close().await?;
        Ok(())
    }
}
