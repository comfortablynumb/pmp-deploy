use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::env_vars::EnvVarConfig;
use crate::deployment::DeploymentType;
use crate::hooks::HooksConfig;
use crate::storage::StorageConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub infrastructure: HashMap<String, InfrastructureConfig>,

    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,

    #[serde(default)]
    pub secrets: Option<SecretsConfig>,

    #[serde(default)]
    pub metrics: Option<MetricsConfig>,
}

impl Config {
    pub fn get_infrastructure(&self, name: &str) -> Option<&InfrastructureConfig> {
        self.infrastructure.get(name)
    }

    pub fn get_environment(&self, name: &str) -> Option<&EnvironmentConfig> {
        self.environments.get(name)
    }

    pub fn list_environments(&self) -> Vec<&str> {
        self.environments.keys().map(|s| s.as_str()).collect()
    }

    pub fn list_infrastructures(&self) -> Vec<&str> {
        self.infrastructure.keys().map(|s| s.as_str()).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfrastructureConfig {
    #[serde(rename = "type")]
    pub infrastructure_type: String,

    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub infrastructure: String,

    #[serde(default = "default_deployment_type")]
    pub deployment_type: String,

    pub image: Option<String>,

    #[serde(default)]
    pub replicas: Option<u32>,

    #[serde(default)]
    pub resources: Option<ResourceConfig>,

    /// Legacy simple environment variables (key: value)
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// New environment variable configuration with source support
    #[serde(default)]
    pub environment: HashMap<String, EnvVarConfig>,

    /// Pre/post deployment hooks
    #[serde(default)]
    pub hooks: Option<HooksConfig>,

    #[serde(default)]
    pub config: HashMap<String, serde_yaml::Value>,
}

fn default_deployment_type() -> String {
    "rolling-update".to_string()
}

impl EnvironmentConfig {
    pub fn get_deployment_type(&self) -> Option<DeploymentType> {
        DeploymentType::from_str(&self.deployment_type)
    }

    /// Get all environment variables, merging legacy `env` and new `environment` formats.
    /// New format takes precedence over legacy format for the same key.
    pub fn get_all_env_vars(&self) -> HashMap<String, EnvVarConfig> {
        let mut result = super::env_vars::from_legacy_env(&self.env);

        // New format overrides legacy format
        for (key, config) in &self.environment {
            result.insert(key.clone(), config.clone());
        }

        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    #[serde(default)]
    pub provider: SecretsProvider,

    #[serde(default)]
    pub secrets: HashMap<String, SecretReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SecretsProvider {
    #[default]
    Environment,
    AwsSecretsManager,
    HashicorpVault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretReference {
    #[serde(default)]
    pub provider: Option<SecretsProvider>,
    pub key: String,
    #[serde(default)]
    pub version: Option<String>,
}

// ============================================================================
// Metrics Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    #[serde(default)]
    pub cloudwatch: Option<CloudWatchMetricsConfig>,

    #[serde(default)]
    pub prometheus: Option<PrometheusMetricsConfig>,

    /// Maps infrastructure types to metrics provider names.
    #[serde(default)]
    pub infrastructure_mapping: HashMap<String, String>,

    /// Global thresholds for standard metrics.
    #[serde(default)]
    pub thresholds: Option<GlobalMetricThresholds>,

    /// Custom metric definitions.
    #[serde(default)]
    pub custom_metrics: Vec<CustomMetricDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudWatchMetricsConfig {
    pub region: Option<String>,

    #[serde(default)]
    pub namespace: Option<String>,

    #[serde(default)]
    pub ecs_cluster: Option<String>,

    #[serde(default)]
    pub ecs_service: Option<String>,

    #[serde(default)]
    pub eks_cluster: Option<String>,

    #[serde(default)]
    pub lambda_function: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusMetricsConfig {
    pub url: String,

    #[serde(default)]
    pub username: Option<String>,

    #[serde(default)]
    pub password: Option<SecretReference>,

    #[serde(default = "default_prometheus_timeout")]
    pub timeout_seconds: u32,
}

fn default_prometheus_timeout() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalMetricThresholds {
    #[serde(default)]
    pub cpu_utilization: Option<ThresholdPair>,

    #[serde(default)]
    pub memory_utilization: Option<ThresholdPair>,

    #[serde(default)]
    pub error_rate: Option<ThresholdPair>,

    #[serde(default)]
    pub request_latency: Option<ThresholdPair>,

    #[serde(default)]
    pub request_count: Option<ThresholdPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdPair {
    pub warning: f64,
    pub critical: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetricDefinition {
    pub name: String,

    pub display_name: String,

    pub provider: String,

    /// Provider-specific query (CloudWatch JSON or PromQL).
    pub query: String,

    pub unit: String,

    #[serde(default)]
    pub thresholds: Option<ThresholdPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub projects: Vec<ProjectReference>,

    #[serde(default)]
    pub defaults: Option<GlobalDefaults>,

    /// Storage configuration for deployment history persistence.
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

impl GlobalConfig {
    pub fn find_project(&self, name: &str) -> Option<&ProjectReference> {
        self.projects.iter().find(|p| {
            p.name.as_deref() == Some(name)
                || p.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == name)
                    .unwrap_or(false)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectReference {
    pub path: PathBuf,

    #[serde(default)]
    pub name: Option<String>,
}

impl ProjectReference {
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        })
    }

    pub fn expanded_path(&self) -> PathBuf {
        let path_str = self.path.to_string_lossy().to_string();
        let expanded = shellexpand::full(&path_str)
            .map(|s| s.to_string())
            .unwrap_or(path_str);
        PathBuf::from(expanded)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalDefaults {
    pub secrets_provider: Option<SecretsProvider>,
    pub deployment_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"
infrastructure:
  aws-dev:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:v1.0.0
    replicas: 3
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();

        assert!(config.infrastructure.contains_key("aws-dev"));
        assert!(config.environments.contains_key("development"));

        let env = config.get_environment("development").unwrap();
        assert_eq!(env.infrastructure, "aws-dev");
        assert_eq!(env.image, Some("my-app:v1.0.0".to_string()));
    }

    #[test]
    fn test_parse_global_config() {
        let yaml = r#"
projects:
  - path: $HOME/my-project
    name: my-project
  - path: /opt/another-project
"#;
        let config: GlobalConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.projects.len(), 2);
        assert_eq!(config.projects[0].name, Some("my-project".to_string()));
    }

    #[test]
    fn test_project_reference_display_name() {
        let with_name = ProjectReference {
            path: PathBuf::from("/some/path"),
            name: Some("custom-name".to_string()),
        };
        assert_eq!(with_name.display_name(), "custom-name");

        let without_name = ProjectReference {
            path: PathBuf::from("/some/project"),
            name: None,
        };
        assert_eq!(without_name.display_name(), "project");
    }
}
