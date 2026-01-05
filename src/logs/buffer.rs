use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{Mutex, RwLock};

use crate::types::{ArcLogEntry, LogEntry, LogLevel};

/// Lock-free atomic counters for each log level
#[derive(Default)]
struct AtomicLevelCounts {
    trace: AtomicUsize,
    debug: AtomicUsize,
    info: AtomicUsize,
    warn: AtomicUsize,
    error: AtomicUsize,
    fatal: AtomicUsize,
    unknown: AtomicUsize,
}

impl AtomicLevelCounts {
    fn increment(&self, level: LogLevel) {
        match level {
            LogLevel::Trace => self.trace.fetch_add(1, Ordering::Relaxed),
            LogLevel::Debug => self.debug.fetch_add(1, Ordering::Relaxed),
            LogLevel::Info => self.info.fetch_add(1, Ordering::Relaxed),
            LogLevel::Warn => self.warn.fetch_add(1, Ordering::Relaxed),
            LogLevel::Error => self.error.fetch_add(1, Ordering::Relaxed),
            LogLevel::Fatal => self.fatal.fetch_add(1, Ordering::Relaxed),
            LogLevel::Unknown => self.unknown.fetch_add(1, Ordering::Relaxed),
        };
    }

    fn decrement(&self, level: LogLevel) {
        match level {
            LogLevel::Trace => self.trace.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Debug => self.debug.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Info => self.info.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Warn => self.warn.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Error => self.error.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Fatal => self.fatal.fetch_sub(1, Ordering::Relaxed),
            LogLevel::Unknown => self.unknown.fetch_sub(1, Ordering::Relaxed),
        };
    }

    fn to_counts(&self) -> LevelCounts {
        LevelCounts {
            trace: self.trace.load(Ordering::Relaxed),
            debug: self.debug.load(Ordering::Relaxed),
            info: self.info.load(Ordering::Relaxed),
            warn: self.warn.load(Ordering::Relaxed),
            error: self.error.load(Ordering::Relaxed),
            fatal: self.fatal.load(Ordering::Relaxed),
            unknown: self.unknown.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.trace.store(0, Ordering::Relaxed);
        self.debug.store(0, Ordering::Relaxed);
        self.info.store(0, Ordering::Relaxed);
        self.warn.store(0, Ordering::Relaxed);
        self.error.store(0, Ordering::Relaxed);
        self.fatal.store(0, Ordering::Relaxed);
        self.unknown.store(0, Ordering::Relaxed);
    }
}

/// Thread-safe ring buffer for log entries
#[derive(Clone)]
#[allow(dead_code)]
pub struct LogBuffer {
    /// Internal storage - uses Arc<LogEntry> to avoid expensive clones during rendering
    entries: Arc<RwLock<VecDeque<ArcLogEntry>>>,

    /// Maximum capacity
    capacity: usize,

    /// Next entry ID
    next_id: Arc<AtomicUsize>,

    /// Fast atomic counter for total entries (avoids locking on len())
    total_count: Arc<AtomicUsize>,

    /// Lock-free level counts (O(1) instead of O(n) scan)
    level_counts: Arc<AtomicLevelCounts>,

    /// Staging buffer for batch writes (reduces lock contention)
    pending: Arc<Mutex<Vec<LogEntry>>>,

    /// Incrementally maintained set of JSON keys from logs
    json_keys: Arc<RwLock<BTreeSet<String>>>,
}

/// Batch size for flushing pending entries
const BATCH_FLUSH_SIZE: usize = 100;

#[allow(dead_code)]
impl LogBuffer {
    /// Create a new log buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            capacity,
            next_id: Arc::new(AtomicUsize::new(0)),
            total_count: Arc::new(AtomicUsize::new(0)),
            level_counts: Arc::new(AtomicLevelCounts::default()),
            pending: Arc::new(Mutex::new(Vec::with_capacity(BATCH_FLUSH_SIZE))),
            json_keys: Arc::new(RwLock::new(BTreeSet::new())),
        }
    }

    /// Push a new entry to the staging buffer
    /// Entries are flushed to the main buffer when batch size is reached
    pub fn push(&self, entry: LogEntry) {
        // Increment atomic counter first (lock-free, used by len())
        self.total_count.fetch_add(1, Ordering::Relaxed);

        // Track JSON keys incrementally (fast path - only new keys need write lock)
        if let Some(fields) = &entry.fields {
            let keys_read = self.json_keys.read();
            let new_keys: Vec<_> = fields
                .keys()
                .filter(|k| !keys_read.contains(*k))
                .cloned()
                .collect();
            drop(keys_read);

            if !new_keys.is_empty() {
                let mut keys_write = self.json_keys.write();
                for key in new_keys {
                    keys_write.insert(key);
                }
            }
        }

        // Add to staging buffer
        let mut pending = self.pending.lock();
        pending.push(entry);

        // Flush when batch size reached
        if pending.len() >= BATCH_FLUSH_SIZE {
            self.flush_pending_locked(&mut pending);
        }
    }

    /// Flush pending entries to main buffer (internal, caller holds pending lock)
    fn flush_pending_locked(&self, pending: &mut Vec<LogEntry>) {
        if pending.is_empty() {
            return;
        }

        let mut entries = self.entries.write();
        for mut entry in pending.drain(..) {
            entry.id = self.next_id.fetch_add(1, Ordering::Relaxed) as u64;
            // Increment level count for new entry
            self.level_counts.increment(entry.level);
            if entries.len() >= self.capacity {
                // Decrement level count for evicted entry
                if let Some(evicted) = entries.pop_front() {
                    self.level_counts.decrement(evicted.level);
                    self.total_count.fetch_sub(1, Ordering::Relaxed);
                }
            }
            entries.push_back(Arc::new(entry));
        }
    }

    /// Force flush any pending entries (call before reading)
    pub fn flush(&self) {
        let mut pending = self.pending.lock();
        self.flush_pending_locked(&mut pending);
    }

    /// Get all known JSON keys (incrementally collected)
    pub fn json_keys(&self) -> Vec<String> {
        self.json_keys.read().iter().cloned().collect()
    }

    /// Get all entries (Arc clones are cheap - just reference count increment)
    /// Flushes pending entries first to ensure consistency
    pub fn all(&self) -> Vec<ArcLogEntry> {
        self.flush();
        self.entries.read().iter().cloned().collect()
    }

    /// Get entries filtered by a predicate
    pub fn filtered<F>(&self, predicate: F) -> Vec<ArcLogEntry>
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
    pub fn by_level(&self, min_level: LogLevel) -> Vec<ArcLogEntry> {
        let min_ord = level_ordinal(min_level);
        self.filtered(|e| level_ordinal(e.level) >= min_ord)
    }

    /// Get entry count per log level (O(1) lock-free via atomic counters)
    pub fn level_counts(&self) -> LevelCounts {
        // Flush pending entries first so counts are accurate
        self.flush();
        self.level_counts.to_counts()
    }

    /// Total entry count (lock-free via atomic counter)
    pub fn len(&self) -> usize {
        self.total_count.load(Ordering::Relaxed)
    }

    /// Check if buffer is empty (lock-free)
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Export all entries as raw lines
    pub fn export_raw(&self) -> String {
        self.flush();
        self.entries
            .read()
            .iter()
            .map(|e| e.raw.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.pending.lock().clear();
        self.entries.write().clear();
        self.json_keys.write().clear();
        self.next_id.store(0, Ordering::SeqCst);
        self.total_count.store(0, Ordering::SeqCst);
        self.level_counts.reset();
    }

    /// Get the last N entries
    pub fn tail(&self, n: usize) -> Vec<ArcLogEntry> {
        let entries = self.entries.read();
        let start = entries.len().saturating_sub(n);
        entries.iter().skip(start).cloned().collect()
    }

    /// Get entries in a range (for virtual scrolling)
    pub fn range(&self, start: usize, count: usize) -> Vec<ArcLogEntry> {
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
#[allow(dead_code)]
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
