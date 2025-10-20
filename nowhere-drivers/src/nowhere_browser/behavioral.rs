use anyhow::Result;
use fantoccini::elements::Element;
use rand::rngs::OsRng;
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
/// Produces human‑like delays and typing behavior to reduce automation signals.
pub struct BehavioralEngine {}

impl BehavioralEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Sleep for a random duration between `min` and `max` milliseconds.
    pub async fn random_delay(&self, min: u64, max: u64) {
        let mut rng = OsRng;
        let ms = rng.gen_range(min..=max);
        sleep(Duration::from_millis(ms)).await;
    }

    /// Type the provided text with small random delays between characters.
    pub async fn type_text_human_like(&self, element: &Element, text: &str) -> Result<()> {
        for ch in text.chars() {
            element.send_keys(&ch.to_string()).await?;
            self.random_delay(30, 150).await;
        }
        Ok(())
    }
}
