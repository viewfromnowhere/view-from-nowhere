use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct NowhereConfig {
    pub version: Option<String>,
    pub actors: Vec<ActorSpec>,
}

/// Shared fields + the per-kind “details”
#[derive(Debug, Deserialize)]
pub struct ActorSpec {
    pub id: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub concurrency: Option<u32>,
    #[serde(flatten)]
    pub details: ActorDetails,
}

/// The tag is `kind`; the payload lives in `config`
#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum ActorDetails {
    #[serde(rename = "twitter")]
    Twitter { config: TwitterConfig },

    #[serde(rename = "llm")]
    Llm { config: LlmConfig },
}

#[derive(Debug, Deserialize)]
pub struct TwitterConfig {
    pub auth_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum LlmConfig {
    Openai {
        model: String,
        auth_token: String,
        #[serde(default)]
        temperature: Option<f32>,
        #[serde(default)]
        max_tokens: Option<u32>,
        #[serde(default = "default_openai_endpoint")]
        endpoint: String,
    },
    Ollama {
        model: String,
        #[serde(default = "default_ollama_endpoint")]
        endpoint: String,
        #[serde(default)]
        temperature: Option<f32>,
        #[serde(default)]
        max_tokens: Option<u32>,
    },
}

fn default_openai_endpoint() -> String {
    "https://api.openai.com/v1".into()
}
fn default_ollama_endpoint() -> String {
    "http://localhost:11434".into()
}

fn expand_env_in_value(v: &mut Value) {
    match v {
        Value::String(s) => {
            if s.contains('$')
                && let Ok(expanded) = shellexpand::env(s)
            {
                *s = expanded.into_owned();
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(expand_env_in_value),
        Value::Object(obj) => obj.values_mut().for_each(expand_env_in_value),
        _ => {}
    }
}

/// Builder hides the `config` crate wiring (YAML + env overrides).
pub struct NowhereConfigLoader {
    builder: config::ConfigBuilder<config::builder::DefaultState>,
}

impl Default for NowhereConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl NowhereConfigLoader {
    /// Start with sensible defaults: YAML file + `NOWHERE_` env overrides.
    ///
    /// ```
    /// use nowhere_config::NowhereConfigLoader;
    ///
    /// let loader = NowhereConfigLoader::new();
    /// let config = loader
    ///     .with_yaml_str("version: '1'\nactors: []")
    ///     .load()
    ///     .expect("valid config");
    ///
    /// assert_eq!(config.version.as_deref(), Some("1"));
    /// assert!(config.actors.is_empty());
    /// ```
    pub fn new() -> Self {
        let builder =
            Config::builder().add_source(Environment::with_prefix("NOWHERE").separator("__"));
        Self { builder }
    }

    /// Attach a YAML/TOML/JSON file; the `config` crate infers format by suffix.
    pub fn with_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.builder = self
            .builder
            // FIXME: support optional config files so headless deployments can rely purely on environment variables.
            .add_source(File::from(path.as_ref()).required(true));
        self
    }

    /// Allow tests/CLI to merge inline YAML snippets.
    ///
    /// ```
    /// use nowhere_config::{ActorDetails, NowhereConfigLoader};
    ///
    /// let cfg = NowhereConfigLoader::new()
    ///     .with_yaml_str(
    ///         r#"
    /// version: "test"
    /// actors:
    ///   - id: "noop"
    ///     enabled: true
    ///     kind: "twitter"
    ///     config:
    ///       auth_token: "example"
    /// "#,
    ///     )
    ///     .load()
    ///     .unwrap();
    ///
    /// assert_eq!(cfg.version.as_deref(), Some("test"));
    /// assert_eq!(cfg.actors.len(), 1);
    /// assert!(matches!(cfg.actors[0].details, ActorDetails::Twitter { .. }));
    /// ```
    pub fn with_yaml_str(mut self, yaml: &str) -> Self {
        self.builder = self
            .builder
            .add_source(File::from_str(yaml, config::FileFormat::Yaml));
        self
    }

    /// Consume the builder and deserialize the merged sources into strongly typed config.
    ///
    /// The loader combines YAML snippets with `NOWHERE_`-prefixed environment variables
    /// and expands `${VAR}` placeholders before materialising strongly typed structs.
    ///
    /// ```
    /// use nowhere_config::{ActorDetails, LlmConfig, NowhereConfigLoader};
    ///
    /// unsafe { std::env::set_var("API_TOKEN", "injected-from-env"); }
    ///
    /// let config = NowhereConfigLoader::new()
    ///     .with_yaml_str(r#"
    /// version: "1"
    /// actors:
    ///   - id: "primary-llm"
    ///     kind: "llm"
    ///     config:
    ///       provider: "openai"
    ///       model: "gpt-4o"
    ///       auth_token: "${API_TOKEN}"
    /// "#)
    ///     .load()
    ///     .expect("valid configuration");
    ///
    /// assert_eq!(config.version.as_deref(), Some("1"));
    /// assert_eq!(config.actors[0].id, "primary-llm");
    ///
    /// match &config.actors[0].details {
    ///     ActorDetails::Llm {
    ///         config: LlmConfig::Openai {
    ///             model,
    ///             auth_token,
    ///             endpoint,
    ///             ..
    ///         },
    ///     } => {
    ///         assert_eq!(model, "gpt-4o");
    ///         assert_eq!(auth_token, "injected-from-env");
    ///         assert_eq!(endpoint, "https://api.openai.com/v1");
    ///     }
    ///     _ => panic!("expected OpenAI configuration"),
    /// }
    ///
    /// unsafe { std::env::remove_var("API_TOKEN"); }
    /// ```
    pub fn load(self) -> Result<NowhereConfig, ConfigError> {
        let cfg = self.builder.build()?;

        // Convert to serde_json::Value first
        let mut v: Value = cfg.try_deserialize()?;
        // Recursively expand environment variables
        expand_env_in_value(&mut v);

        // Deserialize into your strongly-typed config
        let typed: NowhereConfig =
            serde_json::from_value(v).map_err(|e| config::ConfigError::Message(e.to_string()))?;

        Ok(typed)
    }
}
