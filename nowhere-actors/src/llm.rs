use crate::actor::Context;
use crate::actor::{Actor, Addr};
use crate::rate::RateKey;
use crate::rate::{RateLimiter, RateMsg};
use crate::store::StoreActor;
use crate::{
    ArtifactRow, ArtifactWithEntities, BuiltSearchQuery, ChatCmd, ChatResponse, Credibility,
    Entity, LlmMsg, NormalizedArtifact, SearchQueryResponse, StoreMsg,
};
use anyhow::{anyhow, Result};
use nowhere_llm::traits::LlmClient;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

pub struct LlmActor {
    llm_client: Arc<dyn LlmClient + Send + Sync>,
    rate_limiter: Addr<RateLimiter>,
    rate_key: RateKey,
    out: Addr<StoreActor>,
}

impl LlmActor {
    pub fn new(
        rate_limiter: Addr<RateLimiter>,
        rate_key: RateKey,
        out: Addr<StoreActor>,
        llm_client: Arc<dyn LlmClient + Send + Sync>,
    ) -> Self {
        Self {
            llm_client,
            rate_limiter,
            rate_key,
            out,
        }
    }

    // optional ergonomic helpers
    pub fn with_rate_key(mut self, key: RateKey) -> Self {
        self.rate_key = key;
        self
    }
}
#[async_trait::async_trait]
impl Actor for LlmActor {
    type Msg = LlmMsg;

    async fn handle(&mut self, msg: Self::Msg, _ctx: &mut Context<Self>) -> Result<()> {
        match msg {
            LlmMsg::NormalizeArtifact(raw_artifact) => {
                acquire_rate_permit(&self.rate_limiter, &self.rate_key).await?;
                let artifact_json = serde_json::to_string_pretty(&raw_artifact.payload)?;

                let system_prompt = self.llm_client.default_osint_system_prompt().to_string();
                let schema_description = r#"
You must respond with a single JSON object that matches this schema exactly:
{
  "claim_relevance": boolean,
  "reasoning": string,
  "provenance_info": string,
  "entities": [
    {
      "external_id": string | null,
      "name": string,
      "credibility": "strong" | "weak" | "unknown",
      "reasoning": string
    }
  ]
}
The JSON must be valid. Do not include any additional commentary or code fences. Entities can include extracted entities from text, as well as twitter users
including the author of the tweet or those mentioned."#;

                let prompt = format!(
            "Investigation claim: \"{}\"\n\nNormalize the following raw artifact from Twitter into the schema described.\nArtifact external_id: {}\nRaw artifact JSON:\n{}\n{}",
            raw_artifact.claim.text, raw_artifact.external_id, artifact_json, schema_description
        );

                let response = self
                    .llm_client
                    .generate(&prompt, Some(&system_prompt), Some(600), Some(0.2))
                    .await
                    .map_err(anyhow::Error::from)?;

                let parsed = parse_llm_normalization(&response.text)?;
                let internal_id = Uuid::new_v4();
                let entities = parsed
                    .entities
                    .into_iter()
                    .enumerate()
                    .map(|(idx, entity)| Entity {
                        article_id: internal_id,
                        external_id: entity.external_id.unwrap_or_else(|| {
                            format!("{}:entity:{idx}", raw_artifact.external_id)
                        }),
                        name: entity.name,
                        credibility: Credibility::from(entity.credibility.as_str()),
                        reasoning: entity.reasoning,
                    })
                    .collect();

                let normalized = NormalizedArtifact {
                    external_id: raw_artifact.external_id.clone(),
                    internal_id,
                    claim_id: raw_artifact.claim.id,
                    claim_relevance: parsed.claim_relevance,
                    reasoning: parsed.reasoning,
                    provenance_info: parsed.provenance_info,
                    entities,
                };

                self.out
                    .send(StoreMsg::UpsertArtifact(normalized))
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "store actor mailbox dropped (artifact={})",
                            raw_artifact.external_id
                        )
                    })?;
            }
            LlmMsg::BuildSearchQuery { claim, reply } => {
                let system_prompt = self.llm_client.default_osint_system_prompt().to_string();
                let user_directions = r#"
You must respond with a single JSON object that matches this schema exactly:
{
  "query": string,
  "date_from": string,
  "date_to": string,
}
The JSON must be valid. Do not include any additional commentary or code fences.
The query must be a string representing a twitter search query based the attached claim. Ideally, this would include the key entity and perhaps the most
important action or object involved. For example, if the claim is "Terry McLaurin signed a contract for 500 million USD in 2024.", the search would be
'"Terry McLaurin" contract'. The date values must be deserializable into chrono::DateTime<Utc> values."#;
                let prompt = format!(
                    "Investigation claim: \"{}\"\n\n directions: {}",
                    claim.text, user_directions
                );

                acquire_rate_permit(&self.rate_limiter, &self.rate_key).await?;

                let resp = self
                    .llm_client
                    .generate(&prompt, Some(&system_prompt), Some(600), Some(0.2))
                    .await?;

                let search_query_response =
                    serde_json::from_str::<SearchQueryResponse>(&resp.text)?;

                let _ = reply.send(BuiltSearchQuery {
                    query: search_query_response.query,
                    date_from: search_query_response.date_from,
                    date_to: search_query_response.date_to,
                    claim,
                });
            }
        }
        Ok(())
    }
}

pub struct ChatLlmActor {
    llm_client: Arc<dyn LlmClient + Send + Sync>,
    rate_limiter: Addr<RateLimiter>,
    rate_key: RateKey,
    store: Addr<StoreActor>,
}

impl ChatLlmActor {
    pub fn new(
        rate_limiter: Addr<RateLimiter>,
        rate_key: RateKey,
        store: Addr<StoreActor>,
        llm_client: Arc<dyn LlmClient + Send + Sync>,
    ) -> Self {
        Self {
            llm_client,
            rate_limiter,
            rate_key,
            store,
        }
    }

    pub fn with_rate_key(mut self, key: RateKey) -> Self {
        self.rate_key = key;
        self
    }
}

#[async_trait::async_trait]
impl Actor for ChatLlmActor {
    type Msg = ChatCmd;

    async fn handle(&mut self, msg: Self::Msg, _ctx: &mut Context<Self>) -> Result<()> {
        let ChatCmd {
            user_text,
            k,
            reply,
            claim,
        } = msg;

        let hits = store_search_artifacts(&self.store, claim.id, &user_text, k)
            .await
            // FIXME: plumb store errors back to the TUI so users know retrieval failed instead of silently falling back to an empty set.
            .unwrap_or_default();

        let mut bundles = Vec::new();
        for artifact in hits.iter().take(6) {
            // FIXME: make the retrieval depth configurable instead of hard-coding 6 artifacts.
            if let Ok(bundle) = store_get_artifact(&self.store, &artifact.internal_id).await {
                bundles.push(bundle);
            }
        }

        acquire_rate_permit(&self.rate_limiter, &self.rate_key).await?;

        let sys = "You answer questions strictly using the provided artifacts and entities. \
                   Always include artifact internal_ids and entity ids you relied on. \
                   Note entity credibility labels (strong/weak/unknown). \
                   If uncertain, state caveats briefly.";
        let context = serde_json::json!({
            "artifacts": bundles.iter().map(|b| {
                serde_json::json!({
                  "internal_id": b.artifact.internal_id,
                  "external_id": b.artifact.external_id,
                  "reasoning": b.artifact.reasoning,
                  "provenance_info": b.artifact.provenance_info,
                  "entities": b.entities.iter().map(|e| {
                    serde_json::json!({
                      "id": e.id,
                      "name": e.name,
                      "credibility": e.credibility
                    })
                  }).collect::<Vec<_>>()
                })
            }).collect::<Vec<_>>(),
        });

        let prompt = format!(
            "User question: {}\n\nContext JSON (facts only):\n{}\
             \nInstructions: Answer concisely. When you mention a fact, add citations like [A:<artifact_id>] \
             and optionally [E:<entity_id>] right after the sentence. Do not invent data.",
            user_text,
            serde_json::to_string(&context)?
        );

        let resp = self
            .llm_client
            // FIXME: surface temperature/max token choices from config rather than hard-coding generation parameters here.
            .generate(&prompt, Some(sys), Some(1000), Some(0.5))
            .await?;
        let answer = resp.text.trim().to_string();

        let used_artifacts = bundles
            .iter()
            .map(|b| b.artifact.internal_id.clone())
            .collect();
        let used_entities = bundles
            .iter()
            .flat_map(|b| b.entities.iter().map(|e| e.id.clone()))
            .take(5)
            .collect();

        let out = ChatResponse {
            text: answer,
            used_artifacts,
            used_entities,
            // FIXME: capture explicit caveats from the model response instead of always returning an empty list.
            caveats: vec![],
        };
        let _ = reply.send(out);
        Ok(())
    }
}

async fn acquire_rate_permit(rate_limiter: &Addr<RateLimiter>, rate_key: &RateKey) -> Result<()> {
    let (permit_tx, permit_rx) = oneshot::channel();
    rate_limiter
        .send(RateMsg::Acquire {
            key: rate_key.clone(),
            cost: 1,
            reply: permit_tx,
        })
        .await
        .map_err(|_| anyhow!("rate limiter actor dropped"))?;

    permit_rx
        .await
        .map_err(|_| anyhow!("failed to receive rate permit from limiter"))?;

    Ok(())
}

async fn store_search_artifacts(
    store: &Addr<StoreActor>,
    claim: Uuid,
    query: &str,
    limit: i64,
) -> Result<Vec<ArtifactRow>> {
    let (tx, rx) = oneshot::channel();
    store
        .send(StoreMsg::SearchArtifacts {
            claim,
            query: query.to_string(),
            limit,
            reply: tx,
        })
        .await
        .map_err(|_| anyhow!("store mailbox dropped"))?;
    let res = rx.await.map_err(|_| anyhow!("store reply dropped"))?;
    if let Err(ref err) = res {
        tracing::warn!(
            claim_id=%claim,
            query=%query,
            limit,
            error=%err,
            "llm.store_search_artifacts.error"
        );
    }
    res
}

async fn store_get_artifact(
    store: &Addr<StoreActor>,
    id: &str,
) -> anyhow::Result<ArtifactWithEntities> {
    let (tx, rx) = oneshot::channel();
    let uid = Uuid::parse_str(id)?;
    store
        .send(StoreMsg::GetArtifact {
            internal_id: uid,
            reply: tx,
        })
        .await
        .map_err(|_| anyhow::anyhow!("store mailbox dropped"))?;
    rx.await
        .map_err(|_| anyhow::anyhow!("store reply dropped"))?
}

fn parse_llm_normalization(raw: &str) -> Result<LlmNormalization> {
    if let Ok(parsed) = serde_json::from_str::<LlmNormalization>(raw) {
        return Ok(parsed);
    }

    // FIXME: replace ad-hoc brace slicing with a resilient JSON repair/parsing strategy so partial model outputs don't misparse silently.
    let start = raw
        .find('{')
        .ok_or_else(|| anyhow!("no JSON object found"))?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| anyhow!("incomplete JSON object"))?;
    let slice = &raw[start..=end];
    let parsed = serde_json::from_str::<LlmNormalization>(slice)?;
    Ok(parsed)
}

#[derive(Debug, Deserialize)]
struct LlmNormalization {
    claim_relevance: bool,
    reasoning: String,
    provenance_info: String,
    #[serde(default)]
    entities: Vec<LlmEntity>,
}

#[derive(Debug, Deserialize)]
struct LlmEntity {
    #[serde(default)]
    external_id: Option<String>,
    name: String,
    credibility: String,
    reasoning: String,
}
