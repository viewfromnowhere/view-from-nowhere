// use super::types::{
//     BraveHit, MixedKind, NewsResult, SearchResult, VideoResult, WebSearchApiResponse,
//     WebSearchRequest, map_freshness,
// };
// use anyhow::{Context, Result};
// use nowhere_actors::{Addr, RateKey, RateLimiter, RateMsg};
// use nowhere_data::ingest::{AnyStream, DiscoveryItem, WebSearchProgram, WebSource};
// use nowhere_http::{Auth, HttpClient, RequestOpts};
// use reqwest::header::{HeaderName, HeaderValue};
// use std::borrow::Cow;
// use std::collections::HashSet;
// use std::time::Instant;
// use tokio::sync::oneshot;
// use url::Url;
//
// /// Minimal client for Brave Search API (web vertical).
// #[derive(Clone)]
// pub struct BraveApi {
//     http: HttpClient,
//     token: String,
// }
//
// impl BraveApi {
//     pub fn new(subscription_token: String) -> Self {
//         let http = HttpClient::new("https://api.search.brave.com").expect("valid base");
//         Self {
//             http,
//             token: subscription_token,
//         }
//     }
//
//     pub fn simple_search(
//         &self,
//         query: String,
//         rate: Addr<RateLimiter>,
//         rate_key: RateKey,
//     ) -> AnyStream<DiscoveryItem> {
//         let client = self.clone();
//         Box::pin(async_stream::try_stream! {
//           let mut page_idx = 0u32;
//           while page_idx < 1 {
//               // --- Rate gate per page ---
//               let started = std::time::Instant::now();
//               let (tx, rx) = oneshot::channel();
//               let _ = rate.send(RateMsg::Acquire {
//                   key: rate_key.clone(),
//                   cost: 1,
//                   reply: tx,
//               }).await;
//               tracing::trace!(target:"rate", key=%rate_key.0, "waiting for brave token");
//               let _ = rx.await;
//               tracing::trace!(target:"rate", key=%rate_key.0, waited_ms=?started.elapsed().as_millis(), "acquired brave token");
//
//               let resp = client.simple_query_search(query.clone()).await?;
//
//               // Collect URLs in Brave's display order if mixed.main is present, else fallback
//               let hits = collect_brave_hits(&resp);
//               tracing::info!(
//                   target: "web.brave",
//                   query = %query,
//                   hit_count = hits.len(),
//                   "brave.simple_search.page"
//               );
//
//               for hit in hits {
//                   yield DiscoveryItem::Web {
//                       url: Url::parse(&hit.url).ok(),
//                       source: WebSource::Brave,
//                       description: hit.description,
//                       title: hit.title,
//                   };
//               }
//               page_idx += 1;
//              }
//
//         })
//     }
//     /// Stream of `DiscoveryItem::WebUrl` honoring `max_results`.
//     /// We page in fixed steps (20), respecting freshness and optional site filters.
//     // pub fn search_stream(
//     //     &self,
//     //     program: WebSearchProgram,
//     //     rate: Addr<RateLimiter>,
//     //     rate_key: RateKey,
//     // ) -> AnyStream<DiscoveryItem> {
//     //     let client = self.clone();
//     //
//     //     Box::pin(async_stream::try_stream! {
//     //                 // Build concrete query: base query + optional (site:A OR site:B)
//     //                 let mut base_q = String::new();
//     //                 if let Some(q) = program.queries.first() {
//     //                     base_q.push_str(q.trim());
//     //                 }
//     //                 if !program.sites.is_empty() {
//     //                     let sites_expr = program.sites
//     //                         .iter()
//     //                         .filter(|s| !s.trim().is_empty())
//     //                         .map(|s| format!("site:{}", s.trim()))
//     //                         .collect::<Vec<_>>()
//     //                         .join(" OR ");
//     //                     if !sites_expr.is_empty() {
//     //                         if !base_q.is_empty() { base_q.push(' '); }
//     //                         base_q.push_str(&format!("({})", sites_expr));
//     //                     }
//     //                 }
//     //
//     //                 let mut remaining = program.max_results as i32;
//     //                 let mut page: u32 = 0;
//     //                 const BRAVE_PAGE_SIZE: u32 = 20;
//     //                 const BRAVE_MAX_PAGE: u32 = 9;
//     //                 let mut seen: HashSet<String> = HashSet::new();
//     //
//     //                 while remaining > 0 && page <= BRAVE_MAX_PAGE {
//     //                     let count = BRAVE_PAGE_SIZE.min(remaining as u32);
//     //
//     //                     let req = WebSearchRequest {
//     //                         query: base_q.clone(),
//     //                         country: None,
//     //                         search_lang: None,
//     //                         count: Some(count),
//     //                         offset: Some(page.saturating_mul(BRAVE_PAGE_SIZE)),
//     //                         freshness: map_freshness(&program.freshness),
//     //                         safesearch: Some("moderate"),
//     //                         // Default behavior: only web. To include others:
//     //                         // result_filter: Some("web,news,videos".to_string()),
//     //                         result_filter: None,
//     //                         extra_snippets: None,
//     //                         spellcheck_off: None,
//     //                         goggles_id: None,
//     //                     };
//     //
//     //                     let (tx, rx) = oneshot::channel();
//     //                     let _ = rate
//     //                          .send(RateMsg::Acquire {
//     //                              key: rate_key.clone(),
//     //                              cost: 1,
//     //                              reply: tx,
//     //                          })
//     //                     .await;
//     //                     let _ = rx.await;
//     //
//     //
//     //                     let resp = client.search_page(&req).await?;
//     //
//     //                     // Decide which verticals to include
//     //                     let (want_web, want_news, want_videos) = allowed_verticals_from_filter(req.result_filter.as_deref());
//     //
//     //                     // Collect URLs in Brave's display order if mixed.main is present, else fallback
//     //                     let mut urls = collect_urls_in_display_order(&resp, want_web, want_news, want_videos);
//     //
//     //                     // Dedupe across pages and emit
//     //                     urls.retain(|u| seen.insert(url_key(u)));
//     //
//     //                     if urls.is_empty() {
//     //                         // nothing new on this page; try next page
//     //                         if count == 0 { break; }
//     //                     }
//     //
//     //                     for url in urls {
//     //     //                    yield DiscoveryItem::WebUrl { url, source: WebSource::Brave };
//     //                         remaining -= 1;
//     //                         if remaining <= 0 { break; }
//     //                     }
//     //                     page += 1;
//     //                     if count == 0 { break; }
//     //                 }
//     //             })
//     // }
//     //
//     pub async fn simple_query_search(&self, query: String) -> Result<WebSearchApiResponse> {
//         let params = vec![("q", query.clone().into())];
//         let query_snippet = if query.len() > 160 {
//             format!("{}…", &query[..160])
//         } else {
//             query.clone()
//         };
//         let started = Instant::now();
//         tracing::info!(
//             target: "web.brave",
//             query = %query_snippet,
//             "brave.simple_query.start"
//         );
//
//         let resp: WebSearchApiResponse = match self
//             .http
//             .get_json(
//                 "res/v1/web/search",
//                 RequestOpts {
//                     auth: Some(Auth::Header {
//                         name: HeaderName::from_static("x-subscription-token"),
//                         value: HeaderValue::from_str(&self.token)
//                             .map_err(|e| nowhere_http::HttpError::Build(e.to_string()))?,
//                     }),
//                     query: Some(params),
//                     retries: Some(0),
//                     ..Default::default()
//                 },
//             )
//             .await
//         {
//             Ok(resp) => {
//                 tracing::info!(
//                     target: "web.brave",
//                     query = %query_snippet,
//                     elapsed_ms = started.elapsed().as_millis() as u64,
//                     "brave.simple_query.success"
//                 );
//                 resp
//             }
//             Err(e) => {
//                 tracing::warn!(
//                     target: "web.brave",
//                     query = %query_snippet,
//                     elapsed_ms = started.elapsed().as_millis() as u64,
//                     error = %e,
//                     "brave.simple_query.error"
//                 );
//                 return Err(anyhow::Error::new(e)).context("brave search request failed");
//             }
//         };
//         tracing::debug!(?resp, "full web search response");
//         Ok(resp)
//     }
//
//     /// Single-page call using `WebSearchRequest`, returning your `WebSearchApiResponse`.
//     pub async fn search_page(&self, req: &WebSearchRequest) -> Result<WebSearchApiResponse> {
//         let mut params: Vec<(&str, Cow<'_, str>)> = Vec::with_capacity(12);
//
//         // Required
//         params.push(("q", req.query.clone().into()));
//
//         // Optional numeric
//         if let Some(v) = req.count {
//             params.push(("count", v.to_string().into()));
//         }
//         if let Some(v) = req.offset {
//             params.push(("offset", v.to_string().into()));
//         }
//
//         // Optional strings
//         if let Some(ref v) = req.country {
//             if !v.is_empty() {
//                 params.push(("country", v.clone().into()));
//             }
//         }
//         if let Some(ref v) = req.search_lang {
//             if !v.is_empty() {
//                 params.push(("search_lang", v.clone().into()));
//             }
//         }
//         if let Some(v) = req.freshness {
//             params.push(("freshness", v.into()));
//         }
//         if let Some(v) = req.safesearch {
//             params.push(("safesearch", v.into()));
//         }
//         if let Some(ref v) = req.result_filter {
//             if !v.is_empty() {
//                 params.push(("result_filter", v.clone().into()));
//             }
//         }
//         if let Some(ref v) = req.goggles_id {
//             if !v.is_empty() {
//                 params.push(("goggles_id", v.clone().into()));
//             }
//         }
//
//         // Optional bools
//         if let Some(v) = req.extra_snippets {
//             params.push(("extra_snippets", (if v { "true" } else { "false" }).into()));
//         }
//         if let Some(v) = req.spellcheck_off {
//             params.push(("spellcheck_off", (if v { "true" } else { "false" }).into()));
//         }
//
//         // Default to "moderate" if caller did not set safesearch
//         if !params.iter().any(|(k, _)| *k == "safesearch") {
//             params.push(("safesearch", "moderate".into()));
//         }
//
//         let resp: WebSearchApiResponse = self
//             .http
//             .get_json(
//                 "res/v1/web/search",
//                 RequestOpts {
//                     auth: Some(Auth::Header {
//                         name: HeaderName::from_static("x-subscription-token"),
//                         value: HeaderValue::from_str(&self.token)
//                             .map_err(|e| nowhere_http::HttpError::Build(e.to_string()))?,
//                     }),
//                     query: Some(params),
//                     retries: Some(0),
//                     ..Default::default()
//                 },
//             )
//             .await
//             .map_err(|e| anyhow::anyhow!(e.to_string()))
//             .context("brave search request failed")?;
//         tracing::info!(?resp, "full web search response");
//         Ok(resp)
//     }
// }
//
// // Default to web-only; allow "web,news,videos" CSV to widen scope
// fn allowed_verticals_from_filter(filter: Option<&str>) -> (bool, bool, bool) {
//     match filter {
//         None => (true, false, false), // default: web only
//         Some(s) => {
//             let mut web = false;
//             let mut news = false;
//             let mut videos = false;
//             for part in s.split(',').map(|p| p.trim().to_ascii_lowercase()) {
//                 match part.as_str() {
//                     "web" | "search" => web = true,
//                     "news" => news = true,
//                     "videos" | "video" => videos = true,
//                     "_all" | "all" => {
//                         web = true;
//                         news = true;
//                         videos = true;
//                     }
//                     _ => {}
//                 }
//             }
//             (web, news, videos)
//         }
//     }
// }
//
// fn collect_brave_hits(resp: &WebSearchApiResponse) -> Vec<BraveHit> {
//     let mut out = Vec::new();
//
//     let web: Option<&Vec<SearchResult>> = resp.web.as_ref().map(|w| &w.results);
//     let news: Option<&Vec<NewsResult>> = resp.news.as_ref().map(|n| &n.results);
//     let videos: Option<&Vec<VideoResult>> = resp.videos.as_ref().map(|v| &v.results);
//
//     if let Some(mixed) = resp.mixed.as_ref() {
//         for slot in &mixed.main {
//             let take_all = slot.all.unwrap_or(false);
//             match &slot.kind {
//                 MixedKind::Web => {
//                     if take_all {
//                         if let Some(vec) = web {
//                             for it in vec {
//                                 convert_and_push_search_result(&mut out, it);
//                             }
//                         }
//                     } else if let Some(vec) = web
//                         && let Some(it) = vec.get(slot.index)
//                     {
//                         convert_and_push_search_result(&mut out, it);
//                     }
//                 }
//                 MixedKind::News => {
//                     if take_all {
//                         if let Some(vec) = news {
//                             for it in vec {
//                                 if let Some(u) = Url::parse(&it.url).ok()
//                                     && let t = &it.title
//                                     && let Some(d) = it.description.as_deref()
//                                 {
//                                     out.push(BraveHit {
//                                         rank: (out.len() + 1) as u32,
//                                         title: t.to_string(),
//                                         url: u.to_string(),
//                                         description: Some(d.to_string()),
//                                     });
//                                 }
//                             }
//                         }
//                     } else if let Some(vec) = news
//                         && let Some(it) = vec.get(slot.index)
//                         && let Ok(u) = Url::parse(it.url.as_str())
//                         && let t = &it.title
//                         && let Some(d) = &it.description
//                     {
//                         out.push(BraveHit {
//                             rank: (out.len() + 1) as u32,
//                             title: t.to_string(),
//                             url: u.to_string(),
//                             description: Some(d.to_string()),
//                         });
//                     }
//                 }
//                 MixedKind::Videos => {
//                     if take_all {
//                         if let Some(vec) = videos {
//                             for it in vec {
//                                 if let Some(u) = Url::parse(&it.url).ok()
//                                     && let t = &it.title
//                                     && let Some(d) = it.description.as_deref()
//                                 {
//                                     out.push(BraveHit {
//                                         rank: (out.len() + 1) as u32,
//                                         title: t.to_string(),
//                                         url: u.to_string(),
//                                         description: Some(d.to_string()),
//                                     });
//                                 }
//                             }
//                         }
//                     } else if let Some(vec) = news
//                         && let Some(it) = vec.get(slot.index)
//                         && let Ok(u) = Url::parse(it.url.as_str())
//                         && let t = &it.title
//                         && let Some(d) = &it.description
//                     {
//                         out.push(BraveHit {
//                             rank: (out.len() + 1) as u32,
//                             title: t.to_string(),
//                             url: u.to_string(),
//                             description: Some(d.to_string()),
//                         });
//                     }
//                 }
//                 _ => {}
//             }
//         }
//     }
//
//     out
// }
//
// fn collect_urls_in_display_order(
//     resp: &WebSearchApiResponse,
//     want_web: bool,
//     want_news: bool,
//     want_videos: bool,
// ) -> Vec<Url> {
//     let mut out = Vec::new();
//
//     // Borrow the inner Vecs (don't move them out of resp)
//     let web: Option<&Vec<SearchResult>> = resp.web.as_ref().map(|w| &w.results);
//     let news: Option<&Vec<NewsResult>> = resp.news.as_ref().map(|n| &n.results);
//     let videos: Option<&Vec<VideoResult>> = resp.videos.as_ref().map(|v| &v.results);
//
//     // 1) Preferred: use mixed.main ordering when present
//     if let Some(mixed) = resp.mixed.as_ref() {
//         for slot in &mixed.main {
//             let take_all = slot.all.unwrap_or(false);
//             match &slot.kind {
//                 MixedKind::Web if want_web => {
//                     if take_all {
//                         if let Some(vec) = web {
//                             for it in vec {
//                                 push_web_result_urls(&mut out, it);
//                             }
//                         }
//                     } else if let Some(vec) = web
//                         && let Some(it) = vec.get(slot.index)
//                     {
//                         push_web_result_urls(&mut out, it);
//                     }
//                 }
//                 MixedKind::News if want_news => {
//                     if take_all {
//                         if let Some(vec) = news {
//                             for it in vec {
//                                 if let Ok(u) = Url::parse(it.url.as_str()) {
//                                     out.push(u);
//                                 }
//                             }
//                         }
//                     } else if let Some(vec) = news
//                         && let Some(it) = vec.get(slot.index)
//                         && let Ok(u) = Url::parse(it.url.as_str())
//                     {
//                         out.push(u);
//                     }
//                 }
//                 MixedKind::Videos if want_videos => {
//                     if take_all {
//                         if let Some(vec) = videos {
//                             for it in vec {
//                                 if let Ok(u) = Url::parse(it.url.as_str()) {
//                                     out.push(u);
//                                 }
//                             }
//                         }
//                     } else if let Some(vec) = videos
//                         && let Some(it) = vec.get(slot.index)
//                         && let Ok(u) = Url::parse(it.url.as_str())
//                     {
//                         out.push(u);
//                     }
//                 }
//                 _ => {}
//             }
//         }
//     }
//
//     // 2) Fallback: if mixed was absent or yielded nothing, append verticals in order
//     if out.is_empty() {
//         if want_web {
//             if let Some(vec) = web {
//                 for it in vec {
//                     push_web_result_urls(&mut out, it);
//                 }
//             }
//         }
//         if want_news {
//             if let Some(vec) = news {
//                 for it in vec {
//                     if let Ok(u) = Url::parse(it.url.as_str()) {
//                         out.push(u);
//                     }
//                 }
//             }
//         }
//         if want_videos {
//             if let Some(vec) = videos {
//                 for it in vec {
//                     if let Ok(u) = Url::parse(it.url.as_str()) {
//                         out.push(u);
//                     }
//                 }
//             }
//         }
//     }
//
//     out
// }
//
// fn convert_and_push_search_result(out: &mut Vec<BraveHit>, it: &SearchResult) {
//     if let Some(u) = it.url.as_deref().and_then(|s| Url::parse(s).ok())
//         && let Some(t) = it.title.as_deref()
//         && let Some(d) = it.description.as_deref()
//     {
//         out.push(BraveHit {
//             rank: (out.len() + 1) as u32,
//             title: t.to_string(),
//             url: u.to_string(),
//             description: Some(d.to_string()),
//         });
//         return;
//     }
//
//     if let Some(cluster) = it.cluster.as_ref() {
//         for item in cluster {
//             if let Ok(u) = Url::parse(item.url.as_str())
//                 && let t = &item.title
//                 && let Some(d) = item.description.as_deref()
//             {
//                 out.push(BraveHit {
//                     rank: (out.len() + 1) as u32,
//                     title: t.to_string(),
//                     url: u.to_string(),
//                     description: Some(d.to_string()),
//                 });
//             }
//         }
//     }
// }
//
// /// Web results can be a plain result with `url: Option<String>`
// /// OR a "cluster" with multiple `ResultItem`s. Handle both.
// ///
// /// - If `it.url` is present, push that.
// /// - Else, expand `cluster` and push each item's URL.
// ///
// /// Parsing failures are silently ignored (keeps this utility focused).
// fn push_web_result_urls(out: &mut Vec<Url>, it: &SearchResult) {
//     if let Some(u) = it.url.as_deref().and_then(|s| Url::parse(s).ok()) {
//         out.push(u);
//         return;
//     }
//     if let Some(cluster) = it.cluster.as_ref() {
//         for item in cluster {
//             if let Ok(u) = Url::parse(item.url.as_str()) {
//                 out.push(u);
//             }
//         }
//     }
// } // Simple normalization key for deduping (drop fragment, trim trailing slash)
// fn url_key(u: &Url) -> String {
//     let mut clone = u.clone();
//     clone.set_fragment(None);
//     clone.as_str().trim_end_matches('/').to_string()
// }
