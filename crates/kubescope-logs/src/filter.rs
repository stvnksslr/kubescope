use regex::Regex;
use std::collections::HashSet;

use kubescope_types::{LogEntry, LogLevel};

/// Compiled filter for log entries
#[derive(Clone)]
pub struct CompiledFilter {
    /// Regex pattern (if any)
    regex: Option<Regex>,

    /// Original pattern string
    pattern: String,

    /// Log levels to include (empty = all)
    levels: HashSet<LogLevel>,

    /// Pods to include (empty = all)
    pods: HashSet<String>,

    /// Whether to invert match
    invert: bool,

    /// Case sensitivity
    case_insensitive: bool,
}

impl CompiledFilter {
    /// Create a new filter from a pattern string
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        let regex = if pattern.is_empty() {
            None
        } else {
            Some(Regex::new(pattern)?)
        };

        Ok(Self {
            regex,
            pattern: pattern.to_string(),
            levels: HashSet::new(),
            pods: HashSet::new(),
            invert: false,
            case_insensitive: false,
        })
    }

    /// Create a case-insensitive filter
    pub fn new_case_insensitive(pattern: &str) -> Result<Self, regex::Error> {
        let regex = if pattern.is_empty() {
            None
        } else {
            // Prepend (?i) for case insensitive matching
            Some(Regex::new(&format!("(?i){}", pattern))?)
        };

        Ok(Self {
            regex,
            pattern: pattern.to_string(),
            levels: HashSet::new(),
            pods: HashSet::new(),
            invert: false,
            case_insensitive: true,
        })
    }

    /// Set log levels to filter by
    pub fn with_levels(mut self, levels: HashSet<LogLevel>) -> Self {
        self.levels = levels;
        self
    }

    /// Set pods to filter by
    pub fn with_pods(mut self, pods: HashSet<String>) -> Self {
        self.pods = pods;
        self
    }

    /// Invert the match
    pub fn inverted(mut self) -> Self {
        self.invert = true;
        self
    }

    /// Check if a log entry matches this filter
    pub fn matches(&self, entry: &LogEntry) -> bool {
        // Check log level filter
        if !self.levels.is_empty() && !self.levels.contains(&entry.level) {
            return self.invert;
        }

        // Check pod filter
        if !self.pods.is_empty() && !self.pods.contains(&entry.pod_name) {
            return self.invert;
        }

        // Check regex pattern
        let text_match = match &self.regex {
            Some(re) => re.is_match(&entry.raw),
            None => true,
        };

        if self.invert { !text_match } else { text_match }
    }

    /// Find all match positions in a string (for highlighting)
    pub fn find_matches(&self, text: &str) -> Vec<(usize, usize)> {
        match &self.regex {
            Some(re) => re.find_iter(text).map(|m| (m.start(), m.end())).collect(),
            None => Vec::new(),
        }
    }

    /// Get the original pattern
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Check if filter is empty (matches everything)
    pub fn is_empty(&self) -> bool {
        self.regex.is_none() && self.levels.is_empty() && self.pods.is_empty()
    }

    /// Check if filter has a text pattern
    pub fn has_pattern(&self) -> bool {
        self.regex.is_some()
    }

    /// Check if filter is case insensitive
    pub fn is_case_insensitive(&self) -> bool {
        self.case_insensitive
    }
}

impl std::fmt::Debug for CompiledFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledFilter")
            .field("pattern", &self.pattern)
            .field("levels", &self.levels)
            .field("pods", &self.pods)
            .field("invert", &self.invert)
            .finish()
    }
}

/// Quick filter presets
pub struct FilterPresets;

impl FilterPresets {
    /// Filter for errors only
    pub fn errors_only() -> CompiledFilter {
        let mut levels = HashSet::new();
        levels.insert(LogLevel::Error);
        levels.insert(LogLevel::Fatal);
        CompiledFilter::new("").unwrap().with_levels(levels)
    }

    /// Filter for warnings and above
    pub fn warnings_and_above() -> CompiledFilter {
        let mut levels = HashSet::new();
        levels.insert(LogLevel::Warn);
        levels.insert(LogLevel::Error);
        levels.insert(LogLevel::Fatal);
        CompiledFilter::new("").unwrap().with_levels(levels)
    }

    /// Filter for info and above (no debug/trace)
    pub fn info_and_above() -> CompiledFilter {
        let mut levels = HashSet::new();
        levels.insert(LogLevel::Info);
        levels.insert(LogLevel::Warn);
        levels.insert(LogLevel::Error);
        levels.insert(LogLevel::Fatal);
        CompiledFilter::new("").unwrap().with_levels(levels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_filter() {
        let filter = CompiledFilter::new("error").unwrap();
        let mut entry = LogEntry::new("pod".to_string(), 1, "an error occurred".to_string());
        assert!(filter.matches(&entry));

        entry.raw = "everything is fine".to_string();
        assert!(!filter.matches(&entry));
    }

    #[test]
    fn test_level_filter() {
        let filter = FilterPresets::errors_only();
        let mut entry = LogEntry::new("pod".to_string(), 1, "test".to_string());

        entry.level = LogLevel::Error;
        assert!(filter.matches(&entry));

        entry.level = LogLevel::Info;
        assert!(!filter.matches(&entry));
    }

    #[test]
    fn test_find_matches() {
        let filter = CompiledFilter::new("error").unwrap();
        let matches = filter.find_matches("an error occurred, another error here");
        assert_eq!(matches.len(), 2);
    }
}
