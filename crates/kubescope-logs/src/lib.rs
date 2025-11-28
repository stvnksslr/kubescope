//! Log processing for kubescope
//!
//! This crate provides log buffering, parsing, filtering, and streaming.

mod buffer;
mod filter;
mod parser;
mod stream;

pub use buffer::{LevelCounts, LogBuffer};
pub use filter::{CompiledFilter, FilterPresets};
pub use parser::LogParser;
pub use stream::LogStreamManager;

// Re-export types used in our public API
pub use kubescope_types::{LogEntry, LogLevel};
