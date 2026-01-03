use std::sync::Arc;

use super::rolling::RollingUpdateStrategy;
use super::strategy::{DeploymentResult, DeploymentStrategy, DeploymentType, StrategyConfig};
use crate::config::EnvironmentConfig;
use crate::infrastructure::{DeployMode, DeploymentContext, InfrastructureProvider};

/// Factory for creating deployment strategies
pub struct StrategyFactory;

impl StrategyFactory {
    pub fn create(deployment_type: &DeploymentType) -> Box<dyn DeploymentStrategy> {
        match deployment_type {
            DeploymentType::RollingUpdate => Box::new(RollingUpdateStrategy::new()),
            DeploymentType::AllIn => Box::new(RollingUpdateStrategy::new()), // Use rolling with 100% batch
        }
    }

    pub fn create_config(env: &EnvironmentConfig) -> StrategyConfig {
        let deployment_type = env.get_deployment_type().unwrap_or(DeploymentType::RollingUpdate);

        let mut config = StrategyConfig {
            deployment_type: deployment_type.clone(),
            ..Default::default()
        };

        // Extract strategy-specific config from environment
        if let Some(batch) = env.config.get("batch_size").and_then(|v| v.as_u64()) {
            config.batch_size = Some(batch as u32);
        }

        if let Some(timeout) = env
            .config
            .get("health_check_timeout_secs")
            .and_then(|v| v.as_u64())
        {
            config.health_check_timeout_secs = Some(timeout);
        }

        if let Some(rollback) = env
            .config
            .get("rollback_on_failure")
            .and_then(|v| v.as_bool())
        {
            config.rollback_on_failure = rollback;
        }

        // For AllIn strategy, set batch_size to 100%
        if deployment_type == DeploymentType::AllIn {
            config.batch_size = Some(100);
        }

        config
    }
}

/// Executes deployments using the appropriate strategy
pub struct DeploymentExecutor {
    provider: Arc<dyn InfrastructureProvider>,
}

impl DeploymentExecutor {
    pub fn new(provider: Arc<dyn InfrastructureProvider>) -> Self {
        Self { provider }
    }

    pub async fn execute(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        dry_run: bool,
        verbose: bool,
        deploy_mode: DeployMode,
    ) -> anyhow::Result<DeploymentResult> {
        let deployment_type = env.get_deployment_type().unwrap_or(DeploymentType::RollingUpdate);

        // Validate that the provider supports this deployment type
        self.provider.validate_deployment_type(&deployment_type)?;

        let strategy = StrategyFactory::create(&deployment_type);
        let config = StrategyFactory::create_config(env);

        // Validate strategy config
        strategy.validate_config(&config)?;

        tracing::info!(
            "Executing {} deployment strategy (mode: {:?})",
            deployment_type.as_str(),
            deploy_mode
        );

        let ctx = DeploymentContext {
            environment_name: env_name.to_string(),
            environment: env.clone(),
            dry_run,
            verbose,
            deploy_mode,
        };

        // All strategies delegate directly to the provider
        // The provider handles the specifics based on deployment_type
        self.provider.deploy(&ctx).await
    }

    pub async fn rollback(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        verbose: bool,
    ) -> anyhow::Result<DeploymentResult> {
        let ctx = DeploymentContext {
            environment_name: env_name.to_string(),
            environment: env.clone(),
            dry_run: false,
            verbose,
            deploy_mode: DeployMode::Full,
        };

        self.provider.rollback(&ctx).await
    }

    pub async fn status(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        verbose: bool,
    ) -> anyhow::Result<String> {
        let ctx = DeploymentContext {
            environment_name: env_name.to_string(),
            environment: env.clone(),
            dry_run: false,
            verbose,
            deploy_mode: DeployMode::Full,
        };

        self.provider.status(&ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_factory_create() {
        let strategy = StrategyFactory::create(&DeploymentType::RollingUpdate);
        assert_eq!(strategy.deployment_type(), DeploymentType::RollingUpdate);

        let strategy = StrategyFactory::create(&DeploymentType::AllIn);
        assert_eq!(strategy.deployment_type(), DeploymentType::RollingUpdate); // AllIn uses rolling internally
    }

    #[test]
    fn test_strategy_factory_create_config() {
        use std::collections::HashMap;

        let env = EnvironmentConfig {
            infrastructure: "test".to_string(),
            deployment_type: "rolling-update".to_string(),
            image: Some("test:latest".to_string()),
            replicas: Some(3),
            config: HashMap::new(),
            env: HashMap::new(),
            environment: HashMap::new(),
            hooks: None,
            resources: None,
        };

        let config = StrategyFactory::create_config(&env);
        assert_eq!(config.deployment_type, DeploymentType::RollingUpdate);
        assert_eq!(config.batch_size, Some(25)); // default

        // Test with custom batch size
        let mut custom_config = HashMap::new();
        custom_config.insert(
            "batch_size".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(50)),
        );

        let env_custom = EnvironmentConfig {
            config: custom_config,
            ..env
        };

        let config = StrategyFactory::create_config(&env_custom);
        assert_eq!(config.batch_size, Some(50));
    }

    #[test]
    fn test_all_in_strategy_config() {
        use std::collections::HashMap;

        let env = EnvironmentConfig {
            infrastructure: "test".to_string(),
            deployment_type: "all-in".to_string(),
            image: Some("test:latest".to_string()),
            replicas: Some(3),
            config: HashMap::new(),
            env: HashMap::new(),
            environment: HashMap::new(),
            hooks: None,
            resources: None,
        };

        let config = StrategyFactory::create_config(&env);
        assert_eq!(config.deployment_type, DeploymentType::AllIn);
        assert_eq!(config.batch_size, Some(100)); // 100% for all-in
    }
}
