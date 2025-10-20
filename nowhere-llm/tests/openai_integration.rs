mod common;
use nowhere_common::Result;
use nowhere_llm::openai::OpenAiClient;
use nowhere_llm::traits::LlmClient;
use tokio::time::{sleep, Duration};

const MODEL: &str = "gpt-4o-mini";

fn make_client_or_skip() -> OpenAiClient {
    let key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| {
        tracing::debug!("Skipping: OPENAI API KEY not set");

        panic!("SKIP");
    });

    OpenAiClient::new(key, MODEL.to_string()).expect("should work")
}

#[tokio::test]
#[ignore]
async fn openai_generate_smoketest() -> Result<()> {
    common::init_test_tracing();
    let client = make_client_or_skip();

    let mut attempts = 0;

    let response = loop {
        attempts += 1;
        match client.generate("Say Ok", None, Some(8), Some(0.2)).await {
            Ok(r) => break Ok(r),
            Err(e) => {
                let msg = e.to_string();

                let transient = msg.contains("500")
                    || msg.contains("429")
                    || msg.contains("502")
                    || msg.contains("504")
                    || msg.contains("rate")
                    || msg.contains("timeout");

                if attempts < 2 && transient {
                    sleep(Duration::from_millis(200)).await;
                    continue;
                }
                break Err(e);
            }
        }
    }?;

    tracing::debug!("OpenAi response is: {}", response.text);

    assert!(
        !response.text.trim().is_empty(),
        "response text should not be empty"
    );
    Ok(())
}
