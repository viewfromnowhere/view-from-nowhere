// use anyhow::{Context, Result};
// use time::OffsetDateTime;
// use url::Url;
//
// use crate::twitter::types::{Includes, Media, Tweet};
// use nowhere_data::ingest::{MediaKind, MediaRef, Platform, PostArtifact, PostMetrics};
//
// /// Convert Twitter composite JSON into a normalized PostArtifact.
// pub fn extract_post_from_twitter_json(value: &serde_json::Value) -> Result<PostArtifact> {
//     // Expect: { "data": Tweet, "includes": { users: [...], media: [...] } }
//     let tweet: Tweet = serde_json::from_value(value.get("data").cloned().context("missing data")?)?;
//     let includes: Option<Includes> = value
//         .get("includes")
//         .cloned()
//         .and_then(|v| serde_json::from_value(v).ok());
//
//     // Resolve author (optional)
//     let author = tweet.author_id.as_ref().and_then(|aid| {
//         includes
//             .as_ref()
//             .and_then(|inc| inc.users.as_ref())
//             .and_then(|users| users.iter().find(|u| &u.id == aid))
//     });
//
//     let author_handle = author.map(|u| u.username.clone());
//     let author_display_name = author.and_then(|u| u.name.clone());
//
//     // Canonical status URL
//     let source_url = make_status_url(author_handle.as_deref(), &tweet.id);
//
//     // Parse created_at (RFC 3339)
//     let created_at = tweet.created_at.as_deref().and_then(|s| {
//         OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
//     });
//
//     // External URLs
//     let urls: Vec<Url> = tweet
//         .entities
//         .as_ref()
//         .and_then(|e| e.urls.as_ref())
//         .map(|list| {
//             list.iter()
//                 .filter_map(|u| u.expanded_url.as_ref())
//                 .filter_map(|s| Url::parse(s).ok())
//                 .collect()
//         })
//         .unwrap_or_default();
//
//     // Mentions
//     let mentions: Vec<String> = tweet
//         .entities
//         .as_ref()
//         .and_then(|e| e.mentions.as_ref())
//         .map(|list| list.iter().map(|m| m.username.clone()).collect())
//         .unwrap_or_default();
//
//     // Media: attachments.media_keys → includes.media
//     let media: Vec<MediaRef> =
//         if let (Some(att), Some(inc)) = (&tweet.attachments, includes.as_ref()) {
//             let keys = att.media_keys.as_deref().unwrap_or(&[]);
//             let all = inc.media.as_deref().unwrap_or(&[]);
//             keys.iter()
//                 .filter_map(|k| all.iter().find(|m| m.media_key.as_deref() == Some(k)))
//                 .map(to_media_ref)
//                 .collect()
//         } else {
//             vec![]
//         };
//
//     // Metrics
//     let metrics = tweet.public_metrics.as_ref().map(|m| PostMetrics {
//         like_count: m.like_count,
//         repost_count: m.repost_count,
//         reply_count: m.reply_count,
//         quote_count: m.quote_count,
//         bookmark_count: m.bookmark_count,
//     });
//
//     Ok(PostArtifact {
//         platform: Platform::Twitter,
//         external_id: tweet.id.clone(),
//         author_handle,
//         author_display_name,
//         text: tweet.text,
//         lang: tweet.lang,
//         created_at,
//         source_url,
//         urls,
//         media,
//         metrics,
//         conversation_id: tweet.conversation_id,
//         reply_to: tweet
//             .referenced_tweets
//             .as_ref()
//             .and_then(|v| v.iter().find(|r| r.kind == "replied_to"))
//             .map(|r| r.id.clone()),
//         mentions,
//     })
// }
//
// fn to_media_ref(m: &Media) -> MediaRef {
//     let kind = match m.kind.as_deref() {
//         Some("photo") => MediaKind::Photo,
//         Some("video") => MediaKind::Video,
//         Some("animated_gif") => MediaKind::Gif,
//         _ => MediaKind::Unknown,
//     };
//     MediaRef {
//         kind,
//         url: m
//             .url
//             .as_ref()
//             .or(m.preview_image_url.as_ref())
//             .and_then(|s| Url::parse(s).ok()),
//         width: m.width,
//         height: m.height,
//         duration_ms: m.duration_ms,
//     }
// }
//
// /// Build a canonical X status URL if we know the handle; otherwise /i/web/status/{id}.
// pub fn make_status_url(handle: Option<&str>, id: &str) -> Option<Url> {
//     if let Some(h) = handle {
//         Url::parse(&format!("https://x.com/{}/status/{}", h, id)).ok()
//     } else {
//         Url::parse(&format!("https://x.com/i/web/status/{}", id)).ok()
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use serde_json::json;
//
//     #[test]
//     fn extract_minimal() {
//         let v = json!({
//             "data": {
//                 "id": "123",
//                 "text": "hello",
//                 "author_id": "42",
//                 "lang": "en",
//                 "created_at": "2025-09-01T12:00:00Z",
//                 "conversation_id": "123",
//                 "entities": { "mentions": [{"username":"bob"}], "urls": [{"expanded_url":"https://example.com"}] },
//                 "public_metrics": { "like_count": 1, "reply_count": 2, "quote_count": 0, "bookmark_count": 0 },
//                 "attachments": { "media_keys": ["3_abc"] }
//             },
//             "includes": {
//                 "users": [ { "id": "42", "username":"alice", "name":"Alice" } ],
//                 "media": [ { "media_key":"3_abc", "type":"photo", "url":"https://img.example.com/1.jpg", "width":800, "height":600 } ]
//             }
//         });
//         let post = extract_post_from_twitter_json(&v).unwrap();
//         assert_eq!(post.external_id, "123");
//         assert_eq!(post.author_handle.as_deref(), Some("alice"));
//         assert_eq!(post.mentions, vec!["bob"]);
//         assert_eq!(post.urls.len(), 1);
//         assert_eq!(post.media.len(), 1);
//     }
// }
