use crate::actor::{Actor, Context};
use anyhow::Result;
use std::{collections::HashMap, time::Duration};
use tokio::{
    sync::oneshot,
    time::{sleep, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RateKey(pub String);

#[derive(Debug)]
pub enum RateMsg {
    /// Insert/update bucket config.
    Upsert { key: RateKey, qps: f64, burst: u32 },
    /// Acquire `cost` tokens; replies when allowed.
    Acquire {
        key: RateKey,
        cost: u32,
        reply: oneshot::Sender<RatePermit>,
    },
}

#[derive(Debug)]
pub struct RatePermit; // no-op token (ack)

/// Token-bucket rate limiter as an actor.
///
/// Semantics:
/// - `Upsert` creates or updates the bucket for a `RateKey`.
/// - `Acquire` waits (off-actor) until `cost` tokens are available, then replies.
///
/// Throughput: controlled by `qps` (steady rate) and `burst` (bucket capacity).

#[derive(Clone, Copy, Debug)]
struct BucketCfg {
    qps: f64,
    burst: f64,
}

#[derive(Debug)]
struct BucketState {
    cfg: BucketCfg,
    tokens: f64,
    last: Instant,
}

impl BucketState {
    fn new(cfg: BucketCfg) -> Self {
        Self {
            cfg,
            tokens: cfg.burst,
            last: Instant::now(),
        }
    }

    /// Returns wait time needed to have `need` tokens available (0 if ready).
    fn needed_wait(&mut self, need: f64, now: Instant) -> Duration {
        // refill
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.cfg.qps).min(self.cfg.burst);

        if self.tokens >= need {
            self.tokens -= need;
            Duration::from_millis(0)
        } else {
            let deficit = need - self.tokens;
            // FIXME: guard against zero or extremely low qps values to avoid inf/nan wait computations.
            let secs = deficit / self.cfg.qps;
            // Reserve the tokens to avoid stampede after sleep
            self.tokens = 0.0;
            Duration::from_secs_f64(secs.max(0.0))
        }
    }
}

// FIXME: add unit tests covering bursts, refill timing, and multiple concurrent `Acquire` callers so rate limiting regressions surface quickly.
pub struct RateLimiter {
    buckets: HashMap<RateKey, BucketState>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }

    fn upsert(&mut self, key: RateKey, qps: f64, burst: u32) {
        let cfg = BucketCfg {
            qps,
            burst: burst as f64,
        };
        self.buckets
            .entry(key)
            .and_modify(|b| b.cfg = cfg)
            .or_insert_with(|| BucketState::new(cfg));
    }
}

#[async_trait::async_trait]
impl Actor for RateLimiter {
    type Msg = RateMsg;

    async fn handle(&mut self, msg: Self::Msg, _ctx: &mut Context<Self>) -> Result<()> {
        match msg {
            RateMsg::Upsert { key, qps, burst } => {
                self.upsert(key, qps, burst);
            }
            RateMsg::Acquire { key, cost, reply } => {
                let now = Instant::now();
                let state = self.buckets.entry(key.clone()).or_insert_with(|| {
                    BucketState::new(BucketCfg {
                        qps: 1.0,
                        burst: 1.0,
                    })
                });
                let wait = state.needed_wait(cost as f64, now);
                // Do not block the actor; wait and reply in a detached task.
                // FIXME: attach tracing instrumentation or cancellation so these detached tasks don't accumulate unbounded on long waits.
                tokio::spawn(async move {
                    if !wait.is_zero() {
                        sleep(wait).await;
                    }
                    let _ = reply.send(RatePermit);
                });
            }
        }
        Ok(())
    }
}
