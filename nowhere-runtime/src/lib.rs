use anyhow::Result;
use std::sync::Arc;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct NowhereHandle {
    inner: Handle,
    cancel: Arc<CancellationToken>,
}

pub struct NowhereRuntime {
    runtime: Runtime,
    cancel: Arc<CancellationToken>,
}

impl NowhereRuntime {
    /// Build a Tokio runtime configured for the Nowhere workspace.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    /// use std::time::Duration;
    ///
    /// let runtime = NowhereRuntime::build("doctest-runtime", Some(1))
    ///     .expect("runtime builds");
    /// let value = runtime.block_on(async { 2 + 2 });
    /// assert_eq!(value, 4);
    /// runtime.shutdown(Duration::from_millis(10));
    /// ```
    pub fn build(thread_name: &str, worker_threads: Option<usize>) -> Result<Self> {
        let mut builder = Builder::new_multi_thread();
        builder.enable_all().thread_name(thread_name);

        if let Some(workers) = worker_threads {
            builder.worker_threads(workers.max(1));
        }

        let runtime = builder.build()?;
        let cancel = Arc::new(CancellationToken::new());
        Ok(Self { runtime, cancel })
    }

    /// Obtain a cloned handle for spawning tasks and sharing cancellation.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    ///
    /// let runtime = NowhereRuntime::build("handle-example", Some(1)).unwrap();
    /// let handle = runtime.handle();
    /// assert_eq!(handle.cancellation().is_cancelled(), false);
    /// ```
    pub fn handle(&self) -> NowhereHandle {
        NowhereHandle {
            inner: self.runtime.handle().clone(),
            cancel: self.cancel.clone(),
        }
    }

    /// Run a future to completion on the runtime.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    ///
    /// let runtime = NowhereRuntime::build("block-on-example", Some(1)).unwrap();
    /// let result = runtime.block_on(async { "done" });
    /// assert_eq!(result, "done");
    /// ```
    pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.runtime.block_on(fut)
    }

    /// Cancel outstanding work and shut the runtime down gracefully.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    /// use std::time::Duration;
    ///
    /// let runtime = NowhereRuntime::build("shutdown-example", Some(1)).unwrap();
    /// runtime.shutdown(Duration::from_millis(5));
    /// ```
    pub fn shutdown(self, graceful: std::time::Duration) {
        self.cancel.cancel();
        self.runtime.shutdown_timeout(graceful);
    }
}

impl NowhereHandle {
    /// Spawn a future onto the shared runtime handle.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    /// use std::time::Duration;
    ///
    /// let runtime = NowhereRuntime::build("handle-doctest", Some(1)).unwrap();
    /// let handle = runtime.handle();
    /// let task = handle.spawn(async { 21 * 2 });
    /// let result = runtime.block_on(async move { task.await.unwrap() });
    /// assert_eq!(result, 42);
    /// runtime.shutdown(Duration::from_millis(10));
    /// ```
    pub fn spawn<F, T>(&self, fut: F) -> JoinHandle<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.spawn(fut)
    }
    /// Clone the shared cancellation token to coordinate shutdown.
    ///
    /// ```
    /// use nowhere_runtime::NowhereRuntime;
    /// use std::time::Duration;
    ///
    /// let runtime = NowhereRuntime::build("cancel-example", Some(1)).unwrap();
    /// let handle = runtime.handle();
    /// let cancel = handle.cancellation();
    /// cancel.cancel();
    /// assert!(cancel.is_cancelled());
    /// runtime.shutdown(Duration::from_millis(5));
    /// ```
    pub fn cancellation(&self) -> Arc<CancellationToken> {
        self.cancel.clone()
    }
}
