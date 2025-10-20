// use anyhow::Result;
// use time::OffsetDateTime;
// use url::Url;
//
// use chrono::{DateTime, Utc};
// use nowhere_data::ingest::WebPageArtifact;
//
// pub fn extract_web_page(
//     url: &Url,
//     html: &str,
//     retrieved_at: OffsetDateTime,
//     published_at: Option<DateTime<Utc>>,
// ) -> Result<WebPageArtifact> {
//     let title = extract_title(html);
//     let text = text_from_html_light(html);
//
//     Ok(WebPageArtifact {
//         url: url.clone(),
//         canonical_url: None, // TODO: <link rel="canonical">
//         title,
//         text,
//         retrieved_at,
//         html_checksum: Some(blake3::hash(html.as_bytes()).to_hex().to_string()),
//         published_at,
//     })
// }
//
// fn extract_title(html: &str) -> Option<String> {
//     // FIXME(parser): replace with a proper HTML parser (`scraper`/`kuchiki`) to
//     // handle entities, nested head content, and malformed markup robustly.
//     // This heuristic can break on edge cases and should be considered temporary.
//     let lower = html.to_lowercase();
//     let start = lower.find("<title")?;
//     let after = &html[start..];
//     let gt = after.find('>')?;
//     let rest = &after[gt + 1..];
//     let end = rest.to_lowercase().find("</title>")?;
//     Some(rest[..end].trim().to_string())
// }
//
// fn text_from_html_light(html: &str) -> String {
//     // FIXME(extraction): this naive tag-stripper will keep script/style text,
//     // mishandle whitespace, and ignore encoding/entity issues. Replace with a
//     // readability-like algorithm using a DOM parser for production use.
//     let mut out = String::with_capacity(html.len() / 4);
//     let mut in_tag = false;
//     for ch in html.chars() {
//         match ch {
//             '<' => in_tag = true,
//             '>' => in_tag = false,
//             _ if !in_tag => out.push(ch),
//             _ => {}
//         }
//     }
//     out.split_whitespace().collect::<Vec<_>>().join(" ")
// }
