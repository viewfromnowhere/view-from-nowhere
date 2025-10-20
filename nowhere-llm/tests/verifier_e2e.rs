// use anyhow::Result;
// use nowhere_common::observability::LogConfig;
// use nowhere_llm::openai::OpenAiClient;
// use nowhere_llm::traits::LlmClient;
// use nowhere_llm::verifier::{verify_with_llm, LlmScreening};
// use std::env;
// use std::sync::Arc;
//
// #[tokio::test(flavor = "multi_thread")]
// async fn test_verify_with_llm() -> Result<()> {
//     nowhere_common::observability::init_logging(LogConfig::default())?;
//
//     let api_key =
//         env::var("OPENAI_API_KEY").map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
//     let model = env::var("NOWHERE_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string());
//     let llm = OpenAiClient::new(api_key, model)
//         .map_err(|e| anyhow::anyhow!(format!("OpenAiClient init failed: {e}")))?;
//     let llm_client: Arc<dyn LlmClient + Send + Sync> = Arc::new(llm);
//
//     let raw_claim = "Terry McLaurin signed a contract for 100 million USD in 2023";
//     let screening: LlmScreening = verify_with_llm(llm_client.as_ref(), raw_claim).await?;
//     tracing::debug!("screening: {:#?}", screening);
//     assert_screening_invariants(&screening, raw_claim);
//
//     Ok(())
// }
//
// pub fn assert_non_empty_trimmed(label: &str, s: &str) {
//     assert!(!s.trim().is_empty(), "{label} should be non-empty/trimmed");
//     assert_eq!(
//         s,
//         s.trim(),
//         "{label} should not have leading/trailing whitespace"
//     );
// }
//
// pub fn assert_entities_clean(entities: &[String]) {
//     // non-empty, trimmed, reasonable length, no empties after trim
//     for e in entities {
//         assert_eq!(e, e.trim(), "entity should be trimmed: {:?}", e);
//         assert!(!e.is_empty(), "entity should not be empty");
//         assert!(e.len() <= 128, "entity too long: len={}", e.len());
//     }
//     // dedup check
//     let mut uniq = entities.to_vec();
//     uniq.sort();
//     uniq.dedup();
//     assert_eq!(
//         &uniq, entities,
//         "entities should be deduplicated + stable ordered"
//     );
// }
//
// pub fn assert_search_sane(search: &SearchArtifacts) {
//     // adapt to your type; examples:
//     assert!(
//         !search.queries.is_empty(),
//         "search.queries should not be empty"
//     );
//     for v in search.queries.values() {
//         assert!(!v.trim().is_empty(), "query should be non-empty");
//         // basic redaction check
//         assert!(!v.contains("sk-"), "query must not leak secrets");
//     }
//     // if you store results:
//     // assert!(search.results.len() > 0, "should produce at least one candidate");
// }
//
// pub fn assert_screening_invariants(s: &LlmScreening, raw_claim: &str) {
//     assert_non_empty_trimmed("reason", &s.reason);
//     // if you normalize claim, change this to your normalization fn
//     assert_eq!(s.claim, raw_claim, "claim should echo the input");
//     assert_entities_clean(&s.extracted_entities);
//
//     if s.is_verifiable {
//         assert!(
//             !s.extracted_entities.is_empty(),
//             "verifiable -> entities non-empty"
//         );
//         let search = s
//             .search
//             .as_ref()
//             .expect("verifiable -> search should be Some");
//         assert_search_sane(search);
//     } else {
//         assert!(s.reason.len() >= 10, "non-verifiable should explain why");
//         // `search` may be None or Some; if Some, still must be sane
//         if let Some(search) = &s.search {
//             assert_search_sane(search);
//         }
//     }
// }
//
// pub fn assert_serde_roundtrip(s: &LlmScreening) -> Result<()> {
//     let json = serde_json::to_string_pretty(s)?;
//     let back: LlmScreening = serde_json::from_str(&json)?;
//     assert_eq!(&back, s, "serde roundtrip must preserve equality");
//     Ok(())
// }
//
