//! Provider‑agnostic LLM integration for Nowhere.
//!
//! This crate exposes a common [`traits::LlmClient`] interface and concrete
//! provider implementations for Ollama, OpenAI, and Gemini. It also provides
//! a convenience function to initialize a client from a [`nowhere_common::LlmConfig`].
//!
//! # Examples
//! ```no_run
//! use nowhere_common::{LlmConfig, Result};
//! use nowhere_llm::ensure_llm_ready;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let cfg = LlmConfig::None; // or provider variant under appropriate features
//! let client = ensure_llm_ready(&cfg).await?;
//! assert!(!client.model_name().is_empty());
//! # Ok(())
//! # }
//! ```
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod traits;
pub mod verifier;

use gemini::GeminiClient;
use nowhere_common::{LlmConfig, NowhereError};
use ollama::OllamaClient;
use openai::OpenAiClient;
use std::sync::Arc;
use traits::LlmClient;

/// Default model recommendations for nowhere tasks
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.2:3b";
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-1.5-flash";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";

/// Ensure an LLM client is ready (e.g., downloading models if needed).
pub async fn ensure_llm_ready(
    config: &LlmConfig,
) -> nowhere_common::Result<Arc<dyn LlmClient + Send + Sync + 'static>> {
    match config {
        #[cfg(feature = "ollama")]
        LlmConfig::Ollama { base_url, model } => {
            let client = OllamaClient::new(base_url.clone(), model.clone()).await?;
            Ok(Arc::new(client))
        }
        #[cfg(feature = "gemini")]
        LlmConfig::Gemini { api_key, model } => {
            let client = GeminiClient::new(api_key.clone(), model.clone())?;
            Ok(Arc::new(client))
        }
        LlmConfig::None => Err(NowhereError::Config("No LLM configured".to_string())),
        #[cfg(feature = "openai")]
        LlmConfig::OpenAi {
            api_key,
            model,
            base_url: _,
        } => {
            // FIXME(config): honor `base_url` to support Azure/OpenAI-compatible
            // endpoints or gateways; thread through to OpenAiClient.
            let client = OpenAiClient::new(api_key.clone(), model.clone())?;
            Ok(Arc::new(client))
        }
        #[allow(unreachable_patterns)]
        _ => Err(NowhereError::Config("LLM provider not enabled".to_string())),
    }
}
