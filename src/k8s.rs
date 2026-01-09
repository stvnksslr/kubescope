//! Kubernetes client for kubescope

use anyhow::{Context, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::Api;
use kube::api::ListParams;
use kube::config::{AuthInfo, KubeConfigOptions, Kubeconfig, NamedAuthInfo};

use crate::token_cache;
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
    /// Uses token caching for EKS clusters to avoid slow exec calls on repeated startups
    pub async fn client_for_context(&self, context_name: &str) -> Result<kube::Client> {
        // Check if this is an EKS cluster and try to use cached token
        let kubeconfig = self.try_with_cached_token(context_name).await;

        let config = kube::Config::from_custom_kubeconfig(
            kubeconfig,
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

    /// Try to use a cached token for EKS clusters
    /// Returns modified kubeconfig with token if cached, otherwise returns original
    async fn try_with_cached_token(&self, context_name: &str) -> Kubeconfig {
        // Check if this is an EKS cluster
        let Some(cluster_name) =
            token_cache::extract_eks_cluster_name(&self.kubeconfig, context_name)
        else {
            return self.kubeconfig.clone();
        };

        // Try to get cached token
        let token = match token_cache::get_eks_token(&cluster_name).await {
            Ok(token) => token,
            Err(_) => return self.kubeconfig.clone(),
        };

        // Create modified kubeconfig with token instead of exec
        self.kubeconfig_with_token(context_name, &token)
    }

    /// Create a copy of kubeconfig with token-based auth instead of exec
    fn kubeconfig_with_token(&self, context_name: &str, token: &str) -> Kubeconfig {
        let mut kubeconfig = self.kubeconfig.clone();

        // Find the user for this context
        let user_name = kubeconfig
            .contexts
            .iter()
            .find(|c| c.name == context_name)
            .and_then(|c| c.context.as_ref())
            .and_then(|c| c.user.clone());

        let Some(user_name) = user_name else {
            return kubeconfig;
        };

        // Replace the auth info with token-based auth
        if let Some(auth_info) = kubeconfig
            .auth_infos
            .iter_mut()
            .find(|a| a.name == user_name)
        {
            auth_info.auth_info = Some(AuthInfo {
                token: Some(token.to_string().into()),
                ..Default::default()
            });
        } else {
            // Add new auth info if not found
            kubeconfig.auth_infos.push(NamedAuthInfo {
                name: user_name,
                auth_info: Some(AuthInfo {
                    token: Some(token.to_string().into()),
                    ..Default::default()
                }),
            });
        }

        kubeconfig
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
            .map(|d| Self::deployment_to_info(d, namespace))
            .collect())
    }

    /// Fetch a single deployment by name (faster than listing all)
    pub async fn get_deployment(
        &self,
        client: &kube::Client,
        namespace: &str,
        name: &str,
    ) -> Result<DeploymentInfo> {
        let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
        let deploy = deployments.get(name).await.context(format!(
            "Failed to get deployment '{}' in namespace '{}'",
            name, namespace
        ))?;

        Ok(Self::deployment_to_info(deploy, namespace))
    }

    /// Convert a k8s Deployment to DeploymentInfo
    fn deployment_to_info(deploy: Deployment, namespace: &str) -> DeploymentInfo {
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
