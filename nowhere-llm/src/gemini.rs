use crate::traits::{LlmClient, LlmError, LlmResponse};
use async_trait::async_trait;
use nowhere_common::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::time::Duration;

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    safety_settings: Option<Vec<GeminiSafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
}

#[derive(Debug, Serialize)]
struct GeminiSafetySetting {
    category: String,
    threshold: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

/// Google Gemini API client.
///
/// Requires a valid API key and internet access.
pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    /// Create a new client using the provided API key and model.
    pub fn new(api_key: String, model: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| {
                nowhere_common::NowhereError::Agent(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            client,
            api_key,
            model,
        })
    }

    fn create_safety_settings() -> Vec<GeminiSafetySetting> {
        vec![
            GeminiSafetySetting {
                category: "HARM_CATEGORY_HARASSMENT".to_string(),
                threshold: "BLOCK_MEDIUM_AND_ABOVE".to_string(),
            },
            GeminiSafetySetting {
                category: "HARM_CATEGORY_HATE_SPEECH".to_string(),
                threshold: "BLOCK_MEDIUM_AND_ABOVE".to_string(),
            },
            GeminiSafetySetting {
                category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(),
                threshold: "BLOCK_MEDIUM_AND_ABOVE".to_string(),
            },
            GeminiSafetySetting {
                category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(),
                threshold: "BLOCK_MEDIUM_AND_ABOVE".to_string(),
            },
        ]
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<LlmResponse> {
        let url = format!("{}/models/{}:generateContent", GEMINI_BASE_URL, self.model);

        let generation_config = if max_tokens.is_some() || temperature.is_some() {
            Some(GeminiGenerationConfig {
                temperature,
                max_output_tokens: max_tokens,
                top_p: None,
                top_k: None,
            })
        } else {
            None
        };

        // Handle system instruction (Gemini's system prompt)
        let system_instruction = system_prompt.map(|sys_prompt| GeminiSystemInstruction {
            parts: vec![GeminiPart {
                text: sys_prompt.to_string(),
            }],
        });

        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: prompt.to_string(),
                }],
            }],
            generation_config,
            safety_settings: Some(Self::create_safety_settings()),
            system_instruction,
        };

        tracing::debug!("Sending Gemini request to: {}", url);

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .query(&[("key", &self.api_key)])
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                nowhere_common::NowhereError::Agent(format!("Gemini request failed: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();

            return Err(match status.as_u16() {
                429 => nowhere_common::NowhereError::Agent("Rate limit exceeded".to_string()),
                401 => nowhere_common::NowhereError::Agent("Invalid API key".to_string()),
                403 => nowhere_common::NowhereError::Agent("API access forbidden".to_string()),
                _ => nowhere_common::NowhereError::Agent(format!(
                    "Gemini API error ({}): {}",
                    status, error_text
                )),
            });
        }

        let gemini_response: GeminiResponse = resp.json().await.map_err(|e| {
            nowhere_common::NowhereError::Agent(format!("Failed to parse Gemini response: {}", e))
        })?;

        if gemini_response.candidates.is_empty() {
            return Err(nowhere_common::NowhereError::Agent(
                "No candidates returned from Gemini".to_string(),
            ));
        }

        let candidate = &gemini_response.candidates[0];

        // Check for safety blocks
        if let Some(finish_reason) = &candidate.finish_reason {
            if finish_reason == "SAFETY" {
                return Err(nowhere_common::NowhereError::Agent(
                    "Content blocked by Gemini safety filters".to_string(),
                ));
            }
        }

        if candidate.content.parts.is_empty() {
            return Err(nowhere_common::NowhereError::Agent(
                "No content parts in Gemini response".to_string(),
            ));
        }

        let text = candidate.content.parts[0].text.clone();
        let tokens_used = gemini_response
            .usage_metadata
            .and_then(|u| u.total_token_count);

        Ok(LlmResponse {
            text,
            model: Some(self.model.clone()),
            tokens_used,
            confidence: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Simple health check by trying to generate a minimal response
        let test_prompt = "Respond with just 'OK'";

        match self.generate(test_prompt, None, Some(5), Some(0.1)).await {
            Ok(_) => Ok(true),
            Err(e) => {
                tracing::warn!("Gemini health check failed: {}", e);
                Ok(false)
            }
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
