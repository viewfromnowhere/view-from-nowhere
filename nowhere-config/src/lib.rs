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
    pub fn with_yaml_str(mut self, yaml: &str) -> Self {
        self.builder = self
            .builder
            .add_source(File::from_str(yaml, config::FileFormat::Yaml));
        self
    }

    /// Consume builder, deserialize into strongly typed config.
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
