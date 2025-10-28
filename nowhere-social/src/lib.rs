//! Social network clients and extractors used by Nowhere.
//!
//! Currently only the Twitter/X pipeline is implemented, and its submodules still need
//! thorough docs covering rate limits, pagination strategy, and how responses flow into
//! the actor system.
pub mod twitter;
