use std::sync::Arc;

use super::checkpoint::CheckpointManager;
use super::rolling::RollingUpdateStrategy;
use super::strategy::{DeploymentResult, DeploymentStrategy, DeploymentType, StrategyConfig};
use crate::config::EnvironmentConfig;
use crate::hooks::executor::HookContext;
use crate::hooks::HookRunner;
use crate::infrastructure::{DeployMode, DeploymentContext, InfrastructureProvider};
use crate::storage::{DeploymentCheckpoint, DeploymentPhase};

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

/// Configuration for hook execution during deployment.
#[derive(Debug, Clone, Default)]
pub struct HookExecutionConfig {
    /// Skip all hooks.
    pub skip_all: bool,
    /// Skip only pre-deploy hooks.
    pub skip_pre: bool,
    /// Skip only post-deploy hooks.
    pub skip_post: bool,
}

impl HookExecutionConfig {
    pub fn skip_none() -> Self {
        Self::default()
    }

    pub fn skip_all_hooks() -> Self {
        Self { skip_all: true, skip_pre: false, skip_post: false }
    }

    pub fn should_run_pre(&self) -> bool {
        !self.skip_all && !self.skip_pre
    }

    pub fn should_run_post(&self) -> bool {
        !self.skip_all && !self.skip_post
    }
}

/// Executes deployments using the appropriate strategy
pub struct DeploymentExecutor {
    provider: Arc<dyn InfrastructureProvider>,
    checkpoint_manager: Option<CheckpointManager>,
    hook_runner: Option<HookRunner>,
}

impl DeploymentExecutor {
    pub fn new(provider: Arc<dyn InfrastructureProvider>) -> Self {
        Self {
            provider,
            checkpoint_manager: None,
            hook_runner: None,
        }
    }

    /// Enable checkpoint support for resumable deployments.
    pub fn with_checkpoint_manager(mut self, manager: CheckpointManager) -> Self {
        self.checkpoint_manager = Some(manager);
        self
    }

    /// Enable hook execution during deployment.
    pub fn with_hook_runner(mut self, runner: HookRunner) -> Self {
        self.hook_runner = Some(runner);
        self
    }

    pub async fn execute(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        dry_run: bool,
        verbose: bool,
        deploy_mode: DeployMode,
    ) -> anyhow::Result<DeploymentResult> {
        self.execute_with_hooks(
            env_name,
            env,
            dry_run,
            verbose,
            deploy_mode,
            &HookExecutionConfig::default(),
        ).await
    }

    /// Execute deployment with hook configuration.
    pub async fn execute_with_hooks(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        dry_run: bool,
        verbose: bool,
        deploy_mode: DeployMode,
        hook_config: &HookExecutionConfig,
    ) -> anyhow::Result<DeploymentResult> {
        let deployment_id = generate_deployment_id(env_name);

        // Create checkpoint if manager is available
        let mut checkpoint = if let Some(manager) = &self.checkpoint_manager {
            let context_json = serde_json::json!({
                "environment": env_name,
                "deploy_mode": deploy_mode,
                "dry_run": dry_run,
            }).to_string();

            Some(manager.create_with_context(&deployment_id, &context_json).await?)
        } else {
            None
        };

        let result = self.execute_phased(
            env_name,
            env,
            dry_run,
            verbose,
            deploy_mode,
            hook_config,
            &mut checkpoint,
        ).await;

        // Clear checkpoint on success, save error on failure
        if let Some(manager) = &self.checkpoint_manager {
            match &result {
                Ok(_) => {
                    manager.clear_checkpoint(&deployment_id).await?;
                }
                Err(e) => {
                    if let Some(cp) = checkpoint.as_mut() {
                        manager.set_error(cp, &e.to_string()).await?;
                    }
                }
            }
        }

        result
    }

    /// Resume an interrupted deployment from checkpoint.
    pub async fn resume_from_checkpoint(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        env: &EnvironmentConfig,
        hook_config: &HookExecutionConfig,
    ) -> anyhow::Result<DeploymentResult> {
        tracing::info!(
            "Resuming deployment {} from phase {}",
            checkpoint.deployment_id,
            checkpoint.phase
        );

        let dry_run = false;
        let verbose = true;

        // Parse context from checkpoint if available
        let (deploy_mode, env_name) = if let Some(ctx) = &checkpoint.context_snapshot {
            let parsed: serde_json::Value = serde_json::from_str(ctx).unwrap_or_default();
            let mode = parsed.get("deploy_mode")
                .and_then(|v| v.as_str())
                .map(DeployMode::from_str)
                .unwrap_or(DeployMode::Full);
            let name = parsed.get("environment")
                .and_then(|v| v.as_str())
                .unwrap_or(&checkpoint.deployment_id)
                .to_string();
            (mode, name)
        } else {
            (DeployMode::Full, checkpoint.deployment_id.clone())
        };

        // Wrap checkpoint in Option for the phased executor
        let mut checkpoint_opt = Some(std::mem::replace(
            checkpoint,
            DeploymentCheckpoint::new(""),
        ));

        let result = self.execute_phased(
            &env_name,
            env,
            dry_run,
            verbose,
            deploy_mode,
            hook_config,
            &mut checkpoint_opt,
        ).await;

        // Restore the checkpoint
        if let Some(cp) = checkpoint_opt {
            *checkpoint = cp;
        }

        result
    }

    async fn execute_phased(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        dry_run: bool,
        verbose: bool,
        deploy_mode: DeployMode,
        hook_config: &HookExecutionConfig,
        checkpoint: &mut Option<DeploymentCheckpoint>,
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

        // Determine starting phase (for resume support)
        let starting_phase = checkpoint
            .as_ref()
            .map(|c| c.phase.clone())
            .unwrap_or(DeploymentPhase::PreHooks);

        // Phase 1: Pre-deploy hooks
        if starting_phase == DeploymentPhase::PreHooks {
            self.run_phase_pre_hooks(env_name, env, hook_config, dry_run, checkpoint).await?;
            self.advance_checkpoint(checkpoint, DeploymentPhase::AppDeployment).await?;
        }

        // Phase 2: App Deployment (main deployment)
        if starting_phase.ordinal() <= DeploymentPhase::AppDeployment.ordinal() {
            self.check_interrupt(checkpoint.as_ref()).await?;

            let result = self.provider.deploy(&ctx).await?;

            if !result.success {
                return Ok(result);
            }

            self.advance_checkpoint(checkpoint, DeploymentPhase::HealthCheck).await?;
        }

        // Phase 3: Health Check (often handled by provider, but we mark the phase)
        if starting_phase.ordinal() <= DeploymentPhase::HealthCheck.ordinal() {
            self.check_interrupt(checkpoint.as_ref()).await?;
            self.advance_checkpoint(checkpoint, DeploymentPhase::PostHooks).await?;
        }

        // Phase 4: Post-deploy hooks
        if starting_phase.ordinal() <= DeploymentPhase::PostHooks.ordinal() {
            self.run_phase_post_hooks(env_name, env, hook_config, dry_run, checkpoint).await?;
            self.advance_checkpoint(checkpoint, DeploymentPhase::Completed).await?;
        }

        Ok(DeploymentResult {
            success: true,
            message: format!("Deployment to '{}' completed successfully", env_name),
            version: env.image.clone(),
            rollback_version: None,
        })
    }

    async fn run_phase_pre_hooks(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        hook_config: &HookExecutionConfig,
        dry_run: bool,
        checkpoint: &mut Option<DeploymentCheckpoint>,
    ) -> anyhow::Result<()> {
        if !hook_config.should_run_pre() {
            return Ok(());
        }

        let hooks_config = match &env.hooks {
            Some(h) if !h.pre_deploy.is_empty() => h,
            _ => return Ok(()),
        };

        let runner = match &self.hook_runner {
            Some(r) => r,
            None => return Ok(()),
        };

        let hook_ctx = HookContext::new(env_name, &env.infrastructure)
            .with_dry_run(dry_run);

        // Get already completed hooks from checkpoint
        let completed: Vec<String> = checkpoint
            .as_ref()
            .map(|c| c.pre_hooks_completed.clone())
            .unwrap_or_default();

        for hook in &hooks_config.pre_deploy {
            if completed.contains(&hook.name) {
                tracing::info!("Skipping already completed pre-hook: {}", hook.name);
                continue;
            }

            self.check_interrupt(checkpoint.as_ref()).await?;

            tracing::info!("Running pre-deploy hook: {}", hook.name);
            let result = runner.run_single_hook(hook, &hook_ctx).await?;

            if !result.success && hook.fail_on_error {
                return Err(anyhow::anyhow!(
                    "Pre-deploy hook '{}' failed: {}",
                    hook.name,
                    result.error.unwrap_or_default()
                ));
            }

            // Mark hook as completed in checkpoint
            if let (Some(manager), Some(cp)) = (&self.checkpoint_manager, checkpoint.as_mut()) {
                manager.mark_pre_hook_completed(cp, &hook.name).await?;
            }
        }

        Ok(())
    }

    async fn run_phase_post_hooks(
        &self,
        env_name: &str,
        env: &EnvironmentConfig,
        hook_config: &HookExecutionConfig,
        dry_run: bool,
        checkpoint: &mut Option<DeploymentCheckpoint>,
    ) -> anyhow::Result<()> {
        if !hook_config.should_run_post() {
            return Ok(());
        }

        let hooks_config = match &env.hooks {
            Some(h) if !h.post_deploy.is_empty() => h,
            _ => return Ok(()),
        };

        let runner = match &self.hook_runner {
            Some(r) => r,
            None => return Ok(()),
        };

        let hook_ctx = HookContext::new(env_name, &env.infrastructure)
            .with_dry_run(dry_run);

        // Get already completed hooks from checkpoint
        let completed: Vec<String> = checkpoint
            .as_ref()
            .map(|c| c.post_hooks_completed.clone())
            .unwrap_or_default();

        for hook in &hooks_config.post_deploy {
            if completed.contains(&hook.name) {
                tracing::info!("Skipping already completed post-hook: {}", hook.name);
                continue;
            }

            self.check_interrupt(checkpoint.as_ref()).await?;

            tracing::info!("Running post-deploy hook: {}", hook.name);
            let result = runner.run_single_hook(hook, &hook_ctx).await?;

            if !result.success && hook.fail_on_error {
                return Err(anyhow::anyhow!(
                    "Post-deploy hook '{}' failed: {}",
                    hook.name,
                    result.error.unwrap_or_default()
                ));
            }

            // Mark hook as completed in checkpoint
            if let (Some(manager), Some(cp)) = (&self.checkpoint_manager, checkpoint.as_mut()) {
                manager.mark_post_hook_completed(cp, &hook.name).await?;
            }
        }

        Ok(())
    }

    async fn advance_checkpoint(
        &self,
        checkpoint: &mut Option<DeploymentCheckpoint>,
        phase: DeploymentPhase,
    ) -> anyhow::Result<()> {
        if let (Some(manager), Some(cp)) = (&self.checkpoint_manager, checkpoint.as_mut()) {
            manager.set_phase(cp, phase).await?;
        }

        Ok(())
    }

    async fn check_interrupt(
        &self,
        checkpoint: Option<&DeploymentCheckpoint>,
    ) -> anyhow::Result<()> {
        if let (Some(manager), Some(cp)) = (&self.checkpoint_manager, checkpoint) {
            manager.check_interrupt(cp).await?;
        }

        Ok(())
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

fn generate_deployment_id(env_name: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    format!("dep_{}_{}", env_name, timestamp)
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
