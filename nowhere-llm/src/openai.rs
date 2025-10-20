use crate::traits::{LlmClient, LlmResponse};
use async_trait::async_trait;
use nowhere_common::{NowhereError, Result};
use nowhere_http::{HttpClient, HttpError};
use serde::{Deserialize, Serialize};

const OPENAI_API_BASE: &str = "https://api.openai.com/v1/";

pub struct OpenAiClient {
    client: HttpClient,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
pub struct ResponsesApiRequest {
    model: String,
    input: String,
    instructions: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub instructions: Option<String>,
    pub model: String,
    #[serde(default)]
    pub output: Vec<ResponseMessage>,
}

/// One element in the `output` array
#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: Option<String>,
    #[serde(default)]
    pub content: Vec<ResponseContent>,
}

/// One part of the message `content`
#[derive(Debug, Deserialize)]
pub struct ResponseContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: String,
}

impl OpenAiClient {
    /// Create a new client for the given API key and model.
    ///
    /// FIXME(timeout/retry): add per-request timeouts/backoff knobs and consider
    /// integrating the `RateLimiter` actor at the call sites to avoid provider
    /// throttling issues under load.
    pub fn new(api_key: String, model: String) -> Result<Self> {
        let client = HttpClient::new(OPENAI_API_BASE)
            .map_err(|e| NowhereError::Agent(format!("HttpClient init failed: {e}")))?;

        Ok(Self {
            client,
            api_key,
            model,
        })
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<LlmResponse> {
        tracing::debug!("==============OPENAI CLIENT GENERATE WAS CALLED================");

        let instructions = match system_prompt {
            Some(s) => s.to_string(),
            None => "You are an objective, unbiased researcher.".to_string(),
        };

        let req = ResponsesApiRequest {
            model: self.model.clone(),
            input: prompt.to_string(),
            instructions,
        };

        let resp: ResponsesApiResponse = self
            .client
            .post_json("responses", Some(&self.api_key), &req)
            .await
            .map_err(http_to_nowhere)?;

        let text = resp
            .output
            .iter()
            .flat_map(|msg| &msg.content)
            .find(|c| c.kind == "output_text")
            .map(|c| c.text.clone())
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            model: Some(resp.model),
            confidence: None,
            tokens_used: None,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple health check by trying to generate a minimal response
        // FIXME(health): enforce a short timeout here to avoid lingering tasks
        // during startup checks.
        let test_prompt = "Respond with just 'OK'";

        match self.generate(test_prompt, None, Some(5), Some(0.1)).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("OpenAi health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

fn http_to_nowhere(e: HttpError) -> NowhereError {
    NowhereError::Agent(format!("{e}"))
}
