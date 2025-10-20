pub mod actor;
pub mod builder;
pub mod llm;
pub mod rate;
pub mod registry;
pub mod store;
pub mod supervise;
pub mod system;
pub mod twitter;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimContext {
    pub id: Uuid,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchCmd {
    pub query: String,
    pub date_from: DateTime<Utc>,
    pub date_to: DateTime<Utc>,
    pub claim: ClaimContext,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawArtifact {
    pub external_id: String,
    pub payload: serde_json::Value,
    pub claim: ClaimContext,
}

#[derive(Debug, FromRow)]
pub struct NormalizedArtifact {
    pub external_id: String,
    pub internal_id: Uuid,
    pub claim_id: Uuid,
    pub claim_relevance: bool,
    pub reasoning: String,
    pub provenance_info: String,
    pub entities: Vec<Entity>,
}

#[derive(Debug, FromRow)]
pub struct Entity {
    pub article_id: Uuid,
    pub external_id: String,
    pub name: String,
    pub credibility: Credibility,
    pub reasoning: String,
}

#[derive(Debug)]
pub enum Credibility {
    Strong,
    Weak,
    Unknown,
}

impl Credibility {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "strong" => Credibility::Strong,
            "weak" => Credibility::Weak,
            _ => Credibility::Unknown,
        }
    }
}

pub enum StoreMsg {
    InsertClaim(ClaimContext),
    UpsertArtifact(NormalizedArtifact),
    GetArtifact {
        internal_id: Uuid,
        reply: oneshot::Sender<Result<ArtifactWithEntities>>,
    },
    SearchArtifacts {
        claim: Uuid,
        query: String,
        limit: i64,
        reply: oneshot::Sender<Result<Vec<ArtifactRow>>>,
    },
    WatchArtifacts {
        claim: Uuid,
        reply: oneshot::Sender<()>,
    },
    ArtifactUpserted {
        claim: Uuid,
    },
    ListEntitiesByName {
        name: String,
        limit: i64,
        reply: oneshot::Sender<Result<Vec<EntityRow>>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRow {
    pub internal_id: String,
    pub external_id: String,
    pub claim_relevance: bool,
    pub reasoning: String,
    pub provenance_info: String,
    pub claim_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRow {
    pub id: String,
    pub article_id: String,
    pub name: String,
    pub credibility: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactWithEntities {
    pub artifact: ArtifactRow,
    pub entities: Vec<EntityRow>,
}

pub enum LlmMsg {
    NormalizeArtifact(RawArtifact),
    BuildSearchQuery {
        claim: ClaimContext,
        reply: oneshot::Sender<BuiltSearchQuery>,
    },
}

pub struct ChatCmd {
    pub user_text: String,
    pub k: i64,
    pub reply: oneshot::Sender<ChatResponse>,
    pub claim: ClaimContext,
}

#[derive(Serialize, Deserialize)]
pub struct ChatResponse {
    pub text: String,
    pub used_artifacts: Vec<String>,
    pub used_entities: Vec<String>,
    pub caveats: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SearchQueryResponse {
    query: String,
    date_from: DateTime<Utc>,
    date_to: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct BuiltSearchQuery {
    pub query: String,
    pub date_from: DateTime<Utc>,
    pub date_to: DateTime<Utc>,
    pub claim: ClaimContext,
}
