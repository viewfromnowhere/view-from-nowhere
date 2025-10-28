//! Twitter/X API integration surface exposed to the actor system.
//!
//! Submodules provide the HTTP client wrapper, JSON extraction helpers, and strongly
//! typed response models. Additional docs should spell out rate-limit expectations and
//! how pagination tokens flow back to callers.
pub mod client;
pub mod extract;
pub mod types;

// (optional) re-exports if you want `nowhere_social::twitter::TwitterApi` etc.
pub use client::TwitterApi;
