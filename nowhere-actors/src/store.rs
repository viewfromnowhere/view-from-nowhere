//! SQLite-backed persistence actor for claims, artifacts, and entities.
//!
//! Responsibilities include serialized write coordination, FTS-backed searches, and
//! watcher fan-out when artifacts relevant to a claim arrive. More detailed docs should
//! describe the schema expectations, concurrency model, and error propagation strategy.
use crate::actor::Actor;
use crate::actor::Context;
use crate::ClaimContext;
use crate::{
    ArtifactRow, ArtifactWithEntities, Credibility, EntityRow, NormalizedArtifact, StoreMsg,
};
use anyhow::Result;
use sqlx::{Row, SqlitePool};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{oneshot, Semaphore};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct StoreActor {
    pool: SqlitePool,
    // FIXME: expose the write semaphore size via configuration so heavy ingest can batch more than one write at a time.
    write_limit: Arc<Semaphore>,
    watchers: HashMap<Uuid, Vec<oneshot::Sender<()>>>,
}

impl StoreActor {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            write_limit: Arc::new(Semaphore::new(1)),
            watchers: HashMap::new(),
        }
    }
}

// FIXME: cover store message handling end-to-end with tests (claim inserts, artifact upserts, watcher notifications) to prevent regressions in the async spawning logic.
#[async_trait::async_trait]
impl Actor for StoreActor {
    type Msg = StoreMsg;

    async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
        match msg {
            StoreMsg::InsertClaim(c) => {
                let pool = self.pool.clone();
                let permit_src = self.write_limit.clone();
                // FIXME: handle the JoinHandle so panics bubble up instead of being silently dropped.
                tokio::spawn(async move {
                    let permit = match permit_src.acquire_owned().await {
                        Ok(permit) => permit,
                        Err(err) => {
                            error!(error = ?err, "store.insert_claim.acquire_failed");
                            return;
                        }
                    };
                    if let Err(err) = insert_claim(&pool, c).await {
                        error!(error = ?err, "store.insert_claim.failed");
                    }
                    drop(permit);
                });
            }
            StoreMsg::UpsertArtifact(n) => {
                let pool = self.pool.clone();
                let permit_src = self.write_limit.clone();
                let me = ctx.addr();
                let claim_id = n.claim_id;
                let relevant = n.claim_relevance;
                // FIXME: restructure to propagate errors back to callers rather than only logging them.
                tokio::spawn(async move {
                    let permit = match permit_src.acquire_owned().await {
                        Ok(permit) => permit,
                        Err(err) => {
                            error!(error = ?err, "store.upsert.acquire_failed");
                            return;
                        }
                    };
                    if let Err(err) = upsert_normalized(&pool, n).await {
                        error!(error = ?err, "store.upsert.failed");
                    } else if relevant {
                        let _ = me
                            .send(StoreMsg::ArtifactUpserted { claim: claim_id })
                            .await;
                    }
                    drop(permit);
                });
            }

            StoreMsg::GetArtifact { internal_id, reply } => {
                let pool = self.pool.clone();
                let id = internal_id.to_string();
                tokio::spawn(async move {
                    let res = get_artifact_with_entities(&pool, &id).await;
                    if reply.send(res).is_err() {
                        debug!("store.get_artifact.reply_dropped");
                    }
                });
            }
            StoreMsg::WatchArtifacts { claim, reply } => {
                let entry = self.watchers.entry(claim).or_default();
                entry.retain(|tx| !tx.is_closed());
                entry.push(reply);
            }
            StoreMsg::ArtifactUpserted { claim } => {
                if let Some(listeners) = self.watchers.remove(&claim) {
                    for tx in listeners {
                        let _ = tx.send(());
                    }
                }
            }

            StoreMsg::SearchArtifacts {
                claim,
                query,
                limit,
                reply,
            } => {
                let pool = self.pool.clone();
                tokio::spawn(async move {
                    let res = search_artifacts_fts(&pool, &query, claim, limit).await;
                    if reply.send(res).is_err() {
                        debug!("store.search_artifacts.reply_dropped");
                    }
                });
            }

            StoreMsg::ListEntitiesByName { name, limit, reply } => {
                let pool = self.pool.clone();
                tokio::spawn(async move {
                    let res = list_entities_by_name(&pool, &name, limit).await;
                    if reply.send(res).is_err() {
                        debug!("store.list_entities.reply_dropped");
                    }
                });
            }
        }
        Ok(())
    }
}

pub async fn search_artifacts_fts(
    pool: &SqlitePool,
    q: &str,
    claim_id: Uuid,
    limit: i64,
) -> anyhow::Result<Vec<ArtifactRow>> {
    tracing::debug!(
        claim_id=%claim_id,
        query=%q,
        limit,
        "store.search_artifacts_fts.start"
    );
    let sanitized = sanitize_fts_query(q);
    if sanitized.is_none() {
        tracing::info!(
            claim_id=%claim_id,
            query=%q,
            "store.search_artifacts_fts.skip_fts"
        );
    }
    let mut rows = if let Some(ref fts_query) = sanitized {
        // Restrict to this claim + relevant only
        sqlx::query(
            r#"
            SELECT
              a.internal_id,
              a.external_id,
              a.claim_relevance,
              substr(a.reasoning, 1, 2000)       AS reasoning,
              substr(a.provenance_info, 1, 2000) AS provenance_info,
              a.claim_id
            FROM fts_artifact
            JOIN normalized_artifact a ON a.rowid = fts_artifact.rowid
            WHERE a.claim_relevance = 1
              AND a.claim_id = ?
              AND fts_artifact MATCH ?
            -- If your SQLite supports it, this gives nicer relevance ordering:
            ORDER BY bm25(fts_artifact) ASC
            LIMIT ?
            "#,
        )
        .bind(claim_id.to_string())
        .bind(fts_query)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };
    tracing::debug!(
        claim_id=%claim_id,
        query=%q,
        sanitized=?sanitized,
        initial_rows=rows.len(),
        "store.search_artifacts_fts.initial_result"
    );
    let used_fallback;
    if rows.is_empty() {
        tracing::info!(
            claim_id=%claim_id,
            query=%q,
            limit,
            "store.search_artifacts_fts.fallback_query"
        );
        rows = match sqlx::query(
            r#"
            SELECT
              internal_id,
              external_id,
              claim_relevance,
              substr(reasoning, 1, 2000)       AS reasoning,
              substr(provenance_info, 1, 2000) AS provenance_info,
              claim_id
            FROM normalized_artifact
            WHERE claim_relevance = 1
              AND claim_id = ?
            ORDER BY updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(claim_id.to_string())
        .bind(limit)
        .fetch_all(pool)
        .await
        {
            Ok(fallback_rows) => fallback_rows,
            Err(err) => {
                tracing::warn!(
                    claim_id=%claim_id,
                    query=%q,
                    limit,
                    error=%err,
                    "store.search_artifacts_fts.fallback_error"
                );
                return Err(err.into());
            }
        };
        used_fallback = true;
        tracing::debug!(
            claim_id=%claim_id,
            fallback_rows=rows.len(),
            "store.search_artifacts_fts.fallback_result"
        );
    } else {
        used_fallback = false;
    }
    info!(
        claim_id=%claim_id,
        query=%q,
        rows=rows.len(),
        fallback=used_fallback,
        "store.search_artifacts_fts"
    );

    Ok(rows
        .into_iter()
        .map(|r| ArtifactRow {
            internal_id: r.try_get::<String, _>("internal_id").unwrap_or_default(),
            external_id: r.try_get::<String, _>("external_id").unwrap_or_default(),
            claim_relevance: r.try_get::<i64, _>("claim_relevance").unwrap_or(0) != 0,
            reasoning: r.try_get::<String, _>("reasoning").unwrap_or_default(),
            provenance_info: r
                .try_get::<String, _>("provenance_info")
                .unwrap_or_default(),
            // NOTE: claim_id is nullable in the schema
            claim_id: r.try_get::<Option<String>, _>("claim_id").unwrap_or(None),
        })
        .collect())
}

pub async fn search_artifacts_like(
    pool: &SqlitePool,
    q: &str,
    claim_id: Option<Uuid>,
    limit: i64,
) -> anyhow::Result<Vec<ArtifactRow>> {
    let pat = format!("%{}%", q);
    let (cid1, cid2) = match claim_id {
        Some(c) => (Some(c.to_string()), Some(c.to_string())),
        None => (None, None),
    };

    let rows = sqlx::query(
        r#"
        SELECT
          a.internal_id,
          a.external_id,
          a.claim_relevance,
          substr(a.reasoning, 1, 2000)       AS reasoning,
          substr(a.provenance_info, 1, 2000) AS provenance_info,
          a.claim_id
        FROM normalized_artifact a
        WHERE a.claim_relevance = 1
          AND (?1 IS NULL OR a.claim_id = ?2)
          AND (a.reasoning LIKE ?3 OR a.provenance_info LIKE ?3 OR a.external_id LIKE ?3)
        ORDER BY a.updated_at DESC
        LIMIT ?4
        "#,
    )
    .bind(cid1) // ?1
    .bind(cid2) // ?2
    .bind(pat) // ?3
    .bind(limit) // ?4
    .fetch_all(pool)
    .await?;
    info!(
        query=%q,
        claim_id=?claim_id,
        rows=rows.len(),
        "store.search_artifacts_like"
    );

    Ok(rows
        .into_iter()
        .map(|r| ArtifactRow {
            internal_id: r.try_get::<String, _>("internal_id").unwrap_or_default(),
            external_id: r.try_get::<String, _>("external_id").unwrap_or_default(),
            claim_relevance: r.try_get::<i64, _>("claim_relevance").unwrap_or(0) != 0,
            reasoning: r.try_get::<String, _>("reasoning").unwrap_or_default(),
            provenance_info: r
                .try_get::<String, _>("provenance_info")
                .unwrap_or_default(),
            claim_id: r.try_get::<Option<String>, _>("claim_id").unwrap_or(None),
        })
        .collect())
}

async fn insert_claim(pool: &SqlitePool, c: ClaimContext) -> Result<()> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        r#"INSERT INTO claim
        (id, text)
        VALUES (?1, ?2)
    "#,
    )
    .bind(c.id.to_string())
    .bind(c.text)
    .execute(&mut *tx)
    .await?;
    info!(
        claim_id=%c.id,
        rows=res.rows_affected(),
        "store.insert_claim"
    );
    tx.commit().await?;
    Ok(())
}

async fn upsert_normalized(pool: &SqlitePool, n: NormalizedArtifact) -> Result<()> {
    // Single txn for artifact + entities (faster + atomic)
    let mut tx = pool.begin().await?;

    let res_artifact = sqlx::query(
        r#"INSERT INTO normalized_artifact
           (internal_id, external_id, claim_relevance, reasoning, provenance_info, claim_id)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)
           ON CONFLICT(external_id) DO UPDATE SET
             claim_relevance=excluded.claim_relevance,
             reasoning=excluded.reasoning,
             provenance_info=excluded.provenance_info,
             claim_id=excluded.claim_id"#,
    )
    .bind(n.internal_id.to_string())
    .bind(n.external_id.as_str())
    .bind(n.claim_relevance)
    .bind(n.reasoning.as_str())
    .bind(n.provenance_info.as_str())
    .bind(n.claim_id.to_string())
    .execute(&mut *tx)
    .await?;
    info!(
        internal_id=%n.internal_id,
        external_id=%n.external_id,
        claim_id=%n.claim_id,
        rows=res_artifact.rows_affected(),
        "store.upsert_normalized.artifact"
    );

    let mut entity_writes = 0u64;
    let entity_count = n.entities.len();
    for e in &n.entities {
        let credibility_s = match &e.credibility {
            Credibility::Strong => "strong",
            Credibility::Weak => "weak",
            Credibility::Unknown => "unknown",
        };
        let res_entity = sqlx::query(
            r#"INSERT INTO entity (article_id, external_id, name, credibility, reasoning)
               VALUES (?1, ?2, ?3, ?4, ?5)
               ON CONFLICT(article_id, external_id) DO UPDATE SET
                 name=excluded.name,
                 credibility=excluded.credibility,
                 reasoning=excluded.reasoning"#,
        )
        .bind(e.article_id.to_string())
        .bind(e.external_id.as_str())
        .bind(e.name.as_str())
        .bind(credibility_s)
        .bind(e.reasoning.as_str())
        .execute(&mut *tx)
        .await?;
        entity_writes += res_entity.rows_affected();
    }

    tx.commit().await?;
    info!(
        internal_id=%n.internal_id,
        entities=entity_count,
        rows_written=entity_writes,
        "store.upsert_normalized.entities"
    );
    Ok(())
}

async fn get_artifact_with_entities(pool: &SqlitePool, id: &str) -> Result<ArtifactWithEntities> {
    let a = sqlx::query(
        r#"SELECT internal_id, external_id, claim_relevance, reasoning, provenance_info, claim_id
           FROM v_artifact WHERE internal_id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let a = match a {
        Some(row) => {
            info!(artifact_id=%id, "store.artifact_found");
            row
        }
        None => {
            warn!(artifact_id=%id, "store.artifact_missing");
            return Err(anyhow::anyhow!("artifact not found"));
        }
    };

    let rows = sqlx::query(
        r#"SELECT id, article_id, name, credibility, reasoning
           FROM v_entity WHERE article_id = ? ORDER BY created_at ASC"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    info!(
        artifact_id=%id,
        entity_count=rows.len(),
        "store.entities_for_artifact"
    );

    Ok(ArtifactWithEntities {
        artifact: ArtifactRow {
            internal_id: a.try_get("internal_id")?,
            external_id: a.try_get("external_id")?,
            claim_relevance: a.try_get::<i64, _>("claim_relevance")? != 0,
            reasoning: a.try_get("reasoning")?,
            provenance_info: a.try_get("provenance_info")?,
            claim_id: a.try_get("claim_id")?,
        },
        entities: rows
            .into_iter()
            .map(|r| EntityRow {
                id: r.try_get("id").unwrap_or_default(),
                article_id: r.try_get("article_id").unwrap_or_default(),
                name: r.try_get("name").unwrap_or_default(),
                credibility: r.try_get("credibility").unwrap_or_default(),
                reasoning: r.try_get("reasoning").unwrap_or_default(),
            })
            .collect(),
    })
}

async fn list_entities_by_name(
    pool: &SqlitePool,
    name: &str,
    limit: i64,
) -> Result<Vec<EntityRow>> {
    let rows = sqlx::query(
        r#"SELECT id, article_id, name, credibility, reasoning
           FROM v_entity WHERE name = ? ORDER BY created_at DESC LIMIT ?"#,
    )
    .bind(name)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| EntityRow {
            id: r.try_get("id").unwrap_or_default(),
            article_id: r.try_get("article_id").unwrap_or_default(),
            name: r.try_get("name").unwrap_or_default(),
            credibility: r.try_get("credibility").unwrap_or_default(),
            reasoning: r.try_get("reasoning").unwrap_or_default(),
        })
        .collect())
}

fn sanitize_fts_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .filter_map(|word| {
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned.to_ascii_lowercase())
            }
        })
        .collect();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}
