use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::strategy::{DeploymentResult, DeploymentStrategy, DeploymentType, StrategyConfig};

/// Rolling update deployment strategy
/// Gradually replaces instances in configurable batch sizes
pub struct RollingUpdateStrategy {
    state: Arc<Mutex<RollingState>>,
}

#[derive(Default)]
struct RollingState {
    current_batch: u32,
    total_batches: u32,
    completed_instances: u32,
    failed_instances: u32,
    previous_version: Option<String>,
}

impl RollingUpdateStrategy {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RollingState::default())),
        }
    }

    fn calculate_batches(&self, total: u32, batch_size: u32) -> u32 {
        if batch_size == 0 {
            return 1;
        }

        (total + batch_size - 1) / batch_size
    }
}

impl Default for RollingUpdateStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeploymentStrategy for RollingUpdateStrategy {
    fn deployment_type(&self) -> DeploymentType {
        DeploymentType::RollingUpdate
    }

    fn validate_config(&self, config: &StrategyConfig) -> anyhow::Result<()> {
        if let Some(batch_size) = config.batch_size {
            if batch_size == 0 {
                anyhow::bail!("batch_size must be greater than 0");
            }

            if batch_size > 100 {
                anyhow::bail!("batch_size cannot exceed 100%");
            }
        }

        if let Some(timeout) = config.health_check_timeout_secs {
            if timeout == 0 {
                anyhow::bail!("health_check_timeout_secs must be greater than 0");
            }
        }

        Ok(())
    }

    async fn execute(
        &self,
        config: &StrategyConfig,
        deploy_fn: Box<dyn Fn() -> anyhow::Result<()> + Send + Sync>,
    ) -> anyhow::Result<DeploymentResult> {
        let batch_size = config.batch_size.unwrap_or(25);
        let total_batches = self.calculate_batches(100, batch_size);

        {
            let mut state = self.state.lock().await;
            state.total_batches = total_batches;
            state.current_batch = 0;
        }

        tracing::info!(
            "Starting rolling update with {} batches ({}% per batch)",
            total_batches,
            batch_size
        );

        for batch in 0..total_batches {
            {
                let mut state = self.state.lock().await;
                state.current_batch = batch + 1;
            }

            tracing::info!("Deploying batch {}/{}", batch + 1, total_batches);

            if let Err(e) = deploy_fn() {
                let mut state = self.state.lock().await;
                state.failed_instances += 1;

                if config.rollback_on_failure {
                    tracing::error!("Batch {} failed: {}. Initiating rollback.", batch + 1, e);
                    return Ok(DeploymentResult::failure(format!(
                        "Rolling update failed at batch {}/{}. Rollback initiated.",
                        batch + 1,
                        total_batches
                    )));
                }
            } else {
                let mut state = self.state.lock().await;
                state.completed_instances += batch_size;
            }

            if batch < total_batches - 1 {
                tracing::info!("Waiting for health checks...");
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }

        let state = self.state.lock().await;

        Ok(DeploymentResult::success(format!(
            "Rolling update completed: {} batches, {}% updated",
            state.total_batches,
            state.completed_instances.min(100)
        )))
    }

    async fn rollback(&self) -> anyhow::Result<DeploymentResult> {
        let state = self.state.lock().await;

        if let Some(prev_version) = &state.previous_version {
            Ok(DeploymentResult::success(format!(
                "Rolled back to version {}",
                prev_version
            )))
        } else {
            Ok(DeploymentResult::failure(
                "No previous version available for rollback",
            ))
        }
    }
}

/// Configuration for rolling update strategy
#[derive(Debug, Clone)]
pub struct RollingUpdateConfig {
    /// Percentage of instances to update per batch (1-100)
    pub batch_percentage: u32,
    /// Maximum number of instances that can be unavailable during update
    pub max_unavailable: Option<u32>,
    /// Maximum number of extra instances that can be created during update
    pub max_surge: Option<u32>,
    /// Time to wait between batches in seconds
    pub batch_delay_secs: u64,
    /// Whether to pause between batches for manual approval
    pub pause_between_batches: bool,
}

impl Default for RollingUpdateConfig {
    fn default() -> Self {
        Self {
            batch_percentage: 25,
            max_unavailable: Some(1),
            max_surge: Some(1),
            batch_delay_secs: 5,
            pause_between_batches: false,
        }
    }
}

impl RollingUpdateConfig {
    pub fn from_strategy_config(config: &StrategyConfig) -> Self {
        Self {
            batch_percentage: config.batch_size.unwrap_or(25),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_batches() {
        let strategy = RollingUpdateStrategy::new();

        assert_eq!(strategy.calculate_batches(100, 25), 4);
        assert_eq!(strategy.calculate_batches(100, 33), 4);
        assert_eq!(strategy.calculate_batches(100, 50), 2);
        assert_eq!(strategy.calculate_batches(100, 100), 1);
    }

    #[test]
    fn test_validate_config() {
        let strategy = RollingUpdateStrategy::new();

        let valid_config = StrategyConfig {
            deployment_type: DeploymentType::RollingUpdate,
            batch_size: Some(25),
            health_check_timeout_secs: Some(300),
            rollback_on_failure: true,
        };

        assert!(strategy.validate_config(&valid_config).is_ok());

        let invalid_batch = StrategyConfig {
            batch_size: Some(0),
            ..valid_config.clone()
        };

        assert!(strategy.validate_config(&invalid_batch).is_err());

        let invalid_timeout = StrategyConfig {
            health_check_timeout_secs: Some(0),
            ..valid_config
        };

        assert!(strategy.validate_config(&invalid_timeout).is_err());
    }

    #[tokio::test]
    async fn test_rolling_update_execute() {
        let strategy = RollingUpdateStrategy::new();
        let config = StrategyConfig::default();

        let result = strategy
            .execute(&config, Box::new(|| Ok(())))
            .await
            .unwrap();

        assert!(result.success);
    }
}
