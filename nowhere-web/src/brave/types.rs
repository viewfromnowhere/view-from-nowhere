// use nowhere_data::prelude::{Freshness, SafeSearch, Verticals};
// use serde::{Deserialize, Serialize};
// use uuid::Uuid;
// /// Request parameters for Brave Web Search API.
// #[derive(Debug, Clone, Serialize)]
// pub struct WebSearchRequest {
//     /// Query string
//     #[serde(rename = "q")]
//     pub query: String,
//
//     /// Country code (ISO 3166-1 alpha-2)
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub country: Option<String>,
//
//     /// Search language (ISO 639-1, e.g., "en")
//     #[serde(rename = "search_lang", skip_serializing_if = "Option::is_none")]
//     pub search_lang: Option<String>,
//
//     /// Results per page (max depends on Brave, usually up to 20–50)
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub count: Option<u32>,
//
//     /// Offset for pagination
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub offset: Option<u32>,
//
//     /// Freshness: "pd" (day), "pw" (week), "pm" (month), "py" (year)
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub freshness: Option<&'static str>,
//
//     /// Safe search filter
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub safesearch: Option<&'static str>, // "off" | "moderate" | "strict"
//
//     /// Restrict which verticals are returned ("web,news,videos,...")
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub result_filter: Option<String>,
//
//     /// Ask Brave to return extra text snippets
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub extra_snippets: Option<bool>,
//
//     /// Disable spellcheck corrections
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub spellcheck_off: Option<bool>,
//
//     /// Apply Goggles (custom ranking model)
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub goggles_id: Option<String>,
// }
//
// pub fn map_freshness(f: &Freshness) -> Option<&'static str> {
//     Some(match f {
//         Freshness::Any => return None,
//         Freshness::Day => "pd",
//         Freshness::Week => "pw",
//         Freshness::Month => "pm",
//         Freshness::Year => "py",
//     })
// }
//
// pub fn map_safe(s: SafeSearch) -> Option<&'static str> {
//     Some(match s {
//         SafeSearch::Off => "off",
//         SafeSearch::Moderate => "moderate",
//         SafeSearch::Strict => "strict",
//     })
// }
//
// pub fn map_verticals(v: Verticals) -> Option<String> {
//     let mut xs = Vec::new();
//     if v.web {
//         xs.push("web");
//     }
//     if v.news {
//         xs.push("news")
//     }
//     if v.videos {
//         xs.push("videos")
//     }
//     if xs.is_empty() {
//         None
//     } else {
//         Some(xs.join(","))
//     }
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct WebSearchApiResponse {
//     /// Always "search"
//     #[serde(rename = "type")]
//     pub r#type: String, // could also be an enum TypeSearch
//
//     #[serde(default)]
//     pub query: Option<Query>,
//
//     #[serde(default)]
//     pub mixed: Option<MixedResponse>,
//
//     #[serde(default)]
//     pub web: Option<Search>, // "Search" vertical (web results)
//     #[serde(default)]
//     pub news: Option<News>, // news vertical (simplified below)
//     #[serde(default)]
//     pub videos: Option<Videos>, // videos vertical (simplified below)
//
//     // Other sections you might see:
//     #[serde(default)]
//     pub summarizer: Option<SummarizerRef>,
//     #[serde(default)]
//     pub infobox: Option<GraphInfobox>,
//     #[serde(default)]
//     pub discussions: Option<Discussions>,
//     #[serde(default)]
//     pub faq: Option<FAQ>,
//     #[serde(default)]
//     pub locations: Option<Locations>,
//     #[serde(default)]
//     pub rich: Option<RichCallbackInfo>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Query {
//     pub original: String,
//
//     #[serde(default)]
//     pub altered: Option<String>,
//     #[serde(default)]
//     pub show_strict_warning: Option<bool>,
//     #[serde(default)]
//     pub is_navigational: Option<bool>,
//     #[serde(default)]
//     pub is_news_breaking: Option<bool>,
//     #[serde(default)]
//     pub spellcheck_off: Option<bool>,
//     #[serde(default)]
//     pub country: Option<String>,
//     #[serde(default)]
//     pub bad_results: Option<bool>,
//     #[serde(default)]
//     pub should_fallback: Option<bool>,
//     #[serde(default)]
//     pub lat: Option<String>,
//     #[serde(default)]
//     pub long: Option<String>,
//     #[serde(default)]
//     pub postal_code: Option<String>,
//     #[serde(default)]
//     pub city: Option<String>,
//     #[serde(default)]
//     pub header_country: Option<String>,
//     #[serde(default)]
//     pub more_results_available: Option<bool>,
//     #[serde(default)]
//     pub state: Option<String>,
//     // …plus any other optional fields you care about (language, local flags, etc.)
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MixedResponse {
//     #[serde(default)]
//     pub main: Vec<MixedEntry>,
//
//     #[serde(default)]
//     pub top: Vec<MixedEntry>,
//     #[serde(default)]
//     pub side: Vec<MixedEntry>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MixedEntry {
//     #[serde(rename = "type")]
//     pub kind: MixedKind, // "web" | "news" | "videos" etc.
//
//     #[serde(default)]
//     pub index: usize, // present when referencing a single item
//     #[serde(default)]
//     pub all: Option<bool>, // true => include entire vertical
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "lowercase")]
// pub enum MixedKind {
//     Web,
//     News,
//     Videos,
//     // Brave sometimes adds other kinds; keep Unknown to be forward-compatible
//     #[serde(other)]
//     Unknown,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MetaUrl {
//     pub scheme: String,
//     pub netloc: String,
//
//     #[serde(default)]
//     pub hostname: Option<String>,
//     pub favicon: String,
//     pub path: String,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Thumbnail {
//     pub src: String,
//     #[serde(default)]
//     pub original: Option<String>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Search {
//     /// Always "search"
//     #[serde(rename = "type")]
//     pub r#type: String,
//
//     pub results: Vec<SearchResult>,
//     pub family_friendly: bool,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct SearchResult {
//     /// Always "search_result"
//     #[serde(rename = "type")]
//     pub r#type: String,
//
//     pub subtype: String, // "generic" etc.
//     pub is_live: bool,
//
//     #[serde(default)]
//     pub meta_url: Option<MetaUrl>,
//     #[serde(default)]
//     pub thumbnail: Option<Thumbnail>,
//     #[serde(default)]
//     pub age: Option<String>,
//     #[serde(default)]
//     pub language: Option<String>,
//
//     // Rich extras (all optional)
//     #[serde(default)]
//     pub video: Option<VideoData>,
//     #[serde(default)]
//     pub movie: Option<MovieData>,
//     #[serde(default)]
//     pub article: Option<Article>,
//     #[serde(default)]
//     pub product: Option<ProductOrReview>,
//     #[serde(default)]
//     pub product_cluster: Option<Vec<ProductOrReview>>,
//     #[serde(default)]
//     pub creative_work: Option<CreativeWork>,
//     #[serde(default)]
//     pub organization: Option<Organization>,
//     #[serde(default)]
//     pub recipe: Option<Recipe>,
//     #[serde(default)]
//     pub rating: Option<Rating>,
//     #[serde(default)]
//     pub review: Option<Review>,
//     #[serde(default)]
//     pub music_recording: Option<MusicRecording>,
//     #[serde(default)]
//     pub faq: Option<FAQ>,
//     #[serde(default)]
//     pub qa: Option<QAPage>,
//     #[serde(default)]
//     pub cluster_type: Option<String>,
//     #[serde(default)]
//     pub cluster: Option<Vec<ResultItem>>,
//     #[serde(default)]
//     pub content_type: Option<String>,
//     #[serde(default)]
//     pub extra_snippets: Option<Vec<String>>,
//
//     // The “Result” subobject fields (title/url/etc.) frequently appear alongside:
//     #[serde(default)]
//     pub title: Option<String>,
//     #[serde(default)]
//     pub url: Option<String>,
//     #[serde(default)]
//     pub is_source_local: Option<bool>,
//     #[serde(default)]
//     pub is_source_both: Option<bool>,
//     #[serde(default)]
//     pub description: Option<String>,
//     #[serde(default)]
//     pub page_age: Option<String>,
//     #[serde(default, rename = "page_fetched")]
//     pub page_fetched: Option<String>,
//     #[serde(default)]
//     pub profile: Option<Profile>,
//     #[serde(default)]
//     pub family_friendly: Option<bool>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct ResultItem {
//     pub title: String,
//     pub url: String,
//
//     #[serde(default)]
//     pub is_source_local: Option<bool>,
//     #[serde(default)]
//     pub is_source_both: Option<bool>,
//     #[serde(default)]
//     pub description: Option<String>,
//     #[serde(default)]
//     pub page_age: Option<String>,
//     #[serde(default, rename = "page_fetched")]
//     pub page_fetched: Option<String>,
//     #[serde(default)]
//     pub profile: Option<Profile>,
//     #[serde(default)]
//     pub language: Option<String>,
//     pub family_friendly: bool,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Profile {
//     pub name: String,
//     pub url: String,
//
//     #[serde(default)]
//     pub long_name: Option<String>,
//     #[serde(default)]
//     pub img: Option<String>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct News {
//     /// Always "news"
//     #[serde(rename = "type")]
//     pub r#type: String,
//
//     pub results: Vec<NewsResult>,
//
//     #[serde(default)]
//     pub mutated_by_goggles: Option<bool>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct BraveHit {
//     pub rank: u32,
//     pub title: String,
//     pub url: String,
//     pub description: Option<String>,
// }
//
// pub struct BraveBatch {
//     pub session_id: Uuid,
//     query: String,
//     hits: Vec<BraveHit>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct NewsResult {
//     pub title: String,
//     pub url: String,
//
//     #[serde(default)]
//     pub description: Option<String>,
//     #[serde(default)]
//     pub age: Option<String>,
//     #[serde(default)]
//     pub page_age: Option<String>,
//     #[serde(default)]
//     pub fetched_content_timestamp: Option<i64>,
//
//     #[serde(default)]
//     pub profile: Option<Profile>,
//     #[serde(default)]
//     pub meta_url: Option<MetaUrl>,
//     #[serde(default)]
//     pub thumbnail: Option<Thumbnail>,
//
//     #[serde(default)]
//     pub is_source_local: Option<bool>,
//     #[serde(default)]
//     pub is_source_both: Option<bool>,
//     #[serde(default)]
//     pub breaking: Option<bool>,
//     #[serde(default)]
//     pub is_live: Option<bool>,
//
//     #[serde(default)]
//     pub family_friendly: Option<bool>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Videos {
//     /// Always "videos"
//     #[serde(rename = "type")]
//     pub r#type: String,
//
//     pub results: Vec<VideoResult>,
//
//     #[serde(default)]
//     pub mutated_by_goggles: Option<bool>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct VideoResult {
//     #[serde(rename = "type")]
//     pub r#type: String, // "video_result"
//     pub url: String,
//     pub title: String,
//
//     #[serde(default)]
//     pub description: Option<String>,
//     #[serde(default)]
//     pub age: Option<String>,
//     #[serde(default)]
//     pub page_age: Option<String>,
//
//     pub video: VideoData,
//
//     #[serde(default)]
//     pub meta_url: Option<MetaUrl>,
//     #[serde(default)]
//     pub thumbnail: Option<Thumbnail>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct VideoData {
//     #[serde(default)]
//     pub duration: Option<String>,
//     #[serde(default)]
//     pub creator: Option<String>,
//     #[serde(default)]
//     pub publisher: Option<String>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct SummarizerRef {
//     // Often this is a key/token you can use to fetch the summary separately.
//     #[serde(default)]
//     pub key: Option<String>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct GraphInfobox {
//     #[serde(rename = "type")]
//     pub r#type: String, // "graph"
//
//     pub results: Vec<GraphInfoboxVariant>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(tag = "subtype")]
// pub enum GraphInfoboxVariant {
//     #[serde(rename = "generic")]
//     Generic(GenericInfobox),
//     #[serde(rename = "entity")]
//     Entity(EntityInfobox),
//     #[serde(rename = "place")]
//     Place(InfoboxPlace),
//     #[serde(rename = "location")]
//     WithLocation(InfoboxWithLocation),
//     #[serde(rename = "code")]
//     QA(QAInfoBox),
//     // add others as needed
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct GenericInfobox {
//     #[serde(default)]
//     pub found_in_urls: Option<Vec<String>>,
//     // …
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct EntityInfobox {
//     // …
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct InfoboxPlace {
//     pub location: LocationResult,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct InfoboxWithLocation {
//     pub is_location: bool,
//     #[serde(default)]
//     pub coordinates: Option<Vec<f64>>,
//     pub zoom_level: i32,
//     #[serde(default)]
//     pub location: Option<LocationResult>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Discussions {
//     #[serde(rename = "type")]
//     pub r#type: String, // "search"
//     pub results: Vec<DiscussionResult>,
//     pub mutated_by_goggles: bool,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct DiscussionResult {
//     #[serde(rename = "type")]
//     pub r#type: String, // "discussion"
//     #[serde(default)]
//     pub data: Option<ForumData>,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct ForumData {
//     pub forum_name: String,
//     #[serde(default)]
//     pub num_answers: Option<i32>,
//     #[serde(default)]
//     pub score: Option<String>,
//     #[serde(default)]
//     pub title: Option<String>,
//     #[serde(default)]
//     pub question: Option<String>,
//     #[serde(default)]
//     pub top_comment: Option<String>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Locations {
//     #[serde(rename = "type")]
//     pub r#type: String, // "locations" (name varies in docs)
//                         // …
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct LocationResult {
//     #[serde(rename = "type")]
//     pub r#type: String, // "location_result"
//     #[serde(default)]
//     pub id: Option<String>,
//     pub provider_url: String,
//     #[serde(default)]
//     pub coordinates: Option<Vec<f64>>,
//     pub zoom_level: i32,
//     #[serde(default)]
//     pub thumbnail: Option<Thumbnail>,
//     #[serde(default)]
//     pub postal_address: Option<PostalAddress>,
//     #[serde(default)]
//     pub opening_hours: Option<OpeningHours>,
//     #[serde(default)]
//     pub contact: Option<Contact>,
// }
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct PostalAddress {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct OpeningHours {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Contact {/* … */}
//
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct RichCallbackInfo {/* … */}
//
// // Stubs for rich types referenced by SearchResult:
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct QAPage {
//     pub question: String,
//     pub answer: Answer,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Answer {
//     pub text: String,
//     #[serde(default)]
//     pub author: Option<String>,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct FAQ {
//     #[serde(rename = "type")]
//     pub r#type: String,
//     pub results: Vec<QA>,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct QA {
//     pub question: String,
//     pub answer: String,
//     pub title: String,
//     pub url: String,
//     #[serde(default)]
//     pub meta_url: Option<MetaUrl>,
// }
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Article {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct ProductOrReview {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct CreativeWork {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Organization {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Recipe {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Rating {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Review {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MusicRecording {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MovieData {/* … */}
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct QAInfoBox {/* … */}
