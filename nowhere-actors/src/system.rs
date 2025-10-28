//! Core actor system bookkeeping for task tracking and shutdown signaling.
//!
//! Actors subscribe to the broadcast channel for cooperative shutdown, while the
//! `JoinSet` ensures spawned tasks are awaited during teardown. Future docs should
//! clarify cancellation ordering and how many outstanding tasks the channel can buffer.
use anyhow::Result;
use tokio::{sync::broadcast, task::JoinSet};

#[derive(Clone)]
pub struct ShutdownHandle {
    tx: broadcast::Sender<()>,
}

impl ShutdownHandle {
    pub fn signal(&self) {
        let _ = self.tx.send(());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.tx.subscribe()
    }
}

pub struct ActorSystem {
    joinset: JoinSet<Result<()>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl Default for ActorSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorSystem {
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(32);
        Self {
            joinset: JoinSet::new(),
            shutdown_tx,
        }
    }

    pub fn shutdown_notifier(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            tx: self.shutdown_tx.clone(),
        }
    }

    pub fn track(&mut self, fut: impl std::future::Future<Output = Result<()>> + Send + 'static) {
        self.joinset.spawn(fut);
    }

    pub async fn graceful_shutdown(mut self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        while let Some(res) = self.joinset.join_next().await {
            res??;
        }
        Ok(())
    }

    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
