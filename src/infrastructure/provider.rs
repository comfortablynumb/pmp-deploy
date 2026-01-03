use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::{EnvVarResolver, EnvironmentConfig};
use crate::deployment::{DeploymentResult, DeploymentType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InfrastructureType {
    AwsEks,
    AwsEcs,
    AwsLambda,
    Kubernetes,
    DockerCompose,
    Custom(String),
}

impl Serialize for InfrastructureType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for InfrastructureType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str(&s))
    }
}

impl InfrastructureType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "aws-eks" => Self::AwsEks,
            "aws-ecs" => Self::AwsEcs,
            "aws-lambda" => Self::AwsLambda,
            "kubernetes" => Self::Kubernetes,
            "docker-compose" => Self::DockerCompose,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::AwsEks => "aws-eks",
            Self::AwsEcs => "aws-ecs",
            Self::AwsLambda => "aws-lambda",
            Self::Kubernetes => "kubernetes",
            Self::DockerCompose => "docker-compose",
            Self::Custom(s) => s,
        }
    }
}

/// Deployment mode controls what gets updated during a deployment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployMode {
    /// Full deployment: updates application AND infrastructure (auto-scaling, etc.)
    Full,
    /// App-only deployment: only updates the application image/version
    /// Does not modify infrastructure settings like auto-scaling, load balancers, etc.
    #[default]
    AppOnly,
}

impl DeployMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "app-only" | "apponly" | "app" | "image-only" => Self::AppOnly,
            _ => Self::Full,
        }
    }

    pub fn is_app_only(&self) -> bool {
        matches!(self, Self::AppOnly)
    }
}

#[derive(Debug, Clone)]
pub struct DeploymentContext {
    pub environment_name: String,
    pub environment: EnvironmentConfig,
    pub dry_run: bool,
    pub verbose: bool,
    pub deploy_mode: DeployMode,
}

impl DeploymentContext {
    /// Resolve all environment variables from the configuration.
    /// Combines legacy `env` values with new `environment` configuration,
    /// resolving secrets from AWS Secrets Manager, Vault, etc.
    pub async fn resolve_env_vars(&self) -> anyhow::Result<HashMap<String, String>> {
        let env_vars = self.environment.get_all_env_vars();
        let resolver = EnvVarResolver::new();
        resolver.resolve(&env_vars).await
    }

    /// Resolve environment variables with a custom resolver.
    /// Useful when you need to specify AWS region or Vault configuration.
    pub async fn resolve_env_vars_with(
        &self,
        resolver: &EnvVarResolver,
    ) -> anyhow::Result<HashMap<String, String>> {
        let env_vars = self.environment.get_all_env_vars();
        resolver.resolve(&env_vars).await
    }
}

#[async_trait]
pub trait InfrastructureProvider: Send + Sync {
    fn infrastructure_type(&self) -> InfrastructureType;

    /// Returns the list of deployment types supported by this provider.
    fn supported_deployment_types(&self) -> Vec<DeploymentType>;

    /// Validates that the given deployment type is supported by this provider.
    /// Returns an error if the deployment type is not supported.
    fn validate_deployment_type(&self, deployment_type: &DeploymentType) -> anyhow::Result<()> {
        let supported = self.supported_deployment_types();

        if supported.contains(deployment_type) {
            Ok(())
        } else {
            let supported_str: Vec<&str> = supported.iter().map(|t| t.as_str()).collect();
            anyhow::bail!(
                "Deployment type '{}' is not supported by {} infrastructure. Supported types: {}",
                deployment_type.as_str(),
                self.infrastructure_type().as_str(),
                supported_str.join(", ")
            )
        }
    }

    async fn validate_config(&self, config: &HashMap<String, serde_yaml::Value>)
        -> anyhow::Result<()>;

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult>;

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult>;

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String>;

    async fn logs(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infrastructure_type_from_str() {
        assert_eq!(
            InfrastructureType::from_str("aws-eks"),
            InfrastructureType::AwsEks
        );
        assert_eq!(
            InfrastructureType::from_str("aws-ecs"),
            InfrastructureType::AwsEcs
        );
        assert_eq!(
            InfrastructureType::from_str("kubernetes"),
            InfrastructureType::Kubernetes
        );
    }

    #[test]
    fn test_deploy_mode_from_str() {
        assert_eq!(DeployMode::from_str("full"), DeployMode::Full);
        assert_eq!(DeployMode::from_str("app-only"), DeployMode::AppOnly);
        assert_eq!(DeployMode::from_str("apponly"), DeployMode::AppOnly);
        assert_eq!(DeployMode::from_str("app"), DeployMode::AppOnly);
        assert_eq!(DeployMode::from_str("image-only"), DeployMode::AppOnly);
        assert_eq!(DeployMode::from_str("unknown"), DeployMode::Full);
        assert_eq!(DeployMode::from_str("FULL"), DeployMode::Full);
        assert_eq!(DeployMode::from_str("APP-ONLY"), DeployMode::AppOnly);
    }

    #[test]
    fn test_deploy_mode_is_app_only() {
        assert!(!DeployMode::Full.is_app_only());
        assert!(DeployMode::AppOnly.is_app_only());
    }

    #[test]
    fn test_deploy_mode_default() {
        let mode: DeployMode = Default::default();
        assert_eq!(mode, DeployMode::AppOnly);
        assert!(mode.is_app_only());
    }

    #[test]
    fn test_deploy_mode_serialization() {
        let full = serde_yaml::to_string(&DeployMode::Full).unwrap();
        assert_eq!(full.trim(), "full");

        let app_only = serde_yaml::to_string(&DeployMode::AppOnly).unwrap();
        assert_eq!(app_only.trim(), "app-only");
    }

    #[test]
    fn test_deploy_mode_deserialization() {
        let full: DeployMode = serde_yaml::from_str("full").unwrap();
        assert_eq!(full, DeployMode::Full);

        let app_only: DeployMode = serde_yaml::from_str("app-only").unwrap();
        assert_eq!(app_only, DeployMode::AppOnly);
    }
}
