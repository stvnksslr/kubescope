//! Shared types for kubescope
//!
//! This crate contains data structures used across multiple kubescope crates.

use chrono::{DateTime, Utc};
use ratatui::style::Color;
use std::collections::HashMap;

// ============================================================================
// Kubernetes Resource Types
// ============================================================================

/// Kubernetes context information
#[derive(Clone, Debug)]
pub struct ContextInfo {
    pub name: String,
    pub cluster: String,
    pub user: String,
    pub namespace: Option<String>,
    pub is_current: bool,
}

impl ContextInfo {
    pub fn new(
        name: String,
        cluster: String,
        user: String,
        namespace: Option<String>,
        is_current: bool,
    ) -> Self {
        Self {
            name,
            cluster,
            user,
            namespace,
            is_current,
        }
    }
}

/// Namespace information
#[derive(Clone, Debug)]
pub struct NamespaceInfo {
    pub name: String,
    pub status: String,
    pub labels: HashMap<String, String>,
}

impl NamespaceInfo {
    pub fn new(name: String, status: String) -> Self {
        Self {
            name,
            status,
            labels: HashMap::new(),
        }
    }
}

/// Deployment information
#[derive(Clone, Debug)]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub replicas: i32,
    pub available_replicas: i32,
    pub ready_replicas: i32,
    pub labels: HashMap<String, String>,
    pub selector: HashMap<String, String>,
}

impl DeploymentInfo {
    pub fn new(name: String, namespace: String) -> Self {
        Self {
            name,
            namespace,
            replicas: 0,
            available_replicas: 0,
            ready_replicas: 0,
            labels: HashMap::new(),
            selector: HashMap::new(),
        }
    }

    /// Format replica status as "ready/total"
    pub fn replica_status(&self) -> String {
        format!("{}/{}", self.ready_replicas, self.replicas)
    }
}

/// Pod information
#[derive(Clone, Debug)]
pub struct PodInfo {
    pub name: String,
    pub namespace: String,
    pub status: PodStatus,
    pub containers: Vec<ContainerInfo>,
    pub node_name: Option<String>,
    pub pod_ip: Option<String>,
}

impl PodInfo {
    pub fn new(name: String, namespace: String) -> Self {
        Self {
            name,
            namespace,
            status: PodStatus::Unknown,
            containers: Vec::new(),
            node_name: None,
            pod_ip: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PodStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl From<&str> for PodStatus {
    fn from(s: &str) -> Self {
        match s {
            "Pending" => Self::Pending,
            "Running" => Self::Running,
            "Succeeded" => Self::Succeeded,
            "Failed" => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContainerInfo {
    pub name: String,
    pub ready: bool,
    pub restart_count: i32,
}

impl ContainerInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ready: false,
            restart_count: 0,
        }
    }
}

// ============================================================================
// Log Types
// ============================================================================

/// Time range for log filtering
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TimeRange {
    /// Show all available logs (uses tail_lines)
    #[default]
    All,
    /// Last 5 minutes
    Last5m,
    /// Last 15 minutes
    Last15m,
    /// Last 30 minutes
    Last30m,
    /// Last 1 hour
    Last1h,
    /// Last 6 hours
    Last6h,
    /// Last 24 hours
    Last24h,
}

impl TimeRange {
    /// Get the number of seconds for this time range
    pub fn as_seconds(&self) -> Option<i64> {
        match self {
            Self::All => None,
            Self::Last5m => Some(5 * 60),
            Self::Last15m => Some(15 * 60),
            Self::Last30m => Some(30 * 60),
            Self::Last1h => Some(60 * 60),
            Self::Last6h => Some(6 * 60 * 60),
            Self::Last24h => Some(24 * 60 * 60),
        }
    }

    /// Get display label for this time range
    pub fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Last5m => "5m",
            Self::Last15m => "15m",
            Self::Last30m => "30m",
            Self::Last1h => "1h",
            Self::Last6h => "6h",
            Self::Last24h => "24h",
        }
    }

    /// Cycle to the next time range
    pub fn next(&self) -> Self {
        match self {
            Self::All => Self::Last5m,
            Self::Last5m => Self::Last15m,
            Self::Last15m => Self::Last30m,
            Self::Last30m => Self::Last1h,
            Self::Last1h => Self::Last6h,
            Self::Last6h => Self::Last24h,
            Self::Last24h => Self::All,
        }
    }

    /// Cycle to the previous time range
    pub fn prev(&self) -> Self {
        match self {
            Self::All => Self::Last24h,
            Self::Last5m => Self::All,
            Self::Last15m => Self::Last5m,
            Self::Last30m => Self::Last15m,
            Self::Last1h => Self::Last30m,
            Self::Last6h => Self::Last1h,
            Self::Last24h => Self::Last6h,
        }
    }
}

/// Log severity level
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl LogLevel {
    /// Parse log level from common formats
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "trace" | "trc" | "trce" => Self::Trace,
            "debug" | "dbg" | "debg" => Self::Debug,
            "info" | "inf" | "information" => Self::Info,
            "warn" | "warning" | "wrn" => Self::Warn,
            "error" | "err" | "erro" => Self::Error,
            "fatal" | "panic" | "critical" | "crit" | "ftl" => Self::Fatal,
            _ => Self::Unknown,
        }
    }

    /// Get display color for this level
    pub fn color(&self) -> Color {
        match self {
            Self::Trace => Color::DarkGray,
            Self::Debug => Color::Cyan,
            Self::Info => Color::Green,
            Self::Warn => Color::Yellow,
            Self::Error => Color::Red,
            Self::Fatal => Color::Magenta,
            Self::Unknown => Color::White,
        }
    }

    /// Short display string (3 chars)
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "TRC",
            Self::Debug => "DBG",
            Self::Info => "INF",
            Self::Warn => "WRN",
            Self::Error => "ERR",
            Self::Fatal => "FTL",
            Self::Unknown => "???",
        }
    }
}

/// A single log entry
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Unique sequential ID
    pub id: u64,

    /// Line number within the pod's log stream
    pub line_number: u64,

    /// Source pod name
    pub pod_name: String,

    /// Container name (if multiple containers)
    pub container_name: Option<String>,

    /// Original raw log line
    pub raw: String,

    /// Parsed timestamp (if available)
    pub timestamp: Option<DateTime<Utc>>,

    /// Detected log level
    pub level: LogLevel,

    /// Parsed structured fields (if JSON)
    pub fields: Option<HashMap<String, serde_json::Value>>,

    /// Whether this is a JSON log line
    pub is_json: bool,

    /// Pretty-printed version (cached)
    pub pretty_printed: Option<String>,
}

impl LogEntry {
    /// Create a new log entry with minimal fields
    pub fn new(pod_name: String, line_number: u64, raw: String) -> Self {
        Self {
            id: 0,
            line_number,
            pod_name,
            container_name: None,
            raw,
            timestamp: None,
            level: LogLevel::Unknown,
            fields: None,
            is_json: false,
            pretty_printed: None,
        }
    }

    /// Get a short pod name (last part after last hyphen with hash)
    pub fn short_pod_name(&self) -> &str {
        // Pod names are usually like: deployment-name-replicaset-hash-pod-hash
        // We want to show just the last part for brevity
        self.pod_name.rsplit('-').next().unwrap_or(&self.pod_name)
    }

    /// Get the message content (from JSON field or raw line)
    pub fn message(&self) -> &str {
        if let Some(fields) = &self.fields {
            // Try common message field names
            for key in &["message", "msg", "log", "text", "body"] {
                if let Some(serde_json::Value::String(s)) = fields.get(*key) {
                    return s;
                }
            }
        }
        &self.raw
    }
}
