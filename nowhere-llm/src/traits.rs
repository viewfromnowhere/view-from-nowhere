use async_trait::async_trait;
use nowhere_common::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub model: Option<String>,
    pub tokens_used: Option<u32>,
    pub confidence: Option<f64>,
}

#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Configuration error: {0}")]
    Config(String),
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Generate a response to the given prompt with optional system prompt
    async fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<LlmResponse>;

    /// Check if the LLM service is available
    async fn health_check(&self) -> Result<bool>;

    /// Get the model name being used
    fn model_name(&self) -> &str;

    /// Get the default system prompt for nowhere analysis
    fn default_osint_system_prompt(&self) -> &str {
        r#"You are an expert (Open Source Intelligence) analyst with extensive experience in digital investigations, social media analysis, and evidence evaluation.

Your role:
- Analyze evidence for relevance, credibility, and authenticity
- Extract key factual information from various sources
- Provide concise, accurate assessments
- Focus on verifiable facts over speculation
- Consider source reliability and potential biases

Guidelines:
- Be precise and factual in your responses
- Use clear, professional language
- Highlight important details that may be significant for investigation
- Flag potential misinformation or unreliable sources
- Prioritize actionable intelligence"#
    }

    /// Analyze text relevance (specialized for nowhere)
    async fn analyze_relevance(&self, claim: &str, evidence: &str) -> Result<bool> {
        let system_prompt = format!(
            "{}\n\nTask: Determine if the provided evidence is directly relevant to investigating the given claim. Answer ONLY with 'yes' or 'no'.",
            self.default_osint_system_prompt()
        );

        let prompt = format!(
            "CLAIM: \"{}\"\n\nEVIDENCE: \"{}\"\n\nRelevant?",
            claim, evidence
        );

        tracing::info!("Prompt: {}", prompt);
        let response = self
            .generate(&prompt, Some(&system_prompt), Some(10), Some(0.1))
            .await?;
        tracing::debug!("LLM response: {}", response.text);

        Ok(response.text.trim().to_lowercase().contains("yes"))
    }

    /// Extract key information from text
    async fn extract_key_info(&self, text: &str, context: &str) -> Result<Vec<String>> {
        let system_prompt = format!(
            "{}\n\nTask: Extract key factual information from the provided text. Format your response as a bulleted list using '-' prefix.",
            self.default_osint_system_prompt()
        );

        let prompt = format!(
            "CONTEXT: {}\n\nTEXT TO ANALYZE: \"{}\"\n\nExtract key facts:",
            context, text
        );

        let response = self
            .generate(&prompt, Some(&system_prompt), Some(200), Some(0.3))
            .await?;

        // Parse bullet points
        let key_info = response
            .text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('-') || trimmed.starts_with('•') || trimmed.starts_with('*')
                {
                    Some(
                        trimmed
                            .trim_start_matches(['-', '•', '*'])
                            .trim()
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .collect();

        Ok(key_info)
    }

    /// Analyze credibility of a source or piece of information
    async fn analyze_credibility(&self, content: &str, source_info: Option<&str>) -> Result<f64> {
        let system_prompt = format!(
            "{}\n\nTask: Assess the credibility of the provided content on a scale of 0.0 to 1.0, where 0.0 is completely unreliable and 1.0 is highly credible. Consider source authority, factual accuracy, bias indicators, and verification possibilities. Respond with ONLY the numerical score.",
            self.default_osint_system_prompt()
        );

        let prompt = if let Some(source) = source_info {
            format!(
                "SOURCE: {}\n\nCONTENT: \"{}\"\n\nCredibility score:",
                source, content
            )
        } else {
            format!("CONTENT: \"{}\"\n\nCredibility score:", content)
        };

        let response = self
            .generate(&prompt, Some(&system_prompt), Some(20), Some(0.1))
            .await?;

        // Parse the numerical score
        let score = response
            .text
            .trim()
            .split_whitespace()
            .find_map(|word| word.parse::<f64>().ok())
            .unwrap_or(0.5); // Default to neutral if parsing fails

        Ok(score.clamp(0.0, 1.0))
    }

    /// Summarize multiple pieces of evidence
    async fn synthesize_evidence(
        &self,
        evidence_list: &[String],
        investigation_context: &str,
    ) -> Result<String> {
        let system_prompt = format!(
            "{}\n\nTask: Synthesize the provided evidence into a coherent summary for an nowhere investigation. Focus on patterns, corroborating information, and key findings.",
            self.default_osint_system_prompt()
        );

        let evidence_text = evidence_list
            .iter()
            .enumerate()
            .map(|(i, evidence)| format!("{}. {}", i + 1, evidence))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "INVESTIGATION: {}\n\nEVIDENCE TO SYNTHESIZE:\n{}\n\nProvide a synthesis:",
            investigation_context, evidence_text
        );

        let response = self
            .generate(&prompt, Some(&system_prompt), Some(500), Some(0.4))
            .await?;
        Ok(response.text)
    }

    /// Identify potential misinformation or inconsistencies
    async fn detect_inconsistencies(&self, evidence_list: &[String]) -> Result<Vec<String>> {
        let system_prompt = format!(
            "{}\n\nTask: Analyze the provided evidence for potential inconsistencies, contradictions, or signs of misinformation. List any concerns as bullet points with '-' prefix.",
            self.default_osint_system_prompt()
        );

        let evidence_text = evidence_list
            .iter()
            .enumerate()
            .map(|(i, evidence)| format!("{}. {}", i + 1, evidence))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "EVIDENCE TO ANALYZE:\n{}\n\nIdentify inconsistencies or red flags:",
            evidence_text
        );

        let response = self
            .generate(&prompt, Some(&system_prompt), Some(400), Some(0.3))
            .await?;

        // Parse bullet points
        let inconsistencies = response
            .text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('-') || trimmed.starts_with('•') || trimmed.starts_with('*')
                {
                    let cleaned = trimmed
                        .trim_start_matches(['-', '•', '*'])
                        .trim()
                        .to_string();
                    if !cleaned.is_empty() {
                        Some(cleaned)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        Ok(inconsistencies)
    }
}
