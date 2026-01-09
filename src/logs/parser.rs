use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

use crate::types::{LogEntry, LogLevel};

/// Log parser for extracting structure from raw log lines
pub struct LogParser;

impl LogParser {
    /// Parse a raw log line into a LogEntry
    pub fn parse(raw: &str, pod_name: &str, line_number: u64) -> LogEntry {
        let mut entry = LogEntry::new(pod_name.to_string(), line_number, raw.to_string());

        // Try to extract Kubernetes timestamp prefix (format: 2024-01-15T10:30:00.123456789Z)
        let (timestamp, content) = Self::extract_k8s_timestamp(raw);
        entry.timestamp = timestamp;

        // Try to parse as JSON
        if let Some((fields, level, pretty)) = Self::try_parse_json(content) {
            entry.is_json = true;
            entry.fields = Some(fields);
            entry.level = level;
            entry.pretty_printed = Some(pretty);
        } else {
            // If not JSON, try to extract level from plain text
            entry.level = Self::extract_level_from_text(content);
        }

        entry
    }

    /// Extract Kubernetes timestamp from the beginning of a log line
    fn extract_k8s_timestamp(raw: &str) -> (Option<DateTime<Utc>>, &str) {
        // K8s timestamp format: 2024-01-15T10:30:00.123456789Z (30 chars)
        // Sometimes shorter: 2024-01-15T10:30:00Z (20 chars)
        if raw.len() >= 20 {
            // Find the 'Z' that ends the timestamp within first ~35 chars
            // Use get() to safely handle UTF-8 multi-byte chars at boundaries
            let search_end = Self::floor_char_boundary(raw, 35.min(raw.len()));
            if let Some(z_pos) = raw.get(..search_end).and_then(|s| s.find('Z')) {
                let ts_str = &raw[..=z_pos];
                if let Ok(ts) = DateTime::parse_from_rfc3339(ts_str) {
                    let remaining = raw[z_pos + 1..].trim_start();
                    return (Some(ts.with_timezone(&Utc)), remaining);
                }
            }
        }
        (None, raw)
    }

    /// Find the largest valid char boundary <= the given byte index
    fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
        if idx >= s.len() {
            return s.len();
        }
        // Walk backwards to find a valid char boundary
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    /// Try to parse content as JSON and extract fields
    fn try_parse_json(content: &str) -> Option<(HashMap<String, Value>, LogLevel, String)> {
        // Find JSON object in content
        let trimmed = content.trim();
        if !trimmed.starts_with('{') {
            return None;
        }

        let value: Value = serde_json::from_str(trimmed).ok()?;
        let obj = value.as_object()?;

        let fields: HashMap<String, Value> = obj.clone().into_iter().collect();

        // Extract level from common field names
        let level = Self::extract_level_from_json(&fields);

        // Pretty print
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_default();

        Some((fields, level, pretty))
    }

    /// Extract log level from JSON fields
    fn extract_level_from_json(fields: &HashMap<String, Value>) -> LogLevel {
        // Common field names for log level
        let level_fields = [
            "level",
            "lvl",
            "severity",
            "log.level",
            "loglevel",
            "log_level",
            "Level",
            "LEVEL",
        ];

        for field in level_fields {
            if let Some(value) = fields.get(field) {
                match value {
                    Value::String(s) => return LogLevel::from_str(s),
                    Value::Number(n) => {
                        // Some loggers use numeric levels
                        if let Some(num) = n.as_u64() {
                            return match num {
                                0..=10 => LogLevel::Trace,
                                11..=20 => LogLevel::Debug,
                                21..=30 => LogLevel::Info,
                                31..=40 => LogLevel::Warn,
                                41..=50 => LogLevel::Error,
                                _ => LogLevel::Fatal,
                            };
                        }
                    }
                    _ => {}
                }
            }
        }

        LogLevel::Unknown
    }

    /// Extract log level from plain text patterns
    fn extract_level_from_text(content: &str) -> LogLevel {
        let upper = content.to_uppercase();

        // Check for bracketed patterns first [ERROR], [WARN], etc.
        let bracket_patterns = [
            ("[FATAL]", LogLevel::Fatal),
            ("[PANIC]", LogLevel::Fatal),
            ("[CRITICAL]", LogLevel::Fatal),
            ("[ERROR]", LogLevel::Error),
            ("[ERR]", LogLevel::Error),
            ("[WARN]", LogLevel::Warn),
            ("[WARNING]", LogLevel::Warn),
            ("[INFO]", LogLevel::Info),
            ("[DEBUG]", LogLevel::Debug),
            ("[TRACE]", LogLevel::Trace),
        ];

        for (pattern, level) in bracket_patterns {
            if upper.contains(pattern) {
                return level;
            }
        }

        // Check for colon patterns: ERROR:, WARN:, etc.
        let colon_patterns = [
            ("FATAL:", LogLevel::Fatal),
            ("PANIC:", LogLevel::Fatal),
            ("ERROR:", LogLevel::Error),
            ("ERR:", LogLevel::Error),
            ("WARNING:", LogLevel::Warn),
            ("WARN:", LogLevel::Warn),
            ("INFO:", LogLevel::Info),
            ("DEBUG:", LogLevel::Debug),
            ("TRACE:", LogLevel::Trace),
        ];

        for (pattern, level) in colon_patterns {
            if upper.contains(pattern) {
                return level;
            }
        }

        // Check for spaced patterns: " ERROR ", " WARN ", etc.
        let spaced_patterns = [
            (" FATAL ", LogLevel::Fatal),
            (" PANIC ", LogLevel::Fatal),
            (" ERROR ", LogLevel::Error),
            (" WARN ", LogLevel::Warn),
            (" WARNING ", LogLevel::Warn),
            (" INFO ", LogLevel::Info),
            (" DEBUG ", LogLevel::Debug),
            (" TRACE ", LogLevel::Trace),
        ];

        for (pattern, level) in spaced_patterns {
            if upper.contains(pattern) {
                return level;
            }
        }

        // Check for level at start of line
        let start_patterns = [
            ("FATAL", LogLevel::Fatal),
            ("PANIC", LogLevel::Fatal),
            ("ERROR", LogLevel::Error),
            ("ERR", LogLevel::Error),
            ("WARN", LogLevel::Warn),
            ("INFO", LogLevel::Info),
            ("DEBUG", LogLevel::Debug),
            ("TRACE", LogLevel::Trace),
        ];

        let trimmed_upper = upper.trim_start();
        for (pattern, level) in start_patterns {
            if trimmed_upper.starts_with(pattern) {
                return level;
            }
        }

        LogLevel::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_k8s_timestamp() {
        let line = "2024-01-15T10:30:00.123456789Z some log message";
        let entry = LogParser::parse(line, "test-pod", 1);
        assert!(entry.timestamp.is_some());
    }

    #[test]
    fn test_parse_json_log() {
        let line = r#"{"level":"error","msg":"something failed","time":"2024-01-15"}"#;
        let entry = LogParser::parse(line, "test-pod", 1);
        assert!(entry.is_json);
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn test_parse_text_level() {
        let line = "[ERROR] something went wrong";
        let entry = LogParser::parse(line, "test-pod", 1);
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn test_parse_multibyte_utf8_no_panic() {
        // Box-drawing characters are 3 bytes each, this tests UTF-8 boundary handling
        let line = "─────────────────────────────────────────";
        let entry = LogParser::parse(line, "test-pod", 1);
        // Should not panic, timestamp should be None since it's not a valid timestamp
        assert!(entry.timestamp.is_none());

        // Test with timestamp-like content mixed with multibyte chars
        let line2 = "2024-01-15T10:30:00Z ╭────────────────────────────╮";
        let entry2 = LogParser::parse(line2, "test-pod", 2);
        assert!(entry2.timestamp.is_some());
    }
}
