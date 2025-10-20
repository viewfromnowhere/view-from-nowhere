use anyhow::{Result, anyhow};
use chrono::{DateTime, NaiveDateTime, Utc};
use nowhere_drivers::nowhere_browser::driver::NowhereDriver;
use nowhere_drivers::nowhere_browser::stealth::StealthProfile;
use nowhere_llm::traits::LlmClient;
use regex::Regex;
use url::Url;

#[derive(Debug, Clone)]
pub struct PageCapture {
    pub url: Url,
    pub html: String,
    pub screenshot_png: Option<Vec<u8>>,
    pub published_at: Option<DateTime<Utc>>,
}

#[async_trait::async_trait]
pub trait BrowserCapturer: Send + Sync {
    async fn capture(
        &self,
        url: &Url,
        headless: bool,
        profile: StealthProfile,
        llm_client: &dyn LlmClient,
    ) -> Result<PageCapture>;
}

/// Concrete capturer backed by your fantoccini-based driver.
pub struct FantocciniCapturer;

#[async_trait::async_trait]
impl BrowserCapturer for FantocciniCapturer {
    async fn capture(
        &self,
        url: &Url,
        headless: bool,
        profile: StealthProfile,
        llm_client: &dyn LlmClient,
    ) -> Result<PageCapture> {
        let mut driver = NowhereDriver::new(headless, profile).await?;
        let page = driver.goto(url.as_str()).await?;
        let html = page.get_content().await?;

        // let system_prompt = PUBDATE_FINDER_SYSTEM_PROMPT;
        // let user_prompt = build_pubdate_finder_html_prompt(&html);

        // let resp = llm_client
        //     .generate(&user_prompt, Some(system_prompt), None, Some(0.2))
        //     .await
        //     .map_err(|e| anyhow!(format!("LLM error: {e}")))?;
        //
        // let text = resp.text.trim();
        // let json = extract_json_block(text).unwrap_or_else(|| text.to_string());
        //
        // // Parse the object first, then pull the string
        // let published_at = parse_pubdate_json(&json)
        //     .map_err(|e| anyhow!("Failed to parse datetime for publication date: {e}: {json}"))?;
        //
        // Always attempt to close the driver before returning
        let result = Ok(PageCapture {
            url: url.clone(),
            html,
            screenshot_png: None,
            published_at: None,
        });
        let _ = driver.close().await;
        result
    }
}

const PUBDATE_FINDER_SYSTEM_PROMPT: &str = r#"
You are an expert HTML analyzer. Your goal is to find any publication date within the provided HTML.
Return only strict JSON as instructed by the user prompt.
"#;

fn build_pubdate_finder_html_prompt(html_string: &str) -> String {
    // Be explicit: published_at must be an RFC3339 string or null.
    format!(
        r#"
Return STRICT JSON ONLY, matching exactly this shape:

{{
  "published_at": "<RFC3339 timestamp string>" | null
}}

Rules:
- If a clear publication date exists (e.g., meta tags like datePublished, article:published_time, time tags, etc.), output it as an RFC3339 string (e.g., "2025-08-15T15:14:04+00:00").
- If you cannot find a trustworthy publication date, set "published_at" to null.
- Do not include any other properties or text.

HTML:
{html_string}
"#,
        html_string = html_string
    )
}

/// Try to extract a ```json ... ``` fenced block; fall back to raw.
fn extract_json_block(text: &str) -> Option<String> {
    let re_fence = Regex::new("(?s)```json\\s*(\\{.*?\\})\\s*```").ok()?;
    if let Some(caps) = re_fence.captures(text) {
        return Some(caps.get(1)?.as_str().to_string());
    }
    let re_plain = Regex::new("(?s)(\\{.*\\})").ok()?;
    re_plain
        .captures(text)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

/// Parse {"published_at":"..."} (and a few key aliases) into Option<DateTime<Utc>>.
fn parse_pubdate_json(json: &str) -> Result<Option<DateTime<Utc>>> {
    let v: serde_json::Value = serde_json::from_str(json)?;

    // Primary and common alias keys we might accept
    let candidates = &[
        "published_at",
        "date_published",
        "datePublished",
        "article_published_time",
        "article:published_time",
        "publishedAt",
    ];

    // Find the first present key as a string
    let mut s_opt: Option<String> = None;
    for k in candidates {
        if let Some(val) = v.get(*k) {
            if val.is_null() {
                s_opt = None;
                break;
            }
            if let Some(s) = val.as_str() {
                s_opt = Some(s.to_string());
                break;
            }
            // if it's nested like {"published_at":{"value":"..."}} try common subkey
            if let Some(obj) = val.as_object() {
                for sub in ["value", "timestamp", "time"] {
                    if let Some(serde_json::Value::String(s)) = obj.get(sub) {
                        s_opt = Some(s.to_string());
                        break;
                    }
                }
                if s_opt.is_some() {
                    break;
                }
            }
        }
    }

    // Also support the exact JSON being just {"published_at": "..."} and nothing else
    if s_opt.is_none() {
        if let Some(s) = v
            .get("published_at")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
        {
            s_opt = Some(s);
        }
    }

    let s = match s_opt {
        None => return Ok(None),
        Some(s) => s.trim().to_string(),
    };
    if s.is_empty() {
        return Ok(None);
    }

    // Try RFC3339 first (handles offsets like +00:00)
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }

    // Try naive "YYYY-MM-DDTHH:MM:SS" as UTC
    if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)));
    }

    // Try "YYYY-MM-DD" as midnight UTC
    if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        let ndt = date
            .and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| NaiveDateTime::MIN);
        return Ok(Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)));
    }

    Err(anyhow!("unrecognized date format: {}", s))
}
