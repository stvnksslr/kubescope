//! Kubernetes client for kubescope

use anyhow::{Context, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::Api;
use kube::api::ListParams;
use kube::config::{KubeConfigOptions, Kubeconfig};

use crate::types::{ContainerInfo, ContextInfo, DeploymentInfo, NamespaceInfo, PodInfo, PodStatus};

/// Kubernetes client wrapper
pub struct KubeClient {
    kubeconfig: Kubeconfig,
    current_context: Option<String>,
}

impl KubeClient {
    /// Create a new KubeClient by loading the kubeconfig
    pub async fn new() -> Result<Self> {
        let kubeconfig =
            Kubeconfig::read().context("Failed to read kubeconfig. Is kubectl configured?")?;

        let current_context = kubeconfig.current_context.clone();

        Ok(Self {
            kubeconfig,
            current_context,
        })
    }

    /// Get all available contexts from kubeconfig
    pub fn get_contexts(&self) -> Vec<ContextInfo> {
        self.kubeconfig
            .contexts
            .iter()
            .map(|ctx| {
                let context = ctx.context.as_ref();
                ContextInfo::new(
                    ctx.name.clone(),
                    context.map(|c| c.cluster.clone()).unwrap_or_default(),
                    context.and_then(|c| c.user.clone()).unwrap_or_default(),
                    context.and_then(|c| c.namespace.clone()),
                    Some(&ctx.name) == self.current_context.as_ref(),
                )
            })
            .collect()
    }

    /// Create a kube::Client for a specific context
    pub async fn client_for_context(&self, context_name: &str) -> Result<kube::Client> {
        let config = kube::Config::from_custom_kubeconfig(
            self.kubeconfig.clone(),
            &KubeConfigOptions {
                context: Some(context_name.to_string()),
                ..Default::default()
            },
        )
        .await
        .context(format!(
            "Failed to create config for context: {}",
            context_name
        ))?;

        kube::Client::try_from(config).context(format!(
            "Failed to create client for context: {}",
            context_name
        ))
    }

    /// Get the current context name
    #[allow(dead_code)]
    pub fn current_context(&self) -> Option<&str> {
        self.current_context.as_deref()
    }

    /// Fetch all namespaces from the cluster
    pub async fn get_namespaces(&self, client: &kube::Client) -> Result<Vec<NamespaceInfo>> {
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let list = namespaces
            .list(&ListParams::default())
            .await
            .context("Failed to list namespaces")?;

        Ok(list
            .items
            .into_iter()
            .map(|ns| {
                let name = ns.metadata.name.unwrap_or_default();
                let status = ns
                    .status
                    .and_then(|s| s.phase)
                    .unwrap_or_else(|| "Unknown".to_string());
                NamespaceInfo::new(name, status)
            })
            .collect())
    }

    /// Fetch all deployments in a namespace
    pub async fn get_deployments(
        &self,
        client: &kube::Client,
        namespace: &str,
    ) -> Result<Vec<DeploymentInfo>> {
        let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
        let list = deployments
            .list(&ListParams::default())
            .await
            .context(format!("Failed to list deployments in {}", namespace))?;

        Ok(list
            .items
            .into_iter()
            .map(|deploy| {
                let name = deploy.metadata.name.unwrap_or_default();
                let mut info = DeploymentInfo::new(name, namespace.to_string());

                if let Some(spec) = deploy.spec {
                    info.replicas = spec.replicas.unwrap_or(0);

                    // Get the selector labels (convert BTreeMap to HashMap)
                    if let Some(selector) = spec.selector.match_labels {
                        info.selector = selector.into_iter().collect();
                    }
                }

                if let Some(status) = deploy.status {
                    info.available_replicas = status.available_replicas.unwrap_or(0);
                    info.ready_replicas = status.ready_replicas.unwrap_or(0);
                }

                if let Some(labels) = deploy.metadata.labels {
                    info.labels = labels.into_iter().collect();
                }

                info
            })
            .collect())
    }

    /// Fetch pods matching a deployment's selector
    pub async fn get_pods_for_deployment(
        &self,
        client: &kube::Client,
        namespace: &str,
        deployment: &DeploymentInfo,
    ) -> Result<Vec<PodInfo>> {
        let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);

        // Build label selector from deployment's selector
        let label_selector = deployment
            .selector
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");

        let list = pods
            .list(&ListParams::default().labels(&label_selector))
            .await
            .context(format!(
                "Failed to list pods for deployment {}",
                deployment.name
            ))?;

        Ok(list
            .items
            .into_iter()
            .map(|pod| {
                let name = pod.metadata.name.unwrap_or_default();
                let mut info = PodInfo::new(name, namespace.to_string());

                if let Some(spec) = &pod.spec {
                    info.node_name = spec.node_name.clone();
                }

                if let Some(status) = pod.status {
                    info.pod_ip = status.pod_ip;
                    info.status = status
                        .phase
                        .as_deref()
                        .map(PodStatus::from)
                        .unwrap_or(PodStatus::Unknown);

                    // Get container info
                    if let Some(container_statuses) = status.container_statuses {
                        info.containers = container_statuses
                            .into_iter()
                            .map(|cs| {
                                let mut container = ContainerInfo::new(cs.name);
                                container.ready = cs.ready;
                                container.restart_count = cs.restart_count;
                                container
                            })
                            .collect();
                    }
                }

                info
            })
            .collect())
    }
}
