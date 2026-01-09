//! Token caching for EKS authentication
//!
//! Caches AWS EKS tokens to avoid repeated slow exec calls to `aws eks get-token`.
//! Tokens are cached per cluster with a 5-minute TTL (EKS tokens are valid for 15 minutes).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default TTL for cached tokens (5 minutes)
const TOKEN_CACHE_TTL_SECS: u64 = 300;

/// Cached token entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedToken {
    pub token: String,
    pub expiration_timestamp: u64,
}

impl CachedToken {
    /// Check if the token is still valid (not expired)
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Add 30 second buffer before expiration
        self.expiration_timestamp > now + 30
    }
}

/// Token cache stored on disk
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TokenCache {
    /// Map of cluster name to cached token
    pub tokens: HashMap<String, CachedToken>,
}

impl TokenCache {
    /// Get the cache file path
    fn cache_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let cache_dir = home.join(".kubescope");
        Ok(cache_dir.join("token-cache.json"))
    }

    /// Load the token cache from disk
    pub fn load() -> Self {
        Self::cache_path()
            .ok()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save the token cache to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Get a cached token for a cluster if valid
    pub fn get(&self, cluster_name: &str) -> Option<&CachedToken> {
        self.tokens.get(cluster_name).filter(|t| t.is_valid())
    }

    /// Store a token in the cache
    pub fn set(&mut self, cluster_name: String, token: String) {
        let expiration_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + TOKEN_CACHE_TTL_SECS;

        self.tokens.insert(
            cluster_name,
            CachedToken {
                token,
                expiration_timestamp,
            },
        );
    }

    /// Clean up expired tokens
    pub fn cleanup(&mut self) {
        self.tokens.retain(|_, t| t.is_valid());
    }
}

/// AWS EKS get-token response format
#[derive(Debug, Deserialize)]
struct EksTokenResponse {
    status: EksTokenStatus,
}

#[derive(Debug, Deserialize)]
struct EksTokenStatus {
    token: String,
    // Note: expirationTimestamp is available but we use our own TTL for simplicity
    #[serde(rename = "expirationTimestamp")]
    #[allow(dead_code)]
    expiration_timestamp: String,
}

/// Get an EKS token for a cluster, using cache if available
pub async fn get_eks_token(cluster_name: &str) -> Result<String> {
    // Try to get from cache first
    let mut cache = TokenCache::load();
    if let Some(cached) = cache.get(cluster_name) {
        return Ok(cached.token.clone());
    }

    // Not in cache or expired, get fresh token
    let token = fetch_eks_token(cluster_name).await?;

    // Cache the token
    cache.set(cluster_name.to_string(), token.clone());
    cache.cleanup();
    let _ = cache.save(); // Ignore save errors

    Ok(token)
}

/// Fetch a fresh EKS token using aws CLI
async fn fetch_eks_token(cluster_name: &str) -> Result<String> {
    let output = tokio::process::Command::new("aws")
        .args([
            "eks",
            "get-token",
            "--cluster-name",
            cluster_name,
            "--output",
            "json",
        ])
        .output()
        .await
        .context("Failed to run aws eks get-token")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("aws eks get-token failed: {}", stderr);
    }

    let response: EksTokenResponse = serde_json::from_slice(&output.stdout)
        .context("Failed to parse aws eks get-token output")?;

    Ok(response.status.token)
}

/// Extract cluster name from kubeconfig context
/// Returns None if not an EKS cluster or cluster name can't be determined
pub fn extract_eks_cluster_name(
    kubeconfig: &kube::config::Kubeconfig,
    context_name: &str,
) -> Option<String> {
    // Find the context
    let context = kubeconfig
        .contexts
        .iter()
        .find(|c| c.name == context_name)?;
    let context_data = context.context.as_ref()?;
    let cluster_name = &context_data.cluster;

    // Find the cluster config
    let cluster = kubeconfig
        .clusters
        .iter()
        .find(|c| &c.name == cluster_name)?;
    let cluster_data = cluster.cluster.as_ref()?;

    // Check if it's an EKS cluster by looking at the server URL
    let server = cluster_data.server.as_ref()?;
    if !server.contains(".eks.amazonaws.com") {
        return None;
    }

    // Find the auth info to check for exec config
    let user_name = context_data.user.as_ref()?;
    let auth_info = kubeconfig
        .auth_infos
        .iter()
        .find(|a| &a.name == user_name)?;
    let auth_data = auth_info.auth_info.as_ref()?;

    // Check if using exec with aws
    let exec_config = auth_data.exec.as_ref()?;
    if exec_config.command.as_deref() != Some("aws") {
        return None;
    }

    // Extract cluster name from exec args
    // Args typically: ["eks", "get-token", "--cluster-name", "<cluster>", ...]
    let args = exec_config.args.as_ref()?;
    let cluster_idx = args.iter().position(|a| a == "--cluster-name")?;
    args.get(cluster_idx + 1).cloned()
}
