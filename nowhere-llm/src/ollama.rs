use crate::traits::{LlmClient, LlmResponse};
use async_trait::async_trait;
use nowhere_common::{NowhereError, Result};
use serde_json::{json, Value as JsonValue};
use std::time::Duration;

const OLLAMA_CONNECTION_ERROR: &str = "No running Ollama server detected. Start it with: `ollama serve` (after installing). Install instructions: https://github.com/ollama/ollama";

/// Ollama client for local model inference.
///
/// Expects a running Ollama server (see https://github.com/ollama/ollama).
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    /// Create a new client and verify server/model availability.
    pub async fn new(base_url: String, model: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| NowhereError::Agent(format!("Failed to create HTTP client: {}", e)))?;

        let ollama_client = Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
        };

        // Verify server is reachable
        ollama_client.probe_server().await?;

        // Ensure model is available
        ollama_client.ensure_model_available().await?;

        Ok(ollama_client)
    }

    async fn probe_server(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|_| NowhereError::Agent(OLLAMA_CONNECTION_ERROR.to_string()))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(NowhereError::Agent(OLLAMA_CONNECTION_ERROR.to_string()))
        }
    }

    async fn ensure_model_available(&self) -> Result<()> {
        let models = self.fetch_available_models().await?;

        if !models.contains(&self.model) {
            tracing::info!("Model {} not found locally, pulling...", self.model);
            self.pull_model(&self.model).await?;
        }

        Ok(())
    }

    async fn fetch_available_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| NowhereError::Agent(format!("Failed to fetch models: {}", e)))?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let val: JsonValue = resp
            .json()
            .await
            .map_err(|e| NowhereError::Agent(format!("Failed to parse models response: {}", e)))?;

        let models = val
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }

    async fn pull_model(&self, model: &str) -> Result<()> {
        let url = format!("{}/api/pull", self.base_url);
        let payload = json!({
            "model": model,
            "stream": false
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NowhereError::Agent(format!("Failed to pull model: {}", e)))?;

        if resp.status().is_success() {
            tracing::info!("Successfully pulled model: {}", model);
            Ok(())
        } else {
            Err(NowhereError::Agent(format!(
                "Failed to pull model: HTTP {}",
                resp.status()
            )))
        }
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    async fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<LlmResponse> {
        let url = format!("{}/api/generate", self.base_url);

        let mut options = serde_json::Map::new();
        if let Some(temp) = temperature {
            options.insert("temperature".to_string(), json!(temp));
        }
        if let Some(max_tok) = max_tokens {
            options.insert("num_predict".to_string(), json!(max_tok));
        }

        // Combine system prompt with user prompt for Ollama
        let full_prompt = if let Some(sys_prompt) = system_prompt {
            format!("{}\n\nUser: {}\n\nAssistant:", sys_prompt, prompt)
        } else {
            prompt.to_string()
        };

        let payload = json!({
            "model": self.model,
            "prompt": full_prompt,
            "stream": false,
            "options": options
        });
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NowhereError::Agent(format!("Generate request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(NowhereError::Agent(format!(
                "Generate failed: HTTP {}",
                resp.status()
            )));
        }

        let val: JsonValue = resp
            .json()
            .await
            .map_err(|e| NowhereError::Agent(format!("Failed to parse response: {}", e)))?;

        let text = val
            .get("response")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .to_string();

        let tokens_used = val
            .get("eval_count")
            .and_then(|c| c.as_u64())
            .map(|c| c as u32);

        Ok(LlmResponse {
            text,
            model: Some(self.model.clone()),
            tokens_used,
            confidence: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        self.probe_server().await.map(|_| true).or(Ok(false))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
