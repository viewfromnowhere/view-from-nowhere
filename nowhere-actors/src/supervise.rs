use anyhow::Result;
use std::time::Duration;
use tokio::sync::broadcast;

/// Run a fallible unit repeatedly until shutdown, with exponential backoff.
///
/// Necessity:
/// - Encapsulates restart logic; keeps actor code simple.
/// - Prevents hot-looping on immediate failures via backoff cap.
pub async fn supervise<F, Fut>(mut run_once: F, mut shutdown: broadcast::Receiver<()>) -> Result<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send + 'static,
{
    let mut backoff = Duration::from_millis(100);
    loop {
        tokio::select! {
            _ = shutdown.recv() => return Ok(()),
            res = run_once() => {
                match res {
                    Ok(()) => return Ok(()), // clean stop
                    Err(e) => {
                        tracing::warn!(error=?e, "unit crashed; restarting");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                    }
                }
            }
        }
    }
}
