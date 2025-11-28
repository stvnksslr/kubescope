//! Kubernetes client for kubescope
//!
//! This crate provides Kubernetes API integration for fetching contexts,
//! namespaces, deployments, and pods.

mod client;

pub use client::KubeClient;

// Re-export types that are used in our public API
pub use kubescope_types::{
    ContainerInfo, ContextInfo, DeploymentInfo, NamespaceInfo, PodInfo, PodStatus,
};
