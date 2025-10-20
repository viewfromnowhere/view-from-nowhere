use crate::actor::{
    spawn_actor_reserved, spawn_actor_with_shutdown, Actor, ActorHandle, Addr, Reserved,
};
use crate::registry::Registry;
use crate::system::{ActorSystem, ShutdownHandle};
use anyhow::Result;
use std::collections::HashMap;

pub struct Builder {
    sys: ActorSystem,
    reg: Registry,
    // Concrete addresses by name for easy wiring.
    addrs: HashMap<String, Box<dyn std::any::Any + Send + Sync>>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            sys: ActorSystem::new(),
            reg: Registry::default(),
            addrs: HashMap::new(),
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        self.sys.shutdown_handle()
    }

    /// Reserve an actor and publish its `Addr` under `name`.
    pub fn reserve<A>(&mut self, name: &str, mailbox: usize) -> Reserved<A>
    where
        A: Actor,
        A::Msg: Send + 'static,
        Addr<A>: Clone + Send + Sync + 'static,
    {
        let r = spawn_actor_reserved::<A>(name.to_string(), mailbox);
        // publish immediately
        let addr = r.addr();
        self.addrs.insert(name.to_string(), Box::new(addr.clone()));
        self.reg.insert_addr::<A>(name, addr);
        r
    }

    /// Start a previously reserved actor and track its task.
    pub fn start_reserved<A>(&mut self, r: Reserved<A>, actor: A) -> &mut Self
    where
        A: Actor,
        A::Msg: Send + 'static,
        Addr<A>: Clone + Send + Sync + 'static,
    {
        let shutdown_rx = self.sys.shutdown_notifier();
        let h = r.start_with_shutdown(actor, Some(shutdown_rx));
        self.sys.track(async move {
            h.task.await??;
            Ok(())
        });
        self
    }

    /// Spawn an actor and publish its `Addr` under `name`.
    ///
    /// Necessity:
    /// - Encapsulates spawn & task tracking in one call.
    /// - Publishes typed addresses for downstream wiring.
    pub fn spawn<A, F>(&mut self, name: &str, mailbox: usize, new: F) -> &mut Self
    where
        A: Actor,
        F: Fn() -> A + Send + Sync + 'static,
        A::Msg: Send + 'static,
        Addr<A>: Clone + Send + Sync + 'static,
    {
        let shutdown_rx = self.sys.shutdown_notifier();
        let h: ActorHandle<A> = spawn_actor_with_shutdown(new(), mailbox, Some(shutdown_rx));
        let addr = h.addr.clone();
        self.sys.track(async move {
            h.task.await??;
            Ok(())
        });
        self.addrs.insert(name.to_string(), Box::new(addr.clone()));
        self.reg.insert_named::<Addr<A>>(name.to_string(), addr);
        self
    }

    /// Get a typed address by name for wiring fanout/fanin.
    pub fn addr<A: Actor>(&self, name: &str) -> Option<Addr<A>>
    where
        Addr<A>: Clone + 'static,
    {
        self.addrs
            .get(name)
            .and_then(|b| b.downcast_ref::<Addr<A>>().cloned())
    }

    pub async fn graceful_shutdown(self) -> Result<()> {
        // forward to ActorSystem’s graceful_shutdown
        self.sys.graceful_shutdown().await
    }

    /// Block until CTRL-C, then perform a graceful global shutdown.
    ///
    /// Necessity:
    /// - Provides a single place to initiate and await orderly exit.
    pub async fn run_until_ctrl_c(mut self) -> Result<()> {
        let mut shutdown_rx = self.sys.shutdown_notifier();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                let _ = shutdown_rx.recv().await;
            } => {}
        }
        // Drop published addresses/registry entries so actor mailboxes close.
        self.addrs.clear();
        self.reg = Registry::default();
        self.sys.graceful_shutdown().await
    }
}
