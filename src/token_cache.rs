//! Token caching for EKS authentication
//!
//! Caches AWS EKS tokens to avoid repeated slow exec calls to `aws eks get-token`.
//! Tokens are cached per cluster with a 5-minute TTL (EKS tokens are valid for 15 minutes).

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
    fn cache_path() -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        let cache_dir = home.join(".kubescope");
        Some(cache_dir.join("token-cache.json"))
    }

    /// Load the token cache from disk
    pub fn load() -> Self {
        Self::cache_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save the token cache to disk
    pub fn save(&self) {
        let Some(path) = Self::cache_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
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

/// Get a cached token for a cluster (read-only, doesn't fetch new tokens)
/// Returns None if no valid cached token exists
pub fn get_cached_token(cluster_name: &str) -> Option<String> {
    let cache = TokenCache::load();
    cache.get(cluster_name).map(|t| t.token.clone())
}

/// Clear the cached token for a cluster (used when cached token is invalid)
pub fn clear_token(cluster_name: &str) {
    let mut cache = TokenCache::load();
    cache.tokens.remove(cluster_name);
    cache.cleanup();
    cache.save();
}

/// Store a token in the cache after successful authentication
pub fn cache_token(cluster_name: &str, token: &str) {
    let mut cache = TokenCache::load();
    cache.set(cluster_name.to_string(), token.to_string());
    cache.cleanup();
    cache.save();
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
