use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub data: Option<Vec<Tweet>>,
    pub includes: Option<Includes>,
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Meta {
    #[serde(default)]
    pub next_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Includes {
    #[serde(default)]
    pub users: Option<Vec<User>>,
    #[serde(default)]
    pub media: Option<Vec<Media>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    #[serde(default)]
    pub media_key: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub preview_image_url: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,

    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub in_reply_to_user_id: Option<String>,

    #[serde(default)]
    pub public_metrics: Option<PublicMetrics>,
    #[serde(default)]
    pub entities: Option<Entities>,
    #[serde(default)]
    pub referenced_tweets: Option<Vec<ReferencedTweet>>,
    #[serde(default)]
    pub possibly_sensitive: Option<bool>,
    #[serde(default)]
    pub source: Option<String>,

    // Attachments for media mapping
    #[serde(default)]
    pub attachments: Option<Attachments>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Attachments {
    #[serde(default)]
    pub media_keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PublicMetrics {
    pub like_count: Option<u64>,
    #[serde(alias = "retweet_count")]
    pub repost_count: Option<u64>,
    pub reply_count: Option<u64>,
    pub quote_count: Option<u64>,
    pub bookmark_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencedTweet {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Entities {
    #[serde(default)]
    pub urls: Option<Vec<UrlEntity>>,
    #[serde(default)]
    pub mentions: Option<Vec<MentionEntity>>,
    #[serde(default)]
    pub hashtags: Option<Vec<HashTag>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlEntity {
    #[serde(default)]
    pub expanded_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionEntity {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashTag {
    pub tag: String,
}
