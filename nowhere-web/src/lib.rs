//! Web discovery and acquisition utilities.
//!
//! - Brave Search API client (`brave`) for discovery
//! - Browser capture trait and Fantoccini-backed implementation (`browser`)
//! - Lightweight HTML extraction (`extract`)
//!
//! Note: content extraction is intentionally minimal in v0.1; see FIXMEs in
//! `extract` for a suggested parser upgrade path.

pub mod brave;
pub mod browser;
pub mod extract;
