use anyhow::Result;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

/// Minimal actor trait. `Self: Sized` avoids object-safety issues when using `Context<Self>`.
#[async_trait::async_trait]
pub trait Actor: Send + Sized + 'static {
    type Msg: Send + 'static;

    /// Handle a single message. Return `Err` to stop the actor.
    async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()>;
}

/// Runtime context for an actor instance.
pub struct Context<A: Actor> {
    addr: Addr<A>,
    pub stop: bool,
}

impl<A: Actor> Context<A> {
    /// Get a clone of this actor's `Addr`.
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct SelfPing;
    /// # #[async_trait]
    /// # impl Actor for SelfPing {
    /// #     type Msg = u8;
    /// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         if msg == 0 {
    /// #             ctx.addr().try_send(1).unwrap();
    /// #         } else {
    /// #             ctx.stop();
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let actor::ActorHandle { addr, task } = actor::spawn_actor(SelfPing, 2);
    ///     addr.send(0).await.unwrap();
    ///     drop(addr);
    ///     task.await.unwrap().unwrap();
    /// });
    /// ```
    pub fn addr(&self) -> Addr<A> {
        self.addr.clone()
    }
    /// Request a graceful stop after processing the current message.
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct StopOnSecond(u8);
    /// # #[async_trait]
    /// # impl Actor for StopOnSecond {
    /// #     type Msg = u8;
    /// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         self.0 += msg;
    /// #         if self.0 >= 2 {
    /// #             ctx.stop();
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let actor::ActorHandle { addr, task } = actor::spawn_actor(StopOnSecond(0), 4);
    ///     addr.send(1).await.unwrap();
    ///     addr.send(1).await.unwrap();
    ///     drop(addr);
    ///     task.await.unwrap().unwrap();
    /// });
    /// ```
    pub fn stop(&mut self) {
        self.stop = true;
    }
}

/// Address for sending messages to an actor.
pub struct Addr<A: Actor>(mpsc::Sender<A::Msg>);

/// Manual Clone to avoid unnecessary bounds on `A`/`A::Msg`.
impl<A: Actor> Clone for Addr<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A: Actor> Addr<A> {
    /// Async send; awaits backpressure. Returns the message if the receiver is dropped.
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct Counter(u8);
    /// # #[async_trait]
    /// # impl Actor for Counter {
    /// #     type Msg = u8;
    /// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         self.0 += msg;
    /// #         if self.0 >= 3 {
    /// #             ctx.stop();
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let actor::ActorHandle { addr, task } = actor::spawn_actor(Counter(0), 4);
    ///     addr.send(1).await.unwrap();
    ///     addr.send(2).await.unwrap();
    ///     drop(addr);
    ///     task.await.unwrap().unwrap();
    /// });
    /// ```
    pub async fn send(&self, msg: A::Msg) -> std::result::Result<(), A::Msg> {
        self.0.send(msg).await.map_err(|e| e.0)
    }

    /// Try to send without waiting. Returns the message if the mailbox is full or closed.
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct Immediate;
    /// # #[async_trait]
    /// # impl Actor for Immediate {
    /// #     type Msg = &'static str;
    /// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         assert_eq!(msg, "hello");
    /// #         ctx.stop();
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let actor::ActorHandle { addr, task } = actor::spawn_actor(Immediate, 1);
    ///     addr.try_send("hello").unwrap();
    ///     drop(addr);
    ///     task.await.unwrap().unwrap();
    /// });
    /// ```
    pub fn try_send(&self, msg: A::Msg) -> std::result::Result<(), A::Msg> {
        self.0.try_send(msg).map_err(|e| e.into_inner())
    }

    /// Bounded mailbox capacity.
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct Noop;
    /// # #[async_trait]
    /// # impl Actor for Noop {
    /// #     type Msg = ();
    /// #     async fn handle(&mut self, _msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         ctx.stop();
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let actor::ActorHandle { addr, task } = actor::spawn_actor(Noop, 8);
    ///     assert_eq!(addr.capacity(), 8);
    ///     addr.send(()).await.unwrap();
    ///     drop(addr);
    ///     task.await.unwrap().unwrap();
    /// });
    /// ```
    pub fn capacity(&self) -> usize {
        self.0.max_capacity()
    }
}

/// Handle to a running actor task.
pub struct ActorHandle<A: Actor> {
    pub addr: Addr<A>,
    pub task: JoinHandle<anyhow::Result<()>>,
}

/// Spawn an actor with a bounded mailbox.
///
/// Stop conditions:
/// - `handle` returns `Err`
/// - all senders are dropped
/// - `ctx.stop()` is called
///
/// Panics: none expected inside the runtime path; prefer returning `Err`.
///
/// ```
/// # use anyhow::Result;
/// # use async_trait::async_trait;
/// # use nowhere_actors::actor::{self, Actor, Context};
/// # struct Accumulator(u8);
/// # #[async_trait]
/// # impl Actor for Accumulator {
/// #     type Msg = u8;
/// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
/// #         self.0 += msg;
/// #         if self.0 >= 5 {
/// #             ctx.stop();
/// #         }
/// #         Ok(())
/// #     }
/// # }
/// let rt = tokio::runtime::Runtime::new().unwrap();
/// rt.block_on(async {
///     let actor::ActorHandle { addr, task } = actor::spawn_actor(Accumulator(0), 8);
///     addr.send(2).await.unwrap();
///     addr.send(3).await.unwrap();
///     drop(addr);
///     task.await.unwrap().unwrap();
/// });
/// ```
pub fn spawn_actor<A: Actor>(actor: A, capacity: usize) -> ActorHandle<A> {
    spawn_actor_with_shutdown(actor, capacity, None)
}

pub fn spawn_actor_with_shutdown<A: Actor>(
    mut actor: A,
    capacity: usize,
    shutdown: Option<broadcast::Receiver<()>>,
) -> ActorHandle<A> {
    let (tx, mut rx) = mpsc::channel::<A::Msg>(capacity);
    let addr = Addr(tx);
    let addr_for_ctx = addr.clone();

    let task = tokio::spawn(async move {
        let mut ctx = Context {
            addr: addr_for_ctx,
            stop: false,
        };

        if let Some(mut shutdown_rx) = shutdown {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                    maybe_msg = rx.recv() => {
                        match maybe_msg {
                            Some(msg) => {
                                if let Err(e) = actor.handle(msg, &mut ctx).await {
                                    tracing::error!(target = "nowhere-actors", error = ?e, "actor returned error; stopping");
                                    return Err(e);
                                }
                                if ctx.stop {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        } else {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = actor.handle(msg, &mut ctx).await {
                    tracing::error!(target = "nowhere-actors", error = ?e, "actor returned error; stopping");
                    return Err(e);
                }
                if ctx.stop {
                    break;
                }
            }
        }
        Ok(())
    });

    ActorHandle { addr, task }
}

/// Reserved spawn: create mailbox+addr now; start the task later.
pub struct Reserved<A: Actor> {
    name: String,
    addr: Addr<A>,
    rx: Option<mpsc::Receiver<A::Msg>>,
}

impl<A: Actor> Reserved<A> {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn addr(&self) -> Addr<A> {
        self.addr.clone()
    }

    /// Start the actor task using the reserved mailbox (panic if called twice).
    ///
    /// ```
    /// # use anyhow::Result;
    /// # use async_trait::async_trait;
    /// # use nowhere_actors::actor::{self, Actor, Context};
    /// # struct Echo;
    /// # #[async_trait]
    /// # impl Actor for Echo {
    /// #     type Msg = &'static str;
    /// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
    /// #         assert_eq!(msg, "ping");
    /// #         ctx.stop();
    /// #         Ok(())
    /// #     }
    /// # }
    /// let rt = tokio::runtime::Runtime::new().unwrap();
    /// rt.block_on(async {
    ///     let reserved = actor::spawn_actor_reserved::<Echo>("echo", 4);
    ///     let addr = reserved.addr();
    ///     let handle = reserved.start(Echo);
    ///     addr.send("ping").await.unwrap();
    ///     drop(addr);
    ///     handle.task.await.unwrap().unwrap();
    /// });
    /// ```
    pub fn start(self, actor: A) -> ActorHandle<A> {
        self.start_with_shutdown(actor, None)
    }

    pub fn start_with_shutdown(
        mut self,
        mut actor: A,
        shutdown: Option<broadcast::Receiver<()>>,
    ) -> ActorHandle<A> {
        let mut rx = self.rx.take().expect("Reserved::start called twice");
        let addr_for_ctx = self.addr.clone();

        let task = tokio::spawn(async move {
            let mut ctx = Context {
                addr: addr_for_ctx,
                stop: false,
            };

            if let Some(mut shutdown_rx) = shutdown {
                loop {
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            break;
                        }
                        maybe_msg = rx.recv() => {
                            match maybe_msg {
                                Some(msg) => {
                                    if let Err(e) = actor.handle(msg, &mut ctx).await {
                                        tracing::error!(target = "nowhere-actors", error = ?e, "actor returned error; stopping");
                                        return Err(e);
                                    }
                                    if ctx.stop {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
            } else {
                while let Some(msg) = rx.recv().await {
                    if let Err(e) = actor.handle(msg, &mut ctx).await {
                        tracing::error!(target = "nowhere-actors", error = ?e, "actor returned error; stopping");
                        return Err(e);
                    }
                    if ctx.stop {
                        break;
                    }
                }
            }
            Ok(())
        });

        ActorHandle {
            addr: self.addr,
            task,
        }
    }
}

/// Factory for reservation.
///
/// ```
/// # use anyhow::Result;
/// # use async_trait::async_trait;
/// # use nowhere_actors::actor::{self, Actor, Context};
/// # struct Echo;
/// # #[async_trait]
/// # impl Actor for Echo {
/// #     type Msg = &'static str;
/// #     async fn handle(&mut self, msg: Self::Msg, ctx: &mut Context<Self>) -> Result<()> {
/// #         assert_eq!(msg, "ping");
/// #         ctx.stop();
/// #         Ok(())
/// #     }
/// # }
/// let rt = tokio::runtime::Runtime::new().unwrap();
/// rt.block_on(async {
///     let reserved = actor::spawn_actor_reserved::<Echo>("echo", 4);
///     let addr = reserved.addr();
///     let handle = reserved.start(Echo);
///     addr.send("ping").await.unwrap();
///     drop(addr);
///     handle.task.await.unwrap().unwrap();
/// });
/// ```
pub fn spawn_actor_reserved<A: Actor>(name: impl Into<String>, capacity: usize) -> Reserved<A> {
    let name = name.into();
    let (tx, rx) = mpsc::channel::<A::Msg>(capacity);
    let addr = Addr(tx);
    Reserved {
        name,
        addr,
        rx: Some(rx),
    }
}
