use nowhere_config::NowhereConfigLoader;
use serial_test::serial;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

/// Helper to write a YAML file in a temp dir and return its path.
fn write_yaml(tmp: &TempDir, name: &str, yaml: &str) -> PathBuf {
    let p = tmp.path().join(name);
    fs::write(&p, yaml).expect("write yaml");
    p
}

#[test]
#[serial]
fn test_config_load() {
    let tmp = TempDir::new().unwrap();

    // A file that sets some fields; we'll override a subset via env.
    let file_yaml = r#"
version: 0.1
actors:
  - id: twitter
    kind: twitter
    enabled: true
    concurrency: 2
    config:
      auth_token: "${TWITTER_BEARER_TOKEN}"
  - id: llm_openai_fast
    kind: llm
    enabled: true
    concurrency: 2
    config:
      provider: openai
      model: "gpt-4o-mini"
      auth_token: "${OPENAI_API_KEY}"
      temperature: 0.2
      max_tokens: 512
  "#;
    let p = write_yaml(&tmp, "nowhere.yaml", file_yaml);

    let config = NowhereConfigLoader::new()
        .with_file(p)
        .load()
        .expect("load system config");

    assert!(!config.actors.is_empty());
}
