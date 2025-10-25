//! Common types and utilities shared across Nowhere crates.
//!
//! This crate defines configuration, feature flags, observability helpers, and shared error types
//! used throughout the Nowhere workspace. It is intentionally lightweight
//! and dependency‑minimal so that all crates can depend on it without
//! introducing heavy transitive costs.
//!
//! # Overview
//!
//! - [`NowhereConfig`]: Top‑level runtime configuration
//! - [`LlmConfig`]: Provider‑agnostic LLM configuration
//! - [`observability`]: Centralised tracing/logging initialisation
//! - [`NowhereError`] and [`Result`]: Shared error handling
//! - Enums describing behavior such as [`StealthLevel`], [`ApprovalMode`],
//!   and [`OutputFormat`]
//!
//! # Examples
//!
//! Constructing a default configuration:
//!
//! ```rust
//! use nowhere_common::{NowhereConfig, StealthLevel};
//!
//! let mut cfg = NowhereConfig::default();
//! cfg.stealth_level = StealthLevel::Balanced;
//! assert_eq!(cfg.max_concurrent_agents, 5);
//! ```
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod observability;

/// Configuration for an LLM provider used by the platform.
///
/// Feature flags control which variants are compiled in.
/// See the `nowhere-llm` crate for concrete client implementations.
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
    #[cfg(feature = "openai")]
    OpenAi {
        api_key: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        base_url: Option<String>,
    },
    None,
}

impl Default for LlmConfig {
    fn default() -> Self {
        // Default to Ollama if the feature is enabled
        #[cfg(feature = "ollama")]
        {
            Self::Ollama {
                base_url: "http://localhost:11434".to_string(),
                model: "llama3".to_string(),
            }
        }
        #[cfg(not(feature = "ollama"))]
        {
            Self::None
        }
    }
}

/// Configuration for Nowhere operations.
///
/// This structure is passed to orchestrators and UI entrypoints to
/// configure runtime behavior.
///
/// ```
/// use nowhere_common::{
///     ApprovalMode, LlmConfig, NowhereConfig, OutputFormat, Result, StealthLevel,
/// };
///
/// let cfg = NowhereConfig::default();
/// assert_eq!(cfg.max_concurrent_agents, 5);
/// assert_eq!(cfg.default_timeout_secs, 60);
/// assert!(!cfg.headless);
/// assert_eq!(cfg.max_rabbit_hole_depth, Some(3));
/// assert!(matches!(cfg.stealth_level, StealthLevel::Balanced));
/// assert!(matches!(cfg.approval_mode, ApprovalMode::Interactive));
/// assert!(matches!(cfg.output_format, OutputFormat::Json));
/// assert!(matches!(cfg.llm_config, LlmConfig::None));
///
/// fn uses_config(cfg: &NowhereConfig) -> Result<()> {
///     if cfg.max_concurrent_agents > 0 {
///         Ok(())
///     } else {
///         Err(nowhere_common::NowhereError::Config("concurrency missing".into()))
///     }
/// }
///
/// uses_config(&cfg).expect("default config is viable");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowhereConfig {
    // FIXME: consolidate this configuration model with `nowhere-config::NowhereConfig` to avoid two divergent sources of truth.
    /// Maximum number of agents that may run concurrently.
    pub max_concurrent_agents: usize,
    /// Default per‑task timeout in seconds.
    pub default_timeout_secs: u64,
    /// Browser automation stealth level.
    pub stealth_level: StealthLevel,
    /// Whether to run browser automation without a visible window.
    pub headless: bool,
    /// How potentially sensitive actions are approved.
    pub approval_mode: ApprovalMode,
    /// Preferred output format for rendered results.
    pub output_format: OutputFormat,
    /// LLM provider configuration.
    pub llm_config: LlmConfig,
    /// Optional cap for recursive “rabbit hole” depth.
    pub max_rabbit_hole_depth: Option<u64>,
}

impl Default for NowhereConfig {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 5,
            default_timeout_secs: 60,
            stealth_level: StealthLevel::Balanced,
            headless: false,
            approval_mode: ApprovalMode::Interactive,
            output_format: OutputFormat::Json,
            llm_config: LlmConfig::default(),
            max_rabbit_hole_depth: Some(3),
        }
    }
}

/// Browser automation stealth level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StealthLevel {
    Lightweight,
    Balanced,
    Maximum,
}

/// Approval behavior for actions that may require user consent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalMode {
    Interactive, // Ask for approval
    Automatic,   // Auto-approve safe operations
    Supervised,  // Auto-approve with logging
}

/// Preferred output format for reports and exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    Json,
    Yaml,
    Html,
    Csv,
}

/// Error types used across the Nowhere system.
///
/// ```
/// use nowhere_common::{NowhereError, Result};
///
/// fn may_timeout() -> Result<()> {
///     Err(NowhereError::Timeout)
/// }
///
/// assert!(matches!(may_timeout(), Err(NowhereError::Timeout)));
/// ```
#[derive(thiserror::Error, Debug)]
pub enum NowhereError {
    /// An agent failed to complete a requested operation.
    #[error("Agent error: {0}")]
    Agent(String),

    /// A driver (browser, network, etc.) reported an error.
    #[error("Driver error: {0}")]
    Driver(#[from] anyhow::Error),

    /// Configuration was incomplete or invalid.
    #[error("Configuration error: {0}")]
    Config(String),

    /// A referenced investigation could not be located.
    #[error("Investigation not found: {0}")]
    InvestigationNotFound(Uuid),

    /// Operation exceeded the configured timeout.
    #[error("Timeout occurred")]
    Timeout,
}

/// Convenient alias for results that use [`NowhereError`].
///
/// ```
/// use nowhere_common::{NowhereError, Result};
///
/// fn check(flag: bool) -> Result<&'static str> {
///     if flag {
///         Ok("ready")
///     } else {
///         Err(NowhereError::Agent("disabled".into()))
///     }
/// }
///
/// assert_eq!(check(true).unwrap(), "ready");
/// assert!(check(false).is_err());
/// ```
pub type Result<T> = std::result::Result<T, NowhereError>;
