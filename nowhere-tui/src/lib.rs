//! Terminal UI orchestration for claims, chat, and evidence review.
//!
//! The submodules expose command parsing, feed loops, and view rendering; they still
//! require higher-level docs explaining how messages propagate between the TUI and
//! actor runtime.
mod command;
mod feeders;
mod styles;
mod transcript;
mod tui;
mod view;

pub use feeders::spawn_tui_feeders;
pub use tui::{TuiActor, TuiMsg};
