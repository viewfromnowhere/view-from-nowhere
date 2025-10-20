//! Driver layer for browser automation and related capabilities.
//!
//! This crate exposes the browser driver and page/element helpers used by
//! agents to collect content in a stealthy, reliable way.
//!
//! - [`nowhere_browser::driver::NowhereDriver`]: WebDriver client wrapper
//! - [`nowhere_browser::page::NowherePage`]: DOM helpers and LLM‑assisted selectors
//! - [`nowhere_browser::behavioral::BehavioralEngine`]: human‑like timings and typing
//! - [`nowhere_browser::stealth`]: stealth profiles and JS evasions
pub mod nowhere_browser;
