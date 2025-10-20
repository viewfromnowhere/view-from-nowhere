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

    pub fn handle(&self) -> NowhereHandle {
        NowhereHandle {
            inner: self.runtime.handle().clone(),
            cancel: self.cancel.clone(),
        }
    }

    pub fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        self.runtime.block_on(fut)
    }

    pub fn shutdown(self, graceful: std::time::Duration) {
        self.cancel.cancel();
        self.runtime.shutdown_timeout(graceful);
    }
}

impl NowhereHandle {
    pub fn spawn<F, T>(&self, fut: F) -> JoinHandle<T>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.inner.spawn(fut)
    }
    pub fn cancellation(&self) -> Arc<CancellationToken> {
        self.cancel.clone()
    }
}
