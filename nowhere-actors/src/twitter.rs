//! Actor that orchestrates Twitter/X searches and forwards results to the LLM pipeline.
//!
//! It enforces rate limiting, normalizes temporal windows, and fans out fetched tweets
//! as `RawArtifact` messages. Further documentation should outline pagination strategy
//! and resilience plans for transient HTTP or auth failures.
use crate::actor::{Actor, Addr, Context};
use crate::llm::LlmActor;
use crate::rate::{RateKey, RateLimiter, RateMsg};
use crate::{ClaimContext, LlmMsg, RawArtifact, SearchCmd};
use anyhow::{anyhow, ensure, Result};
use chrono::{DateTime, Utc};
use nowhere_social::twitter::{types::SearchResponse, TwitterApi};
use time::OffsetDateTime;
use tokio::sync::oneshot;

pub struct TwitterSearchActor {
    api: TwitterApi,
    rate_key: RateKey,
    rate_limiter: Addr<RateLimiter>,
    out: Addr<LlmActor>,
    max_results: u32,
}

impl TwitterSearchActor {
    pub fn new(
        rate_limiter: Addr<RateLimiter>,
        rate_key: RateKey,
        out: Addr<LlmActor>,
        api: TwitterApi,
    ) -> Self {
        Self {
            api,
            rate_key,
            rate_limiter,
            out,
            max_results: 100,
        }
    }

    // convenience if you prefer passing the bearer here
    pub fn with_bearer(
        rate_limiter: Addr<RateLimiter>,
        rate_key: RateKey,
        out: Addr<LlmActor>,
        bearer_token: String,
    ) -> Self {
        Self::new(rate_limiter, rate_key, out, TwitterApi::new(bearer_token))
    }

    pub fn with_max_results(mut self, n: u32) -> Self {
        self.max_results = n;
        self
    }

    // FIXME: add unit tests for chrono->time conversion to ensure overflow and error branches behave as expected on boundary timestamps.
    fn chrono_to_offset(dt: DateTime<Utc>) -> Result<OffsetDateTime> {
        let nanos = dt
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("timestamp out of range for conversion: {}", dt))?;
        OffsetDateTime::from_unix_timestamp_nanos(nanos.into())
            .map_err(|e| anyhow!("failed to convert timestamp {} to OffsetDateTime: {e}", dt))
    }

    fn search_response_to_artifacts(
        &self,
        resp: SearchResponse,
        claim: ClaimContext,
    ) -> Result<Vec<RawArtifact>> {
        let SearchResponse { data, .. } = resp;

        let mut artifacts = Vec::new();
        if let Some(tweets) = data {
            artifacts.reserve(tweets.len());
            for tw in tweets {
                let tweet_id = tw.id.clone();

                let payload = serde_json::to_value(&tw)?;

                // FIXME: hydrate tweets with expansions (users, media, referenced tweets) to avoid follow-up fetches during normalization.
                artifacts.push(RawArtifact {
                    external_id: tweet_id,
                    payload,
                    claim: claim.clone(),
                });
            }
        }

        Ok(artifacts)
    }
}

#[async_trait::async_trait]
impl Actor for TwitterSearchActor {
    type Msg = SearchCmd;

    async fn handle(&mut self, msg: Self::Msg, _ctx: &mut Context<Self>) -> Result<()> {
        tracing::info!("twitter msg: {:#?}", msg);

        let SearchCmd {
            query,
            date_from,
            date_to,
            claim,
        } = msg;

        ensure!(
            date_to >= date_from,
            "invalid search window: date_to ({}) precedes date_from ({})",
            date_to,
            date_from
        );

        let (permit_tx, permit_rx) = oneshot::channel();
        self.rate_limiter
            .send(RateMsg::Acquire {
                key: self.rate_key.clone(),
                cost: 1,
                reply: permit_tx,
            })
            .await
            .map_err(|_| anyhow!("rate limiter actor dropped"))?;

        permit_rx
            .await
            .map_err(|_| anyhow!("failed to receive rate permit from limiter"))?;

        let resp = self
            // FIXME: implement retry/backoff for transient HTTP/429 errors instead of erroring out immediately.
            .api
            .simple_recent_search(
                query,
                Some(self.max_results),
                Some(Self::chrono_to_offset(date_from)?),
                Some(Self::chrono_to_offset(date_to)?),
            )
            // FIXME: paginate through `next_token` so long-running claims can gather more than one page of tweets.
            .await?;

        for artifact in self.search_response_to_artifacts(resp, claim)? {
            if let Err(msg) = self.out.send(LlmMsg::NormalizeArtifact(artifact)).await {
                return Err(anyhow!(
                    "normalize actor mailbox dropped (artifact={})",
                    match msg {
                        LlmMsg::NormalizeArtifact(raw_artifact) => {
                            raw_artifact.external_id
                        }
                        _ => {
                            String::new()
                        }
                    }
                ));
            }
        }

        Ok(())
    }
}
