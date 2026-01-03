use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentType {
    RollingUpdate,
    AllIn,
}

impl DeploymentType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "rolling-update" => Some(Self::RollingUpdate),
            "all-in" => Some(Self::AllIn),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::RollingUpdate => "rolling-update",
            Self::AllIn => "all-in",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeploymentResult {
    pub success: bool,
    pub message: String,
    pub version: Option<String>,
    pub rollback_version: Option<String>,
}

impl DeploymentResult {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            version: None,
            rollback_version: None,
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            version: None,
            rollback_version: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn with_rollback_version(mut self, version: impl Into<String>) -> Self {
        self.rollback_version = Some(version.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct StrategyConfig {
    pub deployment_type: DeploymentType,
    pub batch_size: Option<u32>,
    pub health_check_timeout_secs: Option<u64>,
    pub rollback_on_failure: bool,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            deployment_type: DeploymentType::RollingUpdate,
            batch_size: Some(25),
            health_check_timeout_secs: Some(300),
            rollback_on_failure: true,
        }
    }
}

#[async_trait]
pub trait DeploymentStrategy: Send + Sync {
    fn deployment_type(&self) -> DeploymentType;

    fn validate_config(&self, config: &StrategyConfig) -> anyhow::Result<()>;

    async fn execute(
        &self,
        config: &StrategyConfig,
        deploy_fn: Box<dyn Fn() -> anyhow::Result<()> + Send + Sync>,
    ) -> anyhow::Result<DeploymentResult>;

    async fn rollback(&self) -> anyhow::Result<DeploymentResult>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_type_from_str() {
        assert_eq!(
            DeploymentType::from_str("rolling-update"),
            Some(DeploymentType::RollingUpdate)
        );
        assert_eq!(
            DeploymentType::from_str("all-in"),
            Some(DeploymentType::AllIn)
        );
        assert_eq!(DeploymentType::from_str("recreate"), None);
        assert_eq!(DeploymentType::from_str("direct"), None);
        assert_eq!(DeploymentType::from_str("invalid"), None);
    }

    #[test]
    fn test_deployment_result_builder() {
        let result = DeploymentResult::success("Deployed successfully")
            .with_version("v1.0.0")
            .with_rollback_version("v0.9.0");

        assert!(result.success);
        assert_eq!(result.version, Some("v1.0.0".to_string()));
        assert_eq!(result.rollback_version, Some("v0.9.0".to_string()));
    }
}
