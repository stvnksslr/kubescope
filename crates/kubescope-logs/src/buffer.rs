use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use kubescope_types::{LogEntry, LogLevel};

/// Thread-safe ring buffer for log entries
#[derive(Clone)]
pub struct LogBuffer {
    /// Internal storage
    entries: Arc<RwLock<VecDeque<LogEntry>>>,

    /// Maximum capacity
    capacity: usize,

    /// Next entry ID
    next_id: Arc<AtomicU64>,
}

impl LogBuffer {
    /// Create a new log buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            capacity,
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Push a new entry, evicting oldest if at capacity
    pub fn push(&self, mut entry: LogEntry) {
        entry.id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut entries = self.entries.write();
        if entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Get all entries (cloned for rendering)
    pub fn all(&self) -> Vec<LogEntry> {
        self.entries.read().iter().cloned().collect()
    }

    /// Get entries filtered by a predicate
    pub fn filtered<F>(&self, predicate: F) -> Vec<LogEntry>
    where
        F: Fn(&LogEntry) -> bool,
    {
        self.entries
            .read()
            .iter()
            .filter(|e| predicate(e))
            .cloned()
            .collect()
    }

    /// Get entries filtered by log level (minimum level)
    pub fn by_level(&self, min_level: LogLevel) -> Vec<LogEntry> {
        let min_ord = level_ordinal(min_level);
        self.filtered(|e| level_ordinal(e.level) >= min_ord)
    }

    /// Get entry count per log level
    pub fn level_counts(&self) -> LevelCounts {
        let entries = self.entries.read();
        let mut counts = LevelCounts::default();

        for entry in entries.iter() {
            match entry.level {
                LogLevel::Trace => counts.trace += 1,
                LogLevel::Debug => counts.debug += 1,
                LogLevel::Info => counts.info += 1,
                LogLevel::Warn => counts.warn += 1,
                LogLevel::Error => counts.error += 1,
                LogLevel::Fatal => counts.fatal += 1,
                LogLevel::Unknown => counts.unknown += 1,
            }
        }

        counts
    }

    /// Total entry count
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Export all entries as raw lines
    pub fn export_raw(&self) -> String {
        self.entries
            .read()
            .iter()
            .map(|e| e.raw.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.write().clear();
        self.next_id.store(0, Ordering::SeqCst);
    }

    /// Get the last N entries
    pub fn tail(&self, n: usize) -> Vec<LogEntry> {
        let entries = self.entries.read();
        let start = entries.len().saturating_sub(n);
        entries.iter().skip(start).cloned().collect()
    }

    /// Get entries in a range (for virtual scrolling)
    pub fn range(&self, start: usize, count: usize) -> Vec<LogEntry> {
        let entries = self.entries.read();
        entries.iter().skip(start).take(count).cloned().collect()
    }
}

/// Counts per log level
#[derive(Clone, Debug, Default)]
pub struct LevelCounts {
    pub trace: usize,
    pub debug: usize,
    pub info: usize,
    pub warn: usize,
    pub error: usize,
    pub fatal: usize,
    pub unknown: usize,
}

impl LevelCounts {
    pub fn total(&self) -> usize {
        self.trace + self.debug + self.info + self.warn + self.error + self.fatal + self.unknown
    }
}

/// Get ordinal for log level comparison
fn level_ordinal(level: LogLevel) -> u8 {
    match level {
        LogLevel::Trace => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
        LogLevel::Fatal => 5,
        LogLevel::Unknown => 2, // Treat unknown as info level
    }
}
