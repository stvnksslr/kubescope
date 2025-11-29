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

    /// Line counter per pod (for line numbers)
    line_counters: Arc<parking_lot::RwLock<std::collections::HashMap<String, AtomicU64>>>,
}

impl LogStreamManager {
    /// Create a new log stream manager
    pub fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
            tasks: Vec::new(),
            line_counters: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Start streaming logs from all pods
    pub fn start_streams(
        &mut self,
        client: kube::Client,
        namespace: &str,
        pods: &[PodInfo],
        log_tx: mpsc::UnboundedSender<LogEntry>,
        tail_lines: Option<i64>,
        since_seconds: Option<i64>,
    ) {
        let pods_api: Api<Pod> = Api::namespaced(client, namespace);

        for pod in pods {
            // Initialize line counter for this pod
            {
                let mut counters = self.line_counters.write();
                counters.insert(pod.name.clone(), AtomicU64::new(0));
            }

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
        log_tx: mpsc::UnboundedSender<LogEntry>,
        tail_lines: Option<i64>,
        since_seconds: Option<i64>,
    ) -> tokio::task::JoinHandle<()> {
        let cancel = self.cancel.clone();
        let line_counters = Arc::clone(&self.line_counters);

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
                                        // Increment line counter
                                        let line_number = {
                                            let counters = line_counters.read();
                                            if let Some(counter) = counters.get(&pod_name) {
                                                counter.fetch_add(1, Ordering::SeqCst) + 1
                                            } else {
                                                1
                                            }
                                        };

                                        // Parse the log line
                                        let entry = LogParser::parse(&line, &pod_name, line_number);

                                        // Send to channel
                                        if log_tx.send(entry).is_err() {
                                            // Channel closed, stop streaming
                                            break;
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
        self.line_counters.write().clear();
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
