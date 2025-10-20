// use crate::traits::LlmClient;
// use anyhow::{anyhow, Result};
// use nowhere_data::ingest::SearchArtifacts;
// use regex::Regex;
// use serde::{Deserialize, Serialize};
// use std::collections::HashMap; // add
// /// ---------- Public Types surfaced to the TUI ----------
//
// #[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
// pub struct LlmScreening {
//     pub is_verifiable: bool,
//     pub reason: String,
//     pub claim: String,
//     pub extracted_entities: Vec<String>, // rough entities (names, orgs, years)
//     pub search: Option<SearchArtifacts>,
// }
//
// /// ---------- LLM Call & Parsing ----------
// /// Main entry point used by the TUI.
// /// 1) Check verifiability with the LLM.
// /// 2) If claim_node present, build Claim + Assessment (verdict remains Unclear).
// pub async fn verify_with_llm(llm: &dyn LlmClient, raw: &str) -> Result<LlmScreening> {
//     let system_prompt = VERIFIABILITY_SYSTEM_PROMPT;
//     let user_prompt = build_user_prompt(raw);
//
//     // Use model-default tokens/temperature; you can pass opts if your client supports them.
//     let resp = llm
//         .generate(&user_prompt, Some(system_prompt), None, Some(0.2))
//         .await
//         .map_err(|e| anyhow!(format!("LLM error: {e}")))?;
//
//     let text = resp.text.trim();
//
//     // Try to locate a JSON block; allow for models that wrap with ```json fences.
//     let json_str = extract_json_block(text).unwrap_or_else(|| text.to_string());
//
//     // Parse into a wire struct that mirrors LlmScreening but keeps `claim_node` as raw JSON first.
//     let wire: LlmScreeningWire = serde_json::from_str(&json_str)
//         .map_err(|e| anyhow!("Failed to parse verifiability JSON: {e}\nRaw:\n{text}"))?;
//
//     // Normalize / sanitize lists
//     let mut entities = wire.entities.unwrap_or_default();
//     entities.sort();
//     entities.dedup();
//
//     let search = wire.search.map(sanitize_artifacts);
//
//     Ok(LlmScreening {
//         is_verifiable: wire.is_verifiable,
//         reason: wire.reason.unwrap_or_default(),
//         extracted_entities: entities,
//         claim: wire.claim.unwrap_or_default(),
//         search,
//     })
// }
//
// /// Wire-format to deserialize strictly from the model output.
// /// `claim_node` stays as `serde_json::Value` first; we later attempt to decode it as `ClaimNode`.
// #[derive(Debug, Clone, Deserialize)]
// struct LlmScreeningWire {
//     is_verifiable: bool,
//     #[serde(default)]
//     reason: Option<String>,
//     #[serde(default)]
//     entities: Option<Vec<String>>,
//     #[serde(default)]
//     claim: Option<String>,
//     #[serde(default)]
//     search: Option<SearchArtifacts>,
// }
//
// /// Try to extract a ```json ... ``` fenced block; fall back to raw.
// fn extract_json_block(text: &str) -> Option<String> {
//     let re_fence = Regex::new("(?s)```json\\s*(\\{.*?\\})\\s*```").ok()?;
//     if let Some(caps) = re_fence.captures(text) {
//         return Some(caps.get(1)?.as_str().to_string());
//     }
//     let re_plain = Regex::new("(?s)(\\{.*\\})").ok()?;
//     re_plain
//         .captures(text)
//         .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
// }
//
// fn clip01(x: f32) -> f32 {
//     x.clamp(0.0, 1.0)
// }
//
// fn sanitize_artifacts(mut s: SearchArtifacts) -> SearchArtifacts {
//     let mut cleaned = HashMap::new();
//     for (k, v) in s.queries.into_iter() {
//         let q = v.replace('\n', " ").replace('\r', " ").trim().to_string();
//         let cased = k.to_ascii_lowercase();
//         let safe = match cased.as_str() {
//             "twitter" | "x" => sanitize_twitter_query(&q),
//             "web" | "brave" => sanitize_web_query(&q),
//             _ => Some(q), // accept as-is for unknown channels (you can tighten later)
//         };
//         if let Some(q2) = safe {
//             cleaned.insert(cased, q2);
//         }
//     }
//     s.queries = cleaned;
//     s
// }
//
// fn sanitize_twitter_query(q: &str) -> Option<String> {
//     // allow only: quotes, parentheses, OR, '-', from:, lang:, -is:retweet
//     // reject common unsupported operators
//     let lower = q.to_ascii_lowercase();
//     let disallowed = [
//         " near:",
//         " min_faves",
//         " min_retweets",
//         " min_replies",
//         " since:",
//         " until:",
//         " url:",
//         " list:",
//         " place:",
//         " point_radius:",
//         " bounding_box:",
//         "~",
//         " to:",
//         " source:",
//         " context:",
//     ];
//     if disallowed.iter().any(|bad| lower.contains(bad)) {
//         return None;
//     }
//
//     // Clamp to 1024 chars (Twitter v2 limit). Trim at last whitespace boundary.
//     let mut out = q.trim().to_string();
//     if out.len() > 1024 {
//         if let Some(idx) = out
//             .char_indices()
//             .take_while(|(i, _)| *i <= 1020)
//             .map(|(i, _)| i)
//             .last()
//         {
//             out.truncate(idx);
//         } else {
//             out.truncate(1024);
//         }
//     }
//     let out = collapse_twitter_boolean_and(out);
//     Some(out)
// }
//
// fn collapse_twitter_boolean_and(input: String) -> String {
//     let mut tokens = Vec::new();
//     let mut current = String::new();
//     let mut in_quotes = false;
//
//     for ch in input.chars() {
//         match ch {
//             '"' => {
//                 current.push(ch);
//                 in_quotes = !in_quotes;
//             }
//             '(' | ')' if !in_quotes => {
//                 if !current.is_empty() {
//                     tokens.push(std::mem::take(&mut current));
//                 }
//                 tokens.push(ch.to_string());
//             }
//             ch if ch.is_whitespace() && !in_quotes => {
//                 if !current.is_empty() {
//                     tokens.push(std::mem::take(&mut current));
//                 }
//             }
//             _ => current.push(ch),
//         }
//     }
//
//     if !current.is_empty() {
//         tokens.push(current);
//     }
//
//     let mut cleaned = Vec::with_capacity(tokens.len());
//     for token in tokens {
//         if !token.starts_with('"') && token.eq_ignore_ascii_case("and") {
//             continue;
//         }
//         cleaned.push(token);
//     }
//
//     rebuild_twitter_query(&cleaned)
// }
//
// fn rebuild_twitter_query(tokens: &[String]) -> String {
//     if tokens.is_empty() {
//         return String::new();
//     }
//
//     let mut out = String::new();
//     for (idx, token) in tokens.iter().enumerate() {
//         if token.is_empty() {
//             continue;
//         }
//         if idx > 0 {
//             let prev = tokens[idx - 1].as_str();
//             if token.as_str() != ")" && prev != "(" {
//                 out.push(' ');
//             }
//         }
//         out.push_str(token);
//     }
//     out
// }
//
// fn sanitize_web_query(q: &str) -> Option<String> {
//     // keep it simple: allow site:, quotes/OR/-, and trim length
//     let lower = q.to_ascii_lowercase();
//     let disallowed = [" near:", "~"];
//     if disallowed.iter().any(|bad| lower.contains(bad)) {
//         return None;
//     }
//     let mut out = q.trim().to_string();
//     if out.len() > 2048 {
//         if let Some(idx) = out
//             .char_indices()
//             .take_while(|(i, _)| *i <= 2044)
//             .map(|(i, _)| i)
//             .last()
//         {
//             out.truncate(idx);
//         } else {
//             out.truncate(2048);
//         }
//     }
//     Some(out)
// }
//
// fn summarize_for_humans(raw: &str) -> String {
//     let s = raw.trim();
//     const MAX: usize = 140;
//     if s.chars().count() <= MAX {
//         s.to_string()
//     } else {
//         let mut out = String::new();
//         for ch in s.chars() {
//             if out.chars().count() + 1 >= MAX {
//                 break;
//             }
//             out.push(ch);
//         }
//         out.push('…');
//         out
//     }
// }
//
// /// ---------- Prompts ----------
// pub const VERIFIABILITY_SYSTEM_PROMPT: &str = r#"
// You are an impartial fact-checking analyst.
//
// Tasks:
// 1) Decide if the user's input is a verifiable factual claim (objective, checkable against external evidence).
// 2) If verifiable, don't alter it. If it isn't verifiable, substitute the claim with one that is verifiable. And present reasons why the original query wasn't verifiable
// 3) Extract entities from the claim.
// 4) Return a compact `search` with exactly one high-recall query per channel (twitter, web). If unsure, set the channel to null.
// 5) Do NOT invent facts. If specifics are missing, keep them null or use angle-bracket placeholders like <DATE>, <LOCATION>, <QUANTITY>.
//
// Output rules:
// - Output STRICT JSON ONLY that matches the schema provided in the user message.
// - Keep strings concise. No markdown, no prose outside fields.
//
// Guidance:
// - A verifiable claim is concrete on who/what, did what, where/when, or measurable/observable.
// - Queries: exactly one per channel; prefer recall over precision.
//   - twitter: prefer "entity" + 1 context word like '"Terry McLaurin" contract'
//   - web: prefer "entity + 1 context word" and optionally one site:domain.
//
// If the input is not verifiable:
// - Set is_verifiable=false, give a short reason and 1–3 clarifying questions.
//
// Return STRICT JSON ONLY.
// "#;
//
// pub fn build_user_prompt(user_text: &str) -> String {
//     format!(
//         r#"
// Return STRICT JSON ONLY with this schema:
//
// {{
//   "is_verifiable": boolean,
//   "reason": string,                               // short rationale
//   "entities": [ string ],                         // rough named entities (deduped, short)
//   "claim": string,
//   "search": {{
//     "queries": {{
//       "twitter": string | null,                   // exactly one query or null
//       "web":     string | null                    // exactly one query or null
//     }},
//     "date_window": {{
//       "from": "YYYY-MM-DDTHH:MM:SSZ",
//       "to":   "YYYY-MM-DDTHH:MM:SSZ"
//     }} | null
//   }} | null
// }}
//
// Constraints:
// - Use placeholders like <DATE>, <LOCATION>, <QUANTITY> if specifics are unknown.
// - twitter query: use at most 2 required concepts total; allowed grammar: quotes, (), OR, single '-', -is:retweet, lang:en.
// - web query: keep concise; allowed grammar: quotes, (), OR, '-', one site:domain.
//
// User input:
// {user_text}
// "#
//     )
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn twitter_sanitizer_removes_boolean_and() {
//         let input = r#""New York Yankees" AND "2024 World Series" AND (win OR won) -is:retweet"#;
//         let sanitized = sanitize_twitter_query(input).expect("should sanitize");
//         assert_eq!(
//             sanitized,
//             r#""New York Yankees" "2024 World Series" (win OR won) -is:retweet"#
//         );
//     }
//
//     #[test]
//     fn twitter_sanitizer_keeps_quoted_and() {
//         let input = r#""bread and butter" and lang:en"#;
//         let sanitized = sanitize_twitter_query(input).expect("should sanitize");
//         assert_eq!(sanitized, r#""bread and butter" lang:en"#);
//     }
//
//     #[test]
//     fn twitter_sanitizer_handles_lowercase_and() {
//         let input = "climate and change";
//         let sanitized = sanitize_twitter_query(input).expect("should sanitize");
//         assert_eq!(sanitized, "climate change");
//     }
// }
