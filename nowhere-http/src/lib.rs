//! Minimal HTTP client with safe logging, retries, and flexible auth.
//!
//! - Request options: headers, `Auth`, query params, timeout, retries
//! - Redacts sensitive query params and never logs secret values
//! - Retries 429/5xx with exponential backoff and `Retry-After` support
//! - Optional *raw* request/response logging via `NOWHERE_HTTP_RAW=1`
//!
//! Example (no_run):
//! ```rust
//! # async fn demo() -> Result<(), nowhere_http::HttpError> {
//! let client = nowhere_http::HttpClient::new("https://api.example.com")?;
//! let got: serde_json::Value = client
//!     .get_json("v1/items", nowhere_http::RequestOpts::default())
//!     .await?;
//! # Ok(()) }
//! ```
//!
//! Security: `Auth::Bearer` values are sanitized before use, and logs only
//! ever include the auth kind (bearer/header/query/none), not the secret.
//!
//! Observability: structured `tracing` events are emitted for request start,
//! headers, body snippets (truncated), retries, final errors, and (optionally)
//! raw request/response lines (target `http.raw`) when `NOWHERE_HTTP_RAW=1`.

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::{Client, Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::env;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;

// ==============================
// Raw logging toggles
// ==============================

const RAW_ENV: &str = "NOWHERE_HTTP_RAW";
const RAW_MAX_BODY: usize = 64 * 1024; // cap raw body logs (64 KiB)

fn raw_enabled() -> bool {
    matches!(
        env::var(RAW_ENV).as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Render a best-effort curl command for repro/debug, with secrets redacted.
// FIXME(secrets): Redact known secret query params in curl output (or omit query entirely)
// to avoid accidental leakage when RAW logging is enabled.
fn make_curl(method: &Method, url: &Url, headers: &HeaderMap, body: Option<&[u8]>) -> String {
    let mut parts = vec!["curl".to_string(), format!("-X{}", method)];
    // headers
    for (name, val) in headers.iter() {
        let mut v = val.to_str().unwrap_or("").to_string();
        let lname = name.as_str().to_ascii_lowercase();
        if lname == "authorization" {
            v = "Bearer <redacted>".into();
        }
        parts.push(format!(
            "-H '{}: {}'",
            name.as_str(),
            v.replace('\'', r"'\''")
        ));
    }
    // body
    if let Some(bytes) = body {
        if let Ok(s) = std::str::from_utf8(bytes) {
            let mut s = s.to_string();
            if s.len() > RAW_MAX_BODY {
                s.truncate(RAW_MAX_BODY);
                s.push('…');
            }
            parts.push(format!("-d '{}'", s.replace('\'', r"'\''")));
        } else {
            parts.push(format!("--data-binary @- # ({} bytes)", bytes.len()));
        }
    }
    parts.push(format!("'{}'", url.as_str()));
    parts.join(" ")
}

/// Redact sensitive headers for logging
fn redact_headers(h: &HeaderMap) -> Vec<(String, String)> {
    h.iter()
        .map(|(k, v)| {
            let key = k.as_str().to_string();
            let mut val = v.to_str().unwrap_or("").to_string();
            if key.eq_ignore_ascii_case("authorization") {
                val = "Bearer <redacted>".into();
            }
            (key, val)
        })
        .collect()
}

// ==============================
// Errors
// ==============================

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid URL: {0}")]
    Url(String),
    #[error("request build failed: {0}")]
    Build(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("decode error: {0}, body_snippet: {1}")]
    Decode(String, String),
    #[error("server returned error {status}: {message}, request_id={request_id}")]
    Api {
        status: StatusCode,
        message: String,
        request_id: String,
    },
}

// ==============================
// Auth & Request Options
// ==============================

/// Authentication strategies supported by the HTTP client helpers.
///
/// ```
/// use nowhere_http::Auth;
///
/// let bearer = Auth::Bearer("token");
/// match bearer {
///     Auth::Bearer(value) => assert_eq!(value, "token"),
///     _ => unreachable!(),
/// }
/// ```
#[derive(Clone, Debug)]
pub enum Auth<'a> {
    /// Authorization: Bearer <token>
    Bearer(&'a str),
    /// Custom header (e.g., Brave: X-Subscription-Token)
    Header {
        name: HeaderName,
        value: HeaderValue,
    },
    /// Auth via query param
    Query {
        name: &'a str,
        value: Cow<'a, str>,
    },
    None,
}

/// Per-request tuning knobs for the HTTP client.
///
/// ```
/// use nowhere_http::{Auth, RequestOpts};
/// use std::borrow::Cow;
/// use std::time::Duration;
///
/// let opts = RequestOpts {
///     timeout: Some(Duration::from_secs(30)),
///     retries: Some(1),
///     auth: Some(Auth::Query {
///         name: "apikey",
///         value: Cow::Borrowed("demo"),
///     }),
///     ..Default::default()
/// };
///
/// assert_eq!(opts.timeout.unwrap().as_secs(), 30);
/// assert!(opts.allow_absolute == false);
/// ```
#[derive(Clone, Debug, Default)]
pub struct RequestOpts<'a> {
    pub timeout: Option<Duration>,
    pub retries: Option<usize>,
    pub auth: Option<Auth<'a>>,
    pub headers: Option<HeaderMap>,
    pub query: Option<Vec<(&'a str, Cow<'a, str>)>>, // e.g. [("q", "term".into())]
    /// If true and `path` is an absolute URL, use it as-is (ignore base).
    pub allow_absolute: bool,
}

// ==============================
// Client
// ==============================

#[derive(Clone)]
pub struct HttpClient {
    base: Url,
    inner: Client,
    pub default_timeout: Duration,
    pub max_retries: usize,
}

impl HttpClient {
    /// Construct a client anchored to a base URL.
    ///
    /// ```no_run
    /// use nowhere_http::{HttpClient, HttpError};
    /// use std::time::Duration;
    ///
    /// let client = HttpClient::new("https://api.example.com")?;
    /// assert_eq!(client.default_timeout, Duration::from_secs(15));
    /// assert_eq!(client.max_retries, 2);
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn new(base: &str) -> Result<Self, HttpError> {
        let base = Url::parse(base).map_err(|e| HttpError::Url(e.to_string()))?;
        let inner = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| HttpError::Build(e.to_string()))?;
        Ok(Self {
            base,
            inner,
            default_timeout: Duration::from_secs(15),
            max_retries: 2,
        })
    }

    /// Override the default timeout returned by [`HttpClient::new`].
    ///
    /// ```no_run
    /// use nowhere_http::{HttpClient, HttpError};
    /// use std::time::Duration;
    ///
    /// let client = HttpClient::new("https://api.example.com")?
    ///     .with_timeout(Duration::from_secs(2));
    /// assert_eq!(client.default_timeout, Duration::from_secs(2));
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.default_timeout = dur;
        self
    }

    /// Override the default retry budget returned by [`HttpClient::new`].
    ///
    /// ```no_run
    /// use nowhere_http::{HttpClient, HttpError};
    ///
    /// let client = HttpClient::new("https://api.example.com")?.with_retries(5);
    /// assert_eq!(client.max_retries, 5);
    /// # Ok::<(), HttpError>(())
    /// ```
    pub fn with_retries(mut self, n: usize) -> Self {
        self.max_retries = n;
        self
    }

    // ==============================
    // Backward-compatible API
    // ==============================

    /// POST JSON using optional Bearer auth (backward compatible).
    pub async fn post_json<B, T>(
        &self,
        path: &str,
        bearer: Option<&str>,
        body: &B,
    ) -> Result<T, HttpError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let auth = bearer.map(Auth::Bearer);
        let opts = RequestOpts {
            auth,
            ..Default::default()
        };
        self.request_json_internal(Method::POST, path, Some(body), opts)
            .await
    }

    // ==============================
    // New generic API
    // ==============================

    /// GET JSON with per-request options (headers/query/auth/timeout/retries).
    pub async fn get_json<T>(&self, path: &str, opts: RequestOpts<'_>) -> Result<T, HttpError>
    where
        T: DeserializeOwned,
    {
        self.request_json_internal::<(), T>(Method::GET, path, None, opts)
            .await
    }

    /// POST JSON with per-request options (headers/query/auth/timeout/retries).
    pub async fn post_json_opts<B, T>(
        &self,
        path: &str,
        body: &B,
        opts: RequestOpts<'_>,
    ) -> Result<T, HttpError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.request_json_internal(Method::POST, path, Some(body), opts)
            .await
    }

    // ==============================
    // Core request implementation
    // ==============================

    // FIXME(observability): consider emitting a dedicated `tracing` span with
    // standardized `http.*` fields (e.g., `otel` conventions) and exposing
    // hooks for per-request metrics.
    async fn request_json_internal<B, T>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        mut opts: RequestOpts<'_>,
    ) -> Result<T, HttpError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        // Resolve URL (allow absolute URL when requested).
        let url = if opts.allow_absolute {
            if let Ok(abs) = Url::parse(path) {
                abs
            } else {
                self.base
                    .join(path)
                    .map_err(|e| HttpError::Url(e.to_string()))?
            }
        } else {
            self.base
                .join(path)
                .map_err(|e| HttpError::Url(e.to_string()))?
        };

        let mut attempt = 0usize;
        let max_retries = opts.retries.unwrap_or(self.max_retries);

        loop {
            // ----- Build request -----
            let mut rb = self.inner.request(method.clone(), url.clone());

            // timeout
            let timeout = opts.timeout.unwrap_or(self.default_timeout);
            rb = rb.timeout(timeout);

            // query (initial)
            if let Some(q) = &opts.query {
                let pairs: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_ref())).collect();
                rb = rb.query(&pairs);
            }

            // body (serialize if JSON so we can log exact bytes)
            let mut request_body_bytes: Option<Vec<u8>> = None;
            if let Some(b) = body {
                match serde_json::to_vec(b) {
                    Ok(bytes) => {
                        request_body_bytes = Some(bytes.clone());
                        rb = rb
                            .header(reqwest::header::CONTENT_TYPE, "application/json")
                            .body(bytes);
                    }
                    Err(_) => {
                        // fallback: let reqwest serialize; we won't have raw bytes for logging
                        rb = rb.json(b);
                    }
                }
            }

            // headers
            if let Some(hdrs) = &opts.headers {
                rb = rb.headers(hdrs.clone());
            }

            // auth
            if let Some(auth) = &opts.auth {
                match auth {
                    Auth::Bearer(tok) => {
                        let tok = sanitize_api_key(tok)?;
                        rb = rb.bearer_auth(tok);
                    }
                    Auth::Header { name, value } => {
                        rb = rb.header(name, value);
                    }
                    Auth::Query { name, value } => {
                        let mut q = opts.query.take().unwrap_or_default();
                        q.push((*name, value.clone()));
                        let pairs: Vec<(&str, &str)> =
                            q.iter().map(|(k, v)| (*k, v.as_ref())).collect();
                        rb = rb.query(&pairs);
                        opts.query = Some(q); // persist for retries
                    }
                    Auth::None => {}
                }
            }

            // ----- Safe request logging (pre-send) -----
            let auth_kind = match &opts.auth {
                Some(Auth::Bearer(_)) => "bearer",
                Some(Auth::Header { .. }) => "header",
                Some(Auth::Query { .. }) => "query",
                Some(Auth::None) | None => "none",
            };

            // Redact sensitive query params
            let redacted_q: Vec<(String, String)> = opts
                .query
                .as_ref()
                .map(|q| {
                    q.iter()
                        .map(|(k, v)| {
                            let k_lower = k.to_ascii_lowercase();
                            let is_secret = matches!(
                                k_lower.as_str(),
                                "access_token"
                                    | "authorization"
                                    | "auth"
                                    | "key"
                                    | "api_key"
                                    | "token"
                                    | "secret"
                                    | "client_secret"
                                    | "bearer"
                            );
                            (
                                (*k).to_string(),
                                if is_secret {
                                    "<redacted>".to_string()
                                } else {
                                    v.as_ref().to_string()
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Lightweight request id without extra deps
            let req_id = format!(
                "r{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let attempt0 = attempt + 1;
            // FIXME(ids): Replace ad-hoc req_id with UUID or a monotonic counter to
            // reduce collision risk under high concurrency.

            tracing::debug!(
                req_id=%req_id,
                attempt=attempt0,
                max_retries,
                method=%method,
                host_path=%format!("{}{}", url.domain().unwrap_or("-"), url.path()),
                query=?redacted_q,
                timeout_ms=timeout.as_millis() as u64,
                auth_kind,
                has_body=%body.is_some(),
                "http.request.start"
            );

            // NEW: raw request line (curl) if enabled
            if raw_enabled() {
                // Merge only caller-provided headers (auth header will be redacted anyway)
                let mut merged = HeaderMap::new();
                if let Some(h) = &opts.headers {
                    for (k, v) in h.iter() {
                        merged.append(k, v.clone());
                    }
                }
                let curl = make_curl(&method, &url, &merged, request_body_bytes.as_deref());
                tracing::debug!(target: "http.raw", %req_id, %curl, "request");
            }

            // ----- Send -----
            let t0 = std::time::Instant::now();
            let resp = match rb.send().await {
                Ok(resp) => resp,
                Err(err) => {
                    let message = err.to_string();
                    if attempt < max_retries {
                        attempt += 1;
                        let delay =
                            Duration::from_millis(200u64.saturating_mul(1 << (attempt - 1)));
                        tracing::warn!(
                            req_id=%req_id,
                            attempt,
                            max_retries,
                            backoff_ms=delay.as_millis() as u64,
                            message=%message,
                            "http.retrying.network_send"
                        );
                        sleep(delay).await;
                        continue;
                    }
                    tracing::warn!(
                        req_id=%req_id,
                        attempt,
                        max_retries,
                        message=%message,
                        "http.network_error.send"
                    );
                    return Err(HttpError::Network(message));
                }
            };
            let status = resp.status();
            let headers = resp.headers().clone();
            let bytes = match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(err) => {
                    let message = err.to_string();
                    if attempt < max_retries {
                        attempt += 1;
                        let delay =
                            Duration::from_millis(200u64.saturating_mul(1 << (attempt - 1)));
                        tracing::warn!(
                            req_id=%req_id,
                            attempt,
                            max_retries,
                            backoff_ms=delay.as_millis() as u64,
                            message=%message,
                            "http.retrying.network_body"
                        );
                        sleep(delay).await;
                        continue;
                    }
                    tracing::warn!(
                        req_id=%req_id,
                        attempt,
                        max_retries,
                        message=%message,
                        "http.network_error.body"
                    );
                    return Err(HttpError::Network(message));
                }
            };
            let dur_ms = t0.elapsed().as_millis() as u64;

            // Response header diagnostics
            let req_hdr_id = headers
                .get("x-request-id")
                .or_else(|| headers.get("x-correlation-id"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");

            let limit = headers
                .get("x-rate-limit-limit")
                .and_then(|v| v.to_str().ok());
            let remain = headers
                .get("x-rate-limit-remaining")
                .and_then(|v| v.to_str().ok());
            let reset = headers
                .get("x-rate-limit-reset")
                .and_then(|v| v.to_str().ok());

            tracing::debug!(
                req_id=%req_id,
                %status,
                duration_ms=dur_ms,
                body_len=bytes.len(),
                x_request_id=%req_hdr_id,
                rate_limit.limit=?limit,
                rate_limit.remaining=?remain,
                rate_limit.reset=?reset,
                "http.response.headers"
            );

            // NEW: raw response (headers + body)
            if raw_enabled() {
                let hdrs = redact_headers(&headers);
                let mut body_snip = bytes.clone();
                let truncated = body_snip.len() > RAW_MAX_BODY;
                if truncated {
                    body_snip.truncate(RAW_MAX_BODY);
                }
                let text = String::from_utf8_lossy(&body_snip);
                tracing::info!(
                    target:"http.raw",
                    %req_id,
                    status=%status,
                    duration_ms=dur_ms,
                    headers=?hdrs,
                    body=%text,
                    truncated
                );
            }

            let snippet = snip_body(&bytes);
            tracing::trace!(
                req_id=%req_id,
                body_snippet=%snippet,
                "http.response.body_snippet"
            );

            // ----- Success path -----
            if status.is_success() {
                // Optional: surface common Twitter meta (safe & cheap)
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    let result_count = val.get("meta").and_then(|m| m.get("result_count")).cloned();
                    let next_token = val.get("meta").and_then(|m| m.get("next_token")).cloned();
                    tracing::debug!(
                        req_id=%req_id,
                        ?result_count,
                        ?next_token,
                        "http.response.meta"
                    );
                }

                // FIXME(content-type): Validate content-type before JSON decode and/or
                // provide non-JSON helpers (get_text/get_bytes).
                return serde_json::from_slice::<T>(&bytes).map_err(|e| {
                    tracing::warn!(
                        req_id=%req_id,
                        serde_line=%e.line(),
                        serde_col=%e.column(),
                        serde_err=%e.to_string(),
                        body_snippet=%snippet,
                        "http.response.decode_error"
                    );
                    HttpError::Decode(e.to_string(), snippet)
                });
            }

            // ----- Non-success: maybe retry -----
            let message = extract_error_message_multi(&bytes);
            let request_id = req_hdr_id.to_string();

            let is_429 = status == StatusCode::TOO_MANY_REQUESTS;
            let is_5xx = status.is_server_error();

            if (is_429 || is_5xx) && attempt < max_retries {
                attempt += 1;
                // FIXME(retry-policy): Make policy pluggable with jitter and cap on total
                // elapsed time; consider honoring Retry-After for 5xx as well.
                let delay = if let Some(secs) = retry_after_delay_secs(&headers) {
                    Duration::from_secs(secs)
                } else {
                    let exp = Duration::from_millis(200u64.saturating_mul(1 << (attempt - 1)));
                    if is_429 {
                        // default floor for 429 when no Retry-After is present
                        exp.max(Duration::from_millis(1100))
                    } else {
                        exp
                    }
                };
                tracing::warn!(
                    req_id=%req_id,
                    %status,
                    attempt,
                    max_retries,
                    backoff_ms=delay.as_millis() as u64,
                    retry_after_secs=?retry_after_delay_secs(&headers),
                    message=%message,
                    body_snippet=%snippet,
                    "http.retrying"
                );
                sleep(delay).await;
                continue;
            }

            // Final error
            tracing::warn!(
                req_id=%req_id,
                %status,
                message=%message,
                x_request_id=%request_id,
                body_snippet=%snippet,
                "http.error"
            );
            return Err(HttpError::Api {
                status,
                message,
                request_id,
            });
        }
    }
}

// ==============================
// Helpers
// ==============================

// FIXME(dedup): Consolidate helper definitions and remove any duplicates.
fn extract_error_message_multi(body: &[u8]) -> String {
    // OpenAI style: {"error":{"message":"..."}}
    #[derive(Deserialize)]
    struct OpenAiEnv {
        error: OpenAiDetail,
    }
    #[derive(Deserialize)]
    struct OpenAiDetail {
        message: String,
    }

    // Twitter: {"errors":[{"message":"...", "detail":"...", "title":"..."}]}
    #[derive(Deserialize)]
    struct TwErrors {
        errors: Vec<TwErr>,
    }
    #[derive(Deserialize)]
    struct TwErr {
        #[serde(default)]
        message: String,
        #[serde(default)]
        detail: String,
        #[serde(default)]
        title: String,
    }

    // Generic: {"message":"..."} or {"detail":"..."} or {"error":"..."}
    #[derive(Deserialize)]
    struct Msg {
        #[serde(default)]
        message: String,
        #[serde(default)]
        detail: String,
        #[serde(default)]
        error: String,
    }

    if let Ok(env) = serde_json::from_slice::<OpenAiEnv>(body) {
        return env.error.message;
    }
    if let Ok(tw) = serde_json::from_slice::<TwErrors>(body) {
        if let Some(first) = tw.errors.into_iter().next() {
            if !first.message.is_empty() {
                return first.message;
            }
            if !first.detail.is_empty() {
                return first.detail;
            }
            if !first.title.is_empty() {
                return first.title;
            }
        }
    }
    if let Ok(m) = serde_json::from_slice::<Msg>(body) {
        if !m.message.is_empty() {
            return m.message;
        }
        if !m.detail.is_empty() {
            return m.detail;
        }
        if !m.error.is_empty() {
            return m.error;
        }
    }
    snip_body(body)
}

fn retry_after_delay_secs(h: &HeaderMap) -> Option<u64> {
    h.get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())?
        .parse()
        .ok()
}

fn snip_body(body: &[u8]) -> String {
    let mut snip = String::from_utf8_lossy(body).to_string();
    if snip.len() > 500 {
        snip.truncate(500);
        snip.push_str("...");
    }
    snip
}

fn sanitize_api_key(raw: &str) -> Result<String, HttpError> {
    // FIXME(strictness): Optionally validate expected key prefix/length per provider and
    // allow passing a prebuilt HeaderValue to avoid reformatting.
    // 1) Trim outer spaces/quotes
    let mut s = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string();

    // 2) Remove *all* ASCII whitespace (spaces, tabs, newlines, carriage returns)
    s.retain(|ch| !ch.is_ascii_whitespace());

    // 3) Ensure ASCII and no control chars
    if !s.is_ascii() {
        return Err(HttpError::Build("API key contains non-ASCII bytes".into()));
    }
    if s.bytes().any(|b| b < 0x20 || b == 0x7F) {
        return Err(HttpError::Build(
            "API key contains control characters".into(),
        ));
    }

    // 4) Validate header value upfront for clear errors
    HeaderValue::from_str(&format!("Bearer {}", s))
        .map_err(|e| HttpError::Build(format!("invalid Authorization header: {e}")))?;
    Ok(s)
}

fn redact_query(url: &Url) -> (String, Vec<(String, String)>) {
    // Return "host + path" string and redacted query list for logging
    let host_path = format!("{}{}", url.domain().unwrap_or("-"), url.path());
    let redacted = url
        .query_pairs()
        .map(|(k, v)| {
            let k = k.to_string();
            let v = v.to_string();
            let is_secret = matches!(
                k.to_ascii_lowercase().as_str(),
                "access_token"
                    | "authorization"
                    | "auth"
                    | "key"
                    | "api_key"
                    | "token"
                    | "secret"
                    | "client_secret"
                    | "bearer"
            );
            (k, if is_secret { "<redacted>".into() } else { v })
        })
        .collect::<Vec<_>>();
    (host_path, redacted)
}

fn content_len(headers: &HeaderMap, body_len: usize) -> usize {
    headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(body_len)
}
