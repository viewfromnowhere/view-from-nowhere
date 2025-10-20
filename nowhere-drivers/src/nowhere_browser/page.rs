use crate::nowhere_browser::{
    behavioral::BehavioralEngine,
    fingerprint::UserAgentManager,
    stealth::{StealthProfile, StealthScripts},
};
use anyhow::{anyhow, Result};
use fantoccini::{elements::Element, Client, Locator};
use nowhere_llm::traits::LlmClient;
use serde_json;
use tracing::info;

/// High‑level page wrapper providing element queries and LLM‑assisted
/// selector discovery.
pub struct NowherePage {
    pub(crate) client: Client,
    pub(crate) stealth_profile: StealthProfile,
    pub(crate) fingerprint_manager: UserAgentManager,
    pub(crate) behavioral_engine: BehavioralEngine,
}

impl NowherePage {
    /// Construct a page wrapper around an existing WebDriver client.
    pub fn new(
        client: Client,
        stealth_profile: StealthProfile,
        fingerprint_manager: UserAgentManager,
        behavioral_engine: BehavioralEngine,
    ) -> Self {
        Self {
            client,
            stealth_profile,
            fingerprint_manager,
            behavioral_engine,
        }
    }

    /// Navigate to `url` and apply stealth/fingerprint scripts.
    pub async fn goto(&mut self, url: &str) -> Result<()> {
        self.behavioral_engine.random_delay(300, 1200).await;
        self.client.goto(url).await.map_err(anyhow::Error::from)?;

        self.apply_stealth_and_fingerprint().await?;

        Ok(())
    }

    /// Apply stealth scripts and basic fingerprinting adjustments.
    async fn apply_stealth_and_fingerprint(&mut self) -> Result<()> {
        self.client
            .execute(StealthScripts::get_core_evasions(), vec![])
            .await?;

        match self.stealth_profile {
            StealthProfile::Lightweight => {
                // No additional scripts for the lightest profile
            }

            StealthProfile::Balanced => {
                self.client
                    .execute(StealthScripts::get_canvas_evasions(), vec![])
                    .await?;
            }

            StealthProfile::Maximum => {
                self.client
                    .execute(StealthScripts::get_canvas_evasions(), vec![])
                    .await?;
                self.client
                    .execute(StealthScripts::get_webgl_evasions(), vec![])
                    .await?;

                let p = &self
                    .fingerprint_manager
                    .get_session_profile(&self.stealth_profile);

                self.client
                    .execute(
                        &format!(
                            "Object.defineProperty(navigator, 'platform', {{ get: () => '{}' }});",
                            p.platform
                        ),
                        vec![],
                    )
                    .await?;
            }
        }
        Ok(())
    }

    /// Return the full page HTML source.
    pub async fn get_content(&self) -> Result<String> {
        self.client.source().await.map_err(anyhow::Error::msg)
    }

    /// Return the page title.
    pub async fn get_title(&self) -> Result<String> {
        self.client.title().await.map_err(anyhow::Error::msg)
    }

    /// Find a single element by CSS selector.
    pub async fn find_element(&self, selector: &str) -> Result<NowhereElement> {
        self.behavioral_engine.random_delay(100, 500).await;

        let element = self
            .client
            .wait()
            .for_element(Locator::Css(selector))
            .await?;
        Ok(NowhereElement::new(element, &self.behavioral_engine))
    }

    /// Find an element by CSS selector, falling back to an LLM‑derived selector.
    pub async fn find_element_robust(
        &self,
        selector: &str,
        llm_query: &str,
        llm_client: &(dyn LlmClient + Send + Sync),
    ) -> Result<NowhereElement> {
        match self.find_element(selector).await {
            Ok(el) => Ok(el),
            Err(_) => {
                let sel = self.get_selector_from_llm(llm_query, llm_client).await?;
                self.find_element(&sel).await
            }
        }
    }

    /// Find zero or more elements by CSS selector.
    pub async fn find_elements(&self, selector: &str) -> Result<Vec<NowhereElement>> {
        let elements = self.client.find_all(Locator::Css(selector)).await?;

        Ok(elements
            .into_iter()
            .map(|element| NowhereElement::new(element, &self.behavioral_engine))
            .collect())
    }

    /// Find elements by selector, falling back to an LLM‑derived selector.
    pub async fn find_elements_robust(
        &self,
        selector: &str,
        llm_query: &str,
        llm_client: &(dyn LlmClient + Send + Sync),
    ) -> Result<Vec<NowhereElement>> {
        match self.find_elements(selector).await {
            Ok(elements) if !elements.is_empty() => Ok(elements),
            _ => {
                info!(
                    target: "browser.selector",
                    %selector,
                    "initial selector failed; requesting LLM rewrite"
                );
                let sel = self.get_selector_from_llm(llm_query, llm_client).await?;
                info!(
                    target: "browser.selector",
                    selector = %sel,
                    "LLM provided replacement selector"
                );

                self.find_elements(&sel).await
            }
        }
    }

    /// Ask an LLM for a CSS selector given a natural‑language query and return the first match.
    pub async fn find_element_by_llm(
        &self,
        query: &str,
        llm_client: &(dyn LlmClient + Send + Sync),
    ) -> Result<NowhereElement> {
        let sel = self.get_selector_from_llm(query, llm_client).await?;
        self.find_element(&sel).await
    }

    /// Return the current page URL.
    pub async fn get_url(&self) -> Result<String> {
        self.client
            .current_url()
            .await
            .map(|url| url.to_string())
            .map_err(anyhow::Error::msg)
    }

    async fn get_selector_from_llm(
        &self,
        query: &str,
        llm_client: &(dyn LlmClient + Send + Sync),
    ) -> Result<String> {
        let prompt = serde_json::to_string(&serde_json::json!({
            "task": "analyze_html_for_selector",
            "query": query,
            "html_content": self.get_content().await?,
        }))?;

        let sys = r#"
            Your task is to analyze the provided HTML and return a CSS selector based on the user's query.
            Your response must be a single JSON object with one key, "selector".
            If a selector is found, the value must be the CSS selector string.
            If no selector is found, the value must be null.
            Do not provide any other text, explanation, or markdown.
            "#;
        let response = llm_client
            .generate(&prompt, Some(sys), Some(2500), Some(0.0))
            .await?;
        let val: serde_json::Value = serde_json::from_str(&response.text)?;
        val.get("selector")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("No selector found"))
    }
}

// =========================
// NowhereElement Definition
// =========================

#[derive(Clone)]
/// Wrapper for DOM elements that provides typed helpers consistent with [`NowherePage`].
pub struct NowhereElement {
    pub element: Element,
    pub behavioral_engine: BehavioralEngine,
}

impl NowhereElement {
    /// Construct an element wrapper.
    pub fn new(element: Element, behavioral: &BehavioralEngine) -> Self {
        Self {
            element,
            behavioral_engine: behavioral.clone(),
        }
    }

    /// Type into the element using human‑like timings.
    pub async fn type_str(&self, text: &str) -> Result<()> {
        self.behavioral_engine
            .type_text_human_like(&self.element, text)
            .await
    }

    /// Find a child element by CSS selector.
    pub async fn find_element(&self, selector: &str) -> Result<NowhereElement> {
        let element = self.element.find(Locator::Css(selector)).await?;
        Ok(NowhereElement::new(element, &self.behavioral_engine))
    }

    /// Find zero or more child elements by CSS selector.
    pub async fn find_elements(&self, selector: &str) -> Result<Vec<NowhereElement>> {
        let elements = self.element.find_all(Locator::Css(selector)).await?;
        Ok(elements
            .into_iter()
            .map(|element| NowhereElement::new(element, &self.behavioral_engine))
            .collect())
    }

    /// Return the element's inner HTML.
    pub async fn get_inner_html(&self) -> Result<String> {
        self.element.html(true).await.map_err(anyhow::Error::from)
    }

    /// Read an attribute value.
    pub async fn get_attribute(&self, attribute: &str) -> Result<Option<String>> {
        self.element
            .attr(attribute)
            .await
            .map_err(anyhow::Error::from)
    }

    /// Return the element's visible text.
    pub async fn get_inner_text(&self) -> Result<String> {
        self.element.text().await.map_err(anyhow::Error::from)
    }
}
