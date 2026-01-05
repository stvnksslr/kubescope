use dashmap::DashMap;
use futures::{AsyncBufReadExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::LogParams;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::logs::LogParser;
use crate::types::{LogEntry, PodInfo};

/// Manages log streaming from multiple pods
pub struct LogStreamManager {
    /// Cancellation token for stopping streams
    cancel: CancellationToken,

    /// Active stream task handles
    tasks: Vec<tokio::task::JoinHandle<()>>,

    /// Line counter per pod (for line numbers) - lock-free concurrent map
    line_counters: Arc<DashMap<String, AtomicU64>>,

    /// Counter for dropped logs due to backpressure
    dropped_count: Arc<AtomicU64>,
}

impl LogStreamManager {
    /// Create a new log stream manager
    pub fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
            tasks: Vec::new(),
            line_counters: Arc::new(DashMap::new()),
            dropped_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the number of dropped logs due to backpressure
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// Reset the dropped log counter
    pub fn reset_dropped_count(&self) {
        self.dropped_count.store(0, Ordering::Relaxed);
    }

    /// Start streaming logs from all pods
    pub fn start_streams(
        &mut self,
        client: kube::Client,
        namespace: &str,
        pods: &[PodInfo],
        log_tx: mpsc::Sender<LogEntry>,
        tail_lines: Option<i64>,
        since_seconds: Option<i64>,
    ) {
        let pods_api: Api<Pod> = Api::namespaced(client, namespace);

        for pod in pods {
            // Initialize line counter for this pod (lock-free)
            self.line_counters
                .insert(pod.name.clone(), AtomicU64::new(0));

            let task = self.spawn_pod_stream(
                pods_api.clone(),
                pod.name.clone(),
                pod.containers.first().map(|c| c.name.clone()),
                log_tx.clone(),
                tail_lines,
                since_seconds,
            );
            self.tasks.push(task);
        }
    }

    fn spawn_pod_stream(
        &self,
        api: Api<Pod>,
        pod_name: String,
        container: Option<String>,
        log_tx: mpsc::Sender<LogEntry>,
        tail_lines: Option<i64>,
        since_seconds: Option<i64>,
    ) -> tokio::task::JoinHandle<()> {
        let cancel = self.cancel.clone();
        let line_counters = Arc::clone(&self.line_counters);
        let dropped_count = Arc::clone(&self.dropped_count);

        tokio::spawn(async move {
            let params = LogParams {
                follow: true,
                container,
                // Use since_seconds if provided, otherwise use tail_lines
                tail_lines: if since_seconds.is_some() {
                    None
                } else {
                    tail_lines
                },
                since_seconds,
                timestamps: true,
                ..Default::default()
            };

            match api.log_stream(&pod_name, &params).await {
                Ok(stream) => {
                    let mut lines = stream.lines();

                    loop {
                        tokio::select! {
                            _ = cancel.cancelled() => break,

                            result = lines.try_next() => {
                                match result {
                                    Ok(Some(line)) => {
                                        // Increment line counter (lock-free via DashMap)
                                        let line_number = line_counters
                                            .entry(pod_name.clone())
                                            .or_insert_with(|| AtomicU64::new(0))
                                            .fetch_add(1, Ordering::Relaxed) + 1;

                                        // Parse the log line
                                        let entry = LogParser::parse(&line, &pod_name, line_number);

                                        // Send to channel with backpressure handling
                                        match log_tx.try_send(entry) {
                                            Ok(()) => {}
                                            Err(mpsc::error::TrySendError::Full(_)) => {
                                                // Channel full - drop log and increment counter
                                                dropped_count.fetch_add(1, Ordering::Relaxed);
                                            }
                                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                                // Channel closed, stop streaming
                                                break;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        // Stream ended (pod terminated?)
                                        break;
                                    }
                                    Err(_) => {
                                        // Error reading stream
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // Failed to start log stream
                }
            }
        })
    }

    /// Stop all streams
    pub fn stop(&mut self) {
        self.cancel.cancel();
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.line_counters.clear();
        // Create a fresh cancellation token for future streams
        self.cancel = CancellationToken::new();
    }

    /// Check if any streams are still running
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.tasks.iter().any(|t| !t.is_finished())
    }

    /// Get the number of active streams
    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.is_finished()).count()
    }
}

impl Default for LogStreamManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LogStreamManager {
    fn drop(&mut self) {
        self.stop();
    }
}
