use anyhow::Result;
use nowhere_actors::{
    actor::{Addr, Reserved},
    builder::Builder,
    llm::{ChatLlmActor, LlmActor},
    rate::{RateKey, RateLimiter, RateMsg},
    store::StoreActor,
    twitter::TwitterSearchActor,
};
use nowhere_config::{ActorDetails, LlmConfig, NowhereConfig};
use nowhere_llm::{ollama::OllamaClient, openai::OpenAiClient, traits::LlmClient};
use nowhere_tui::{TuiActor, spawn_tui_feeders};
use sqlx::SqlitePool;
use std::sync::Arc;

const DEFAULT_MAILBOX: usize = 1024;

pub struct Tether {
    builder: Builder,
}

impl Tether {
    pub fn new() -> Self {
        Self {
            builder: Builder::new(),
        }
    }
    pub fn builder_mut(&mut self) -> &mut Builder {
        &mut self.builder
    }
    pub async fn run(self) -> Result<()> {
        self.builder.run_until_ctrl_c().await
    }
}

// helpers
fn llm_rate_key(spec_id: &str) -> RateKey {
    RateKey(format!("llm:{spec_id}"))
}
fn twitter_rate_key(spec_id: &str) -> RateKey {
    RateKey(format!("tw:search:{spec_id}"))
}
fn chat_llm_rate_key(spec_id: &str) -> RateKey {
    RateKey(format!("llm:chat:{spec_id}"))
}

async fn make_pool_from_env() -> Result<SqlitePool> {
    let url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL not set (e.g. sqlite://nowhere.db)");
    let pool = SqlitePool::connect(&url).await?;
    Ok(pool)
}

pub async fn build_from_config(t: &mut Tether, cfg: NowhereConfig) -> Result<()> {
    let b = t.builder_mut();
    let shutdown = b.shutdown_handle();

    // -------- PHASE 1: RESERVE EVERYTHING --------
    use std::collections::HashMap;
    let mut r_llm: HashMap<String, Reserved<LlmActor>> = HashMap::new();
    let mut r_chat_llm: HashMap<String, Reserved<ChatLlmActor>> = HashMap::new();
    let mut r_tw: HashMap<String, Vec<Reserved<TwitterSearchActor>>> = HashMap::new();

    // infra
    let r_rate = b.reserve::<RateLimiter>("rate:main", 1024);
    let r_store = b.reserve::<StoreActor>("store:main", 1024);

    // ui (start last)
    let r_tui = b.reserve::<TuiActor>("tui:main", 256);
    // let r_tui_store = b.reserve::<StoreActor>("store:tui", 1024);

    // app actors
    for spec in cfg.actors.iter().filter(|a| a.enabled.unwrap_or(true)) {
        let conc = spec.concurrency.unwrap_or(1).max(1) as usize;

        match &spec.details {
            ActorDetails::Llm { .. } => {
                r_llm.insert(spec.id.clone(), b.reserve::<LlmActor>(&spec.id, 1024));
                let chat_name = format!("{}#chat", spec.id);
                r_chat_llm.insert(spec.id.clone(), b.reserve::<ChatLlmActor>(&chat_name, 1024));
            }
            ActorDetails::Twitter { .. } => {
                let mut v = Vec::with_capacity(conc);
                for i in 0..conc {
                    let name = format!("{}#{}", spec.id, i);
                    v.push(b.reserve::<TwitterSearchActor>(&name, 1024));
                }
                r_tw.insert(spec.id.clone(), v);
            }
        }
    }

    // -------- PHASE 2a: START INFRA FIRST --------
    // Start RateLimiter and Store so we can provision keys and wire outputs.
    let rate = RateLimiter::new();
    b.start_reserved(r_rate, rate);
    // FIXME: surface database connection errors instead of panicking so the TUI can report configuration issues.
    let pool = make_pool_from_env().await.unwrap();
    let store = StoreActor::new(pool.clone());
    // let tui_store = StoreActor::new(pool.clone());
    b.start_reserved(r_store, store);
    // b.start_reserved(r_tui_store, tui_store);

    // Resolve infra addrs
    let rate_addr: Addr<RateLimiter> = b.addr("rate:main").expect("rate addr");
    let store_addr: Addr<StoreActor> = b.addr("store:main").expect("store addr");
    // let tui_store_addr: Addr<StoreActor> = b.addr("store:tui").expect("tui_store addr");

    // -------- PHASE 2b: PROVISION RATE LIMITS (policy lives here) --------
    // Example defaults — make these come from config if you want.
    // LLM limits (per LLM spec)
    for spec in cfg.actors.iter().filter(|a| a.enabled.unwrap_or(true)) {
        if let ActorDetails::Llm { .. } = &spec.details {
            let key = llm_rate_key(&spec.id);
            // FIXME: surface failures from the rate-limiter mailbox instead of discarding them; currently rate limiting silently disables itself.
            let _ = rate_addr.try_send(RateMsg::Upsert {
                key: key.clone(),
                qps: 1.0, // e.g., 1 request/sec
                burst: 5,
            });
            let chat_key = chat_llm_rate_key(&spec.id);
            let _ = rate_addr.try_send(RateMsg::Upsert {
                key: chat_key.clone(),
                qps: 1.0,
                burst: 5,
            });
        }
    }
    // Twitter limits (pooled per spec across workers)
    for spec in cfg.actors.iter().filter(|a| a.enabled.unwrap_or(true)) {
        if let ActorDetails::Twitter { .. } = &spec.details {
            let key = twitter_rate_key(&spec.id);
            // FIXME: propagate mailbox send errors so we can alert when rate limiter is overloaded or stopped.
            let _ = rate_addr.try_send(RateMsg::Upsert {
                key: key.clone(),
                qps: 3.0, // tune per bearer token/account
                burst: 30,
            });
        }
    }

    // -------- PHASE 2c: START APP ACTORS (deps injected) --------
    for spec in cfg.actors.iter().filter(|a| a.enabled.unwrap_or(true)) {
        match &spec.details {
            ActorDetails::Llm { config } => {
                let client = build_llm_client(config).await?;
                let key = llm_rate_key(&spec.id);
                let chat_key = chat_llm_rate_key(&spec.id);

                let r = r_llm.remove(&spec.id).expect("reserved LlmActor");
                let actor = LlmActor::new(
                    rate_addr.clone(),
                    key.clone(),
                    store_addr.clone(),
                    client.clone(),
                )
                .with_rate_key(key.clone());

                b.start_reserved(r, actor);

                if let Some(chat_reserved) = r_chat_llm.remove(&spec.id) {
                    let chat_actor = ChatLlmActor::new(
                        rate_addr.clone(),
                        chat_key.clone(),
                        store_addr.clone(),
                        client.clone(),
                    )
                    .with_rate_key(chat_key.clone());
                    b.start_reserved(chat_reserved, chat_actor);
                }
            }

            ActorDetails::Twitter { config } => {
                let llm_id = "llm:main".to_string();
                let llm_addr: Addr<LlmActor> = b
                    .addr(&llm_id)
                    .unwrap_or_else(|| panic!("missing LLM dep '{llm_id}'"));

                let shared_key = twitter_rate_key(&spec.id); // pooled
                // let per_worker_key = |idx| RateKey(format!("tw:search:{}#{}", spec.id, idx)); // alt

                if let Some(workers) = r_tw.remove(&spec.id) {
                    for r in workers.into_iter() {
                        let actor = TwitterSearchActor::with_bearer(
                            rate_addr.clone(),
                            shared_key.clone(), // or per_worker_key(idx)
                            llm_addr.clone(),
                            config.auth_token.clone(),
                        );
                        b.start_reserved(r, actor);
                    }
                }
            }
        }
    }

    // -------- PHASE 3: START TUI LAST --------
    {
        let llm_addr: Addr<LlmActor> = b.addr("llm:main").expect("llm addr");
        let chat_llm_addr: Addr<ChatLlmActor> = b.addr("llm:main#chat").expect("chat llm addr");
        // FIXME: fan-in messages from all Twitter workers instead of hard-coding #0 so higher concurrency actually reaches the TUI.
        let tw0: Addr<TwitterSearchActor> = b.addr("twitter:ingest#0").expect("twitter addr"); // optional

        let tui = TuiActor::new(llm_addr, chat_llm_addr, tw0, store_addr, shutdown.clone())?;
        b.start_reserved(r_tui, tui);

        let tui_addr: Addr<TuiActor> = b.addr("tui:main").unwrap();
        spawn_tui_feeders(tui_addr, shutdown);
    }

    Ok(())
}
pub async fn build_llm_client(cfg: &LlmConfig) -> Result<Arc<dyn LlmClient + Send + Sync>> {
    match cfg {
        LlmConfig::Openai {
            model, auth_token, ..
        } => {
            // FIXME: thread through configurable endpoint/temperature/max_tokens instead of relying on client defaults.
            // sync constructor
            let client = OpenAiClient::new(auth_token.clone(), model.clone())?;
            Ok(Arc::new(client))
        }
        LlmConfig::Ollama {
            model, endpoint, ..
        } => {
            // FIXME: reuse a shared client per endpoint to avoid reconnecting for each actor instance.
            let client = OllamaClient::new(endpoint.clone(), model.clone()).await?;
            Ok(Arc::new(client))
        }
    }
}
