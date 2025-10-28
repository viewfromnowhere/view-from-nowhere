//! Minimal wrapper around the Twitter/X search API with Nowhere defaults.
//!
//! Handles auth, request parameter shaping, and safe time windows before delegating to
//! the shared HTTP client. Future documentation should cover pagination (`next_token`)
//! handling once implemented.
use crate::twitter::types::SearchResponse;
use anyhow::Result;
use nowhere_http::{Auth, HttpClient, RequestOpts};
use time::{Duration, OffsetDateTime};

#[derive(Clone)]
pub struct TwitterApi {
    http: HttpClient,
    bearer: String,
}

impl TwitterApi {
    pub fn new(bearer_token: String) -> Self {
        let http = HttpClient::new("https://api.twitter.com").expect("twitter base url");
        Self {
            http,
            bearer: bearer_token,
        }
    }

    pub async fn simple_recent_search(
        &self,
        query: String,
        max_results: Option<u32>,
        _date_from: Option<OffsetDateTime>,
        _date_to: Option<OffsetDateTime>,
    ) -> Result<SearchResponse> {
        let max_results = max_results.unwrap_or(100).clamp(10, 100);

        // Twitter constraints for /2/tweets/search/recent
        let now = OffsetDateTime::now_utc();
        // Provide some slack so the request is safely >10s behind "now".
        let latest_end = now - Duration::seconds(20);
        let earliest_start = now - Duration::days(7);

        // Derive a safe [start, end] window
        // Twitter now enforces that callers supply a window that is fully within the
        // last 7 days (and end <= now - 10s). We ignore caller-supplied dates so we
        // always request a compliant window.
        let start = earliest_start;
        let end = latest_end;

        let mut params: Vec<(&str, std::borrow::Cow<'_, str>)> = vec![
        ("query", query.into()),
        ("max_results", max_results.to_string().into()),
        ("tweet.fields",
         "created_at,lang,entities,conversation_id,public_metrics,possibly_sensitive,referenced_tweets,in_reply_to_user_id,attachments".into()),
    ];

        params.push((
            "start_time",
            start
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap()
                .into(),
        ));
        params.push((
            "end_time",
            end.format(&time::format_description::well_known::Rfc3339)
                .unwrap()
                .into(),
        ));

        let resp: SearchResponse = self
            .http
            .get_json(
                "2/tweets/search/recent",
                RequestOpts {
                    auth: Some(Auth::Bearer(&self.bearer)),
                    query: Some(params),
                    retries: Some(0),
                    ..Default::default()
                },
            )
            .await?;

        tracing::debug!("Twitter search response: {:?}", resp);
        Ok(resp)
    }
}
