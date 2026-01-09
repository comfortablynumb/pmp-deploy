//! Kubernetes resource management for ConfigMaps, Secrets, HPA, and raw manifests.
//!
//! This module provides functionality to create and manage Kubernetes resources
//! that support application deployments.

use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::api::{Api, DeleteParams, Patch, PatchParams};
use kube::Client;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::config::EnvVarConfig;

// ============================================================================
// Raw Manifest Configuration
// ============================================================================

/// Configuration for raw manifest application.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawManifestConfig {
    /// List of manifest files or directories to apply.
    pub files: Vec<String>,

    /// Whether to search directories recursively.
    #[serde(default)]
    pub recursive: bool,

    /// Whether to prune resources not in the manifests.
    #[serde(default)]
    pub prune: bool,

    /// Label selector for pruning.
    pub prune_selector: Option<String>,
}

impl RawManifestConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

// ============================================================================
// ConfigMap Configuration
// ============================================================================

/// Configuration for a Kubernetes ConfigMap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sConfigMapSpec {
    /// Name of the ConfigMap.
    pub name: String,

    /// Namespace (uses default if not specified).
    pub namespace: Option<String>,

    /// Data key-value pairs.
    #[serde(default)]
    pub data: HashMap<String, String>,

    /// Binary data key-value pairs (base64 encoded).
    #[serde(default)]
    pub binary_data: HashMap<String, String>,

    /// Files to load as data entries (key = filename, value = path).
    #[serde(default)]
    pub data_from_files: HashMap<String, String>,

    /// Labels to apply to the ConfigMap.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Annotations to apply to the ConfigMap.
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

impl K8sConfigMapSpec {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    /// Load file contents into data map.
    pub fn load_files(&mut self) -> anyhow::Result<()> {
        for (key, path) in &self.data_from_files {
            let content = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?;
            self.data.insert(key.clone(), content);
        }
        Ok(())
    }
}

// ============================================================================
// Secret Configuration
// ============================================================================

/// Configuration for a Kubernetes Secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sSecretSpec {
    /// Name of the Secret.
    pub name: String,

    /// Namespace (uses default if not specified).
    pub namespace: Option<String>,

    /// Secret type (default: Opaque).
    #[serde(default = "default_secret_type")]
    pub secret_type: String,

    /// Secret data using EnvVarConfig for flexible value sources.
    #[serde(default)]
    pub data: HashMap<String, EnvVarConfig>,

    /// String data (will be base64 encoded automatically).
    #[serde(default)]
    pub string_data: HashMap<String, String>,

    /// Labels to apply to the Secret.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Annotations to apply to the Secret.
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

fn default_secret_type() -> String {
    "Opaque".to_string()
}

impl K8sSecretSpec {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

// ============================================================================
// HPA Configuration
// ============================================================================

/// Configuration for Horizontal Pod Autoscaler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpaConfig {
    /// Name of the HPA resource.
    pub name: Option<String>,

    /// Target deployment name.
    pub target_deployment: String,

    /// Minimum number of replicas.
    pub min_replicas: i32,

    /// Maximum number of replicas.
    pub max_replicas: i32,

    /// Target CPU utilization percentage.
    pub target_cpu_utilization: Option<i32>,

    /// Target memory utilization percentage.
    pub target_memory_utilization: Option<i32>,

    /// Custom metrics for scaling.
    #[serde(default)]
    pub custom_metrics: Vec<HpaCustomMetric>,

    /// Scale down stabilization window in seconds.
    pub scale_down_stabilization_secs: Option<i32>,

    /// Scale up stabilization window in seconds.
    pub scale_up_stabilization_secs: Option<i32>,
}

impl HpaConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    /// Get the HPA name (defaults to target deployment name).
    pub fn hpa_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{}-hpa", self.target_deployment))
    }
}

/// Custom metric for HPA scaling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpaCustomMetric {
    /// Metric name.
    pub name: String,

    /// Metric type: "Resource", "Pods", "Object", "External".
    #[serde(default = "default_metric_type")]
    pub metric_type: String,

    /// Target value (for AverageValue type).
    pub target_value: Option<String>,

    /// Target average value.
    pub target_average_value: Option<String>,

    /// Target average utilization percentage.
    pub target_average_utilization: Option<i32>,
}

fn default_metric_type() -> String {
    "Resource".to_string()
}

// ============================================================================
// K8sResourceManager
// ============================================================================

/// Manages Kubernetes resources (ConfigMaps, Secrets, HPA).
pub struct K8sResourceManager {
    client: Client,
    namespace: String,
}

impl K8sResourceManager {
    pub fn new(client: Client, namespace: &str) -> Self {
        Self {
            client,
            namespace: namespace.to_string(),
        }
    }

    /// Create or update a ConfigMap.
    pub async fn apply_config_map(&self, spec: &K8sConfigMapSpec) -> anyhow::Result<ConfigMap> {
        let namespace = spec.namespace.as_deref().unwrap_or(&self.namespace);
        let api: Api<ConfigMap> = Api::namespaced(self.client.clone(), namespace);

        let mut data = BTreeMap::new();
        for (k, v) in &spec.data {
            data.insert(k.clone(), v.clone());
        }

        let mut binary_data = BTreeMap::new();
        for (k, v) in &spec.binary_data {
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                v,
            )?;
            binary_data.insert(k.clone(), k8s_openapi::ByteString(decoded));
        }

        let labels: BTreeMap<String, String> = spec.labels.clone().into_iter().collect();
        let annotations: BTreeMap<String, String> = spec.annotations.clone().into_iter().collect();

        let cm = ConfigMap {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(spec.name.clone()),
                namespace: Some(namespace.to_string()),
                labels: if labels.is_empty() {
                    None
                } else {
                    Some(labels)
                },
                annotations: if annotations.is_empty() {
                    None
                } else {
                    Some(annotations)
                },
                ..Default::default()
            },
            data: if data.is_empty() { None } else { Some(data) },
            binary_data: if binary_data.is_empty() {
                None
            } else {
                Some(binary_data)
            },
            ..Default::default()
        };

        let params = PatchParams::apply("pmp-deploy");
        let result = api
            .patch(&spec.name, &params, &Patch::Apply(&cm))
            .await?;

        tracing::info!("Applied ConfigMap '{}'", spec.name);
        Ok(result)
    }

    /// Create or update a Secret.
    pub async fn apply_secret(
        &self,
        spec: &K8sSecretSpec,
        resolved_data: &HashMap<String, String>,
    ) -> anyhow::Result<Secret> {
        let namespace = spec.namespace.as_deref().unwrap_or(&self.namespace);
        let api: Api<Secret> = Api::namespaced(self.client.clone(), namespace);

        let mut string_data = BTreeMap::new();
        for (k, v) in resolved_data {
            string_data.insert(k.clone(), v.clone());
        }
        for (k, v) in &spec.string_data {
            string_data.insert(k.clone(), v.clone());
        }

        let labels: BTreeMap<String, String> = spec.labels.clone().into_iter().collect();
        let annotations: BTreeMap<String, String> = spec.annotations.clone().into_iter().collect();

        let secret = Secret {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(spec.name.clone()),
                namespace: Some(namespace.to_string()),
                labels: if labels.is_empty() {
                    None
                } else {
                    Some(labels)
                },
                annotations: if annotations.is_empty() {
                    None
                } else {
                    Some(annotations)
                },
                ..Default::default()
            },
            type_: Some(spec.secret_type.clone()),
            string_data: if string_data.is_empty() {
                None
            } else {
                Some(string_data)
            },
            ..Default::default()
        };

        let params = PatchParams::apply("pmp-deploy");
        let result = api
            .patch(&spec.name, &params, &Patch::Apply(&secret))
            .await?;

        tracing::info!("Applied Secret '{}'", spec.name);
        Ok(result)
    }

    /// Create or update an HPA.
    pub async fn apply_hpa(
        &self,
        spec: &HpaConfig,
        namespace: Option<&str>,
    ) -> anyhow::Result<HorizontalPodAutoscaler> {
        let ns = namespace.unwrap_or(&self.namespace);
        let api: Api<HorizontalPodAutoscaler> = Api::namespaced(self.client.clone(), ns);

        let hpa_name = spec.hpa_name();

        let mut metrics = Vec::new();

        // CPU metric
        if let Some(cpu) = spec.target_cpu_utilization {
            metrics.push(k8s_openapi::api::autoscaling::v2::MetricSpec {
                type_: "Resource".to_string(),
                resource: Some(k8s_openapi::api::autoscaling::v2::ResourceMetricSource {
                    name: "cpu".to_string(),
                    target: k8s_openapi::api::autoscaling::v2::MetricTarget {
                        type_: "Utilization".to_string(),
                        average_utilization: Some(cpu),
                        ..Default::default()
                    },
                }),
                ..Default::default()
            });
        }

        // Memory metric
        if let Some(memory) = spec.target_memory_utilization {
            metrics.push(k8s_openapi::api::autoscaling::v2::MetricSpec {
                type_: "Resource".to_string(),
                resource: Some(k8s_openapi::api::autoscaling::v2::ResourceMetricSource {
                    name: "memory".to_string(),
                    target: k8s_openapi::api::autoscaling::v2::MetricTarget {
                        type_: "Utilization".to_string(),
                        average_utilization: Some(memory),
                        ..Default::default()
                    },
                }),
                ..Default::default()
            });
        }

        let hpa = HorizontalPodAutoscaler {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(hpa_name.clone()),
                namespace: Some(ns.to_string()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscalerSpec {
                scale_target_ref: k8s_openapi::api::autoscaling::v2::CrossVersionObjectReference {
                    api_version: Some("apps/v1".to_string()),
                    kind: "Deployment".to_string(),
                    name: spec.target_deployment.clone(),
                },
                min_replicas: Some(spec.min_replicas),
                max_replicas: spec.max_replicas,
                metrics: if metrics.is_empty() {
                    None
                } else {
                    Some(metrics)
                },
                behavior: self.build_hpa_behavior(spec),
            }),
            ..Default::default()
        };

        let params = PatchParams::apply("pmp-deploy");
        let result = api.patch(&hpa_name, &params, &Patch::Apply(&hpa)).await?;

        tracing::info!(
            "Applied HPA '{}' for deployment '{}'",
            hpa_name,
            spec.target_deployment
        );
        Ok(result)
    }

    fn build_hpa_behavior(
        &self,
        spec: &HpaConfig,
    ) -> Option<k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscalerBehavior> {
        let scale_down = spec.scale_down_stabilization_secs.map(|secs| {
            k8s_openapi::api::autoscaling::v2::HPAScalingRules {
                stabilization_window_seconds: Some(secs),
                ..Default::default()
            }
        });

        let scale_up = spec.scale_up_stabilization_secs.map(|secs| {
            k8s_openapi::api::autoscaling::v2::HPAScalingRules {
                stabilization_window_seconds: Some(secs),
                ..Default::default()
            }
        });

        if scale_down.is_some() || scale_up.is_some() {
            Some(k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscalerBehavior {
                scale_down,
                scale_up,
            })
        } else {
            None
        }
    }

    /// Delete a ConfigMap.
    pub async fn delete_config_map(&self, name: &str) -> anyhow::Result<()> {
        let api: Api<ConfigMap> = Api::namespaced(self.client.clone(), &self.namespace);
        api.delete(name, &DeleteParams::default()).await?;
        tracing::info!("Deleted ConfigMap '{}'", name);
        Ok(())
    }

    /// Delete a Secret.
    pub async fn delete_secret(&self, name: &str) -> anyhow::Result<()> {
        let api: Api<Secret> = Api::namespaced(self.client.clone(), &self.namespace);
        api.delete(name, &DeleteParams::default()).await?;
        tracing::info!("Deleted Secret '{}'", name);
        Ok(())
    }

    /// Delete an HPA.
    pub async fn delete_hpa(&self, name: &str) -> anyhow::Result<()> {
        let api: Api<HorizontalPodAutoscaler> =
            Api::namespaced(self.client.clone(), &self.namespace);
        api.delete(name, &DeleteParams::default()).await?;
        tracing::info!("Deleted HPA '{}'", name);
        Ok(())
    }

    /// Get HPA status.
    pub async fn get_hpa_status(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<HorizontalPodAutoscaler>> {
        let api: Api<HorizontalPodAutoscaler> =
            Api::namespaced(self.client.clone(), &self.namespace);
        api.get_opt(name).await.map_err(|e| e.into())
    }
}

/// Apply raw manifests using kubectl.
pub struct RawManifestApplier {
    namespace: String,
    context: Option<String>,
}

impl RawManifestApplier {
    pub fn new(namespace: &str, context: Option<&str>) -> Self {
        Self {
            namespace: namespace.to_string(),
            context: context.map(String::from),
        }
    }

    /// Apply manifests from files.
    pub async fn apply(
        &self,
        config: &RawManifestConfig,
        dry_run: bool,
    ) -> anyhow::Result<String> {
        if config.files.is_empty() {
            anyhow::bail!("No manifest files specified");
        }

        let mut args = vec!["apply".to_string()];

        // Add files
        for file in &config.files {
            if config.recursive {
                args.push("-R".to_string());
            }
            args.push("-f".to_string());
            args.push(file.clone());
        }

        // Namespace
        args.push("-n".to_string());
        args.push(self.namespace.clone());

        // Context
        if let Some(ctx) = &self.context {
            args.push("--context".to_string());
            args.push(ctx.clone());
        }

        // Prune
        if config.prune {
            args.push("--prune".to_string());
            if let Some(selector) = &config.prune_selector {
                args.push("-l".to_string());
                args.push(selector.clone());
            }
        }

        // Dry run
        if dry_run {
            args.push("--dry-run=client".to_string());
        }

        let output = tokio::process::Command::new("kubectl")
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("kubectl apply failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    }

    /// Apply rendered manifest content directly.
    pub async fn apply_content(&self, content: &str, dry_run: bool) -> anyhow::Result<String> {
        let mut args = vec!["apply".to_string(), "-f".to_string(), "-".to_string()];

        // Namespace
        args.push("-n".to_string());
        args.push(self.namespace.clone());

        // Context
        if let Some(ctx) = &self.context {
            args.push("--context".to_string());
            args.push(ctx.clone());
        }

        // Dry run
        if dry_run {
            args.push("--dry-run=client".to_string());
        }

        let mut child = tokio::process::Command::new("kubectl")
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Write content to stdin
        if let Some(stdin) = child.stdin.as_mut() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(content.as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("kubectl apply failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    }

    /// Diff manifests against current state.
    pub async fn diff(&self, config: &RawManifestConfig) -> anyhow::Result<String> {
        if config.files.is_empty() {
            anyhow::bail!("No manifest files specified");
        }

        let mut args = vec!["diff".to_string()];

        for file in &config.files {
            if config.recursive {
                args.push("-R".to_string());
            }
            args.push("-f".to_string());
            args.push(file.clone());
        }

        args.push("-n".to_string());
        args.push(self.namespace.clone());

        if let Some(ctx) = &self.context {
            args.push("--context".to_string());
            args.push(ctx.clone());
        }

        let output = tokio::process::Command::new("kubectl")
            .args(&args)
            .output()
            .await?;

        // kubectl diff returns exit code 1 if there are differences
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stderr.is_empty() && !output.status.success() && output.status.code() != Some(1) {
            anyhow::bail!("kubectl diff failed: {}", stderr);
        }

        Ok(stdout.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_manifest_config_default() {
        let config = RawManifestConfig::default();
        assert!(config.files.is_empty());
        assert!(!config.recursive);
        assert!(!config.prune);
    }

    #[test]
    fn test_raw_manifest_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
files:
  - ./manifests/deployment.yaml
  - ./manifests/service.yaml
recursive: true
prune: true
prune_selector: app=myapp
"#,
        )
        .unwrap();

        let config = RawManifestConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(config.recursive);
        assert!(config.prune);
        assert_eq!(config.prune_selector, Some("app=myapp".to_string()));
    }

    #[test]
    fn test_hpa_config_name() {
        let config = HpaConfig {
            name: None,
            target_deployment: "my-app".to_string(),
            min_replicas: 1,
            max_replicas: 10,
            target_cpu_utilization: Some(70),
            target_memory_utilization: None,
            custom_metrics: vec![],
            scale_down_stabilization_secs: None,
            scale_up_stabilization_secs: None,
        };

        assert_eq!(config.hpa_name(), "my-app-hpa");

        let config_with_name = HpaConfig {
            name: Some("custom-hpa".to_string()),
            ..config
        };

        assert_eq!(config_with_name.hpa_name(), "custom-hpa");
    }

    #[test]
    fn test_hpa_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
target_deployment: my-app
min_replicas: 2
max_replicas: 20
target_cpu_utilization: 70
target_memory_utilization: 80
scale_down_stabilization_secs: 300
"#,
        )
        .unwrap();

        let config = HpaConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.target_deployment, "my-app");
        assert_eq!(config.min_replicas, 2);
        assert_eq!(config.max_replicas, 20);
        assert_eq!(config.target_cpu_utilization, Some(70));
        assert_eq!(config.target_memory_utilization, Some(80));
        assert_eq!(config.scale_down_stabilization_secs, Some(300));
    }

    #[test]
    fn test_configmap_spec_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
name: app-config
namespace: production
data:
  LOG_LEVEL: info
  API_URL: https://api.example.com
labels:
  app: my-app
"#,
        )
        .unwrap();

        let spec = K8sConfigMapSpec::from_yaml_value(&yaml).unwrap();
        assert_eq!(spec.name, "app-config");
        assert_eq!(spec.namespace, Some("production".to_string()));
        assert_eq!(spec.data.get("LOG_LEVEL"), Some(&"info".to_string()));
        assert_eq!(spec.labels.get("app"), Some(&"my-app".to_string()));
    }

    #[test]
    fn test_secret_spec_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
name: db-credentials
secret_type: Opaque
string_data:
  username: admin
labels:
  app: my-app
"#,
        )
        .unwrap();

        let spec = K8sSecretSpec::from_yaml_value(&yaml).unwrap();
        assert_eq!(spec.name, "db-credentials");
        assert_eq!(spec.secret_type, "Opaque");
        assert_eq!(
            spec.string_data.get("username"),
            Some(&"admin".to_string())
        );
    }
}
