use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmConfig {
    #[cfg(feature = "ollama")]
    Ollama {
        base_url: String,
        model: String,
    },
    #[cfg(feature = "gemini")]
    Gemini {
        api_key: String,
        model: String,
    },
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        #[cfg(feature = "ollama")]
        {
            Self::Ollama {
                base_url: "http://localhost:11434".to_string(),
                model: crate::DEFAULT_OLLAMA_MODEL.to_string(),
            }
        }
        #[cfg(not(feature = "ollama"))]
        {
            Self::Gemini {
                api_key: (),
                model: (),
            }
        }
    }
}
