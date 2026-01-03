//! Hook execution engine.

use async_trait::async_trait;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

use crate::error::{DeploymentError, Result};
use crate::infrastructure::DeploymentContext;

use super::types::{HookConfig, HookTiming, HookType, HooksConfig};

/// Result of a hook execution.
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Name of the hook.
    pub name: String,
    /// Whether the hook succeeded.
    pub success: bool,
    /// Exit code (if applicable).
    pub exit_code: Option<i32>,
    /// Output/logs from the hook.
    pub output: String,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Duration of execution.
    pub duration: Duration,
}

impl HookResult {
    pub fn success(name: &str, output: String, duration: Duration) -> Self {
        Self {
            name: name.to_string(),
            success: true,
            exit_code: Some(0),
            output,
            error: None,
            duration,
        }
    }

    pub fn failure(name: &str, error: String, duration: Duration) -> Self {
        Self {
            name: name.to_string(),
            success: false,
            exit_code: None,
            output: String::new(),
            error: Some(error),
            duration,
        }
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn with_output(mut self, output: String) -> Self {
        self.output = output;
        self
    }
}

/// Trait for hook executors.
#[async_trait]
pub trait HookExecutor: Send + Sync {
    /// Get the hook type this executor handles.
    fn hook_type(&self) -> HookType;

    /// Execute a hook.
    async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult>;
}

/// Context for hook execution.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Environment name.
    pub environment: String,
    /// Infrastructure type.
    pub infrastructure_type: String,
    /// Current image being deployed (if known).
    pub image: Option<String>,
    /// Previous image (for rollback hooks).
    pub previous_image: Option<String>,
    /// Deployment context from the provider.
    pub deployment_context: Option<DeploymentContext>,
    /// AWS region (for AWS hooks).
    pub aws_region: Option<String>,
    /// Kubernetes namespace (for K8s hooks).
    pub k8s_namespace: Option<String>,
    /// ECS cluster name.
    pub ecs_cluster: Option<String>,
    /// Lambda function name.
    pub lambda_function: Option<String>,
    /// Whether this is a dry-run.
    pub dry_run: bool,
}

impl Default for HookContext {
    fn default() -> Self {
        Self {
            environment: String::new(),
            infrastructure_type: String::new(),
            image: None,
            previous_image: None,
            deployment_context: None,
            aws_region: None,
            k8s_namespace: None,
            ecs_cluster: None,
            lambda_function: None,
            dry_run: false,
        }
    }
}

impl HookContext {
    pub fn new(environment: &str, infrastructure_type: &str) -> Self {
        Self {
            environment: environment.to_string(),
            infrastructure_type: infrastructure_type.to_string(),
            ..Default::default()
        }
    }

    pub fn with_image(mut self, image: &str) -> Self {
        self.image = Some(image.to_string());
        self
    }

    pub fn with_deployment_context(mut self, ctx: DeploymentContext) -> Self {
        self.deployment_context = Some(ctx);
        self
    }

    pub fn with_aws_region(mut self, region: &str) -> Self {
        self.aws_region = Some(region.to_string());
        self
    }

    pub fn with_k8s_namespace(mut self, namespace: &str) -> Self {
        self.k8s_namespace = Some(namespace.to_string());
        self
    }

    pub fn with_ecs_cluster(mut self, cluster: &str) -> Self {
        self.ecs_cluster = Some(cluster.to_string());
        self
    }

    pub fn with_lambda_function(mut self, function: &str) -> Self {
        self.lambda_function = Some(function.to_string());
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

/// Hook runner that orchestrates hook execution.
pub struct HookRunner {
    executors: Vec<Box<dyn HookExecutor>>,
}

impl HookRunner {
    pub fn new() -> Self {
        Self {
            executors: Vec::new(),
        }
    }

    /// Register a hook executor.
    pub fn register<E: HookExecutor + 'static>(&mut self, executor: E) {
        self.executors.push(Box::new(executor));
    }

    /// Find executor for a hook type.
    fn find_executor(&self, hook_type: &HookType) -> Option<&dyn HookExecutor> {
        self.executors
            .iter()
            .find(|e| &e.hook_type() == hook_type)
            .map(|e| e.as_ref())
    }

    /// Execute all hooks for a given timing.
    pub async fn run_hooks(
        &self,
        hooks_config: &HooksConfig,
        timing: HookTiming,
        context: &HookContext,
    ) -> Result<Vec<HookResult>> {
        let hooks = hooks_config.get_hooks(timing);

        if hooks.is_empty() {
            return Ok(Vec::new());
        }

        info!(
            "Running {} {} hooks",
            hooks.len(),
            timing.as_str()
        );

        let mut results = Vec::new();

        for hook in hooks {
            let result = self.run_single_hook(hook, context).await?;
            let should_fail = !result.success && hook.fail_on_error;

            results.push(result.clone());

            if should_fail {
                return Err(crate::error::Error::Deployment(
                    DeploymentError::HookFailed {
                        hook_name: hook.name.clone(),
                        message: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                    },
                ));
            }
        }

        Ok(results)
    }

    /// Execute a single hook with timeout.
    pub async fn run_single_hook(
        &self,
        hook: &HookConfig,
        context: &HookContext,
    ) -> Result<HookResult> {
        let executor = self.find_executor(&hook.hook_type).ok_or_else(|| {
            crate::error::Error::Deployment(DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: format!("No executor found for hook type: {}", hook.hook_type.as_str()),
            })
        })?;

        info!(
            "Executing hook '{}' (type: {})",
            hook.name,
            hook.hook_type.as_str()
        );

        if context.dry_run {
            info!("Dry-run: Would execute hook '{}'", hook.name);
            return Ok(HookResult::success(
                &hook.name,
                "Dry-run: skipped".to_string(),
                Duration::ZERO,
            ));
        }

        let timeout_duration = Duration::from_secs(hook.timeout_secs as u64);
        let start = std::time::Instant::now();

        match timeout(timeout_duration, executor.execute(hook, context)).await {
            Ok(Ok(result)) => {
                if result.success {
                    info!(
                        "Hook '{}' completed successfully in {:?}",
                        hook.name, result.duration
                    );
                } else {
                    warn!(
                        "Hook '{}' failed: {}",
                        hook.name,
                        result.error.as_deref().unwrap_or("Unknown error")
                    );
                }
                Ok(result)
            }
            Ok(Err(e)) => {
                let duration = start.elapsed();
                warn!("Hook '{}' failed with error: {}", hook.name, e);
                Ok(HookResult::failure(&hook.name, e.to_string(), duration))
            }
            Err(_) => {
                let duration = start.elapsed();
                warn!(
                    "Hook '{}' timed out after {:?}",
                    hook.name, timeout_duration
                );
                Ok(HookResult::failure(
                    &hook.name,
                    format!("Timeout after {} seconds", hook.timeout_secs),
                    duration,
                ))
            }
        }
    }

    /// Run a specific hook by name.
    pub async fn run_hook_by_name(
        &self,
        hooks_config: &HooksConfig,
        hook_name: &str,
        context: &HookContext,
    ) -> Result<HookResult> {
        // Search all timings for the hook
        let hook = [
            &hooks_config.pre_deploy,
            &hooks_config.post_deploy,
            &hooks_config.on_failure,
            &hooks_config.on_success,
        ]
        .iter()
        .flat_map(|hooks| hooks.iter())
        .find(|h| h.name == hook_name)
        .ok_or_else(|| {
            crate::error::Error::Deployment(DeploymentError::HookFailed {
                hook_name: hook_name.to_string(),
                message: format!("Hook '{}' not found", hook_name),
            })
        })?;

        self.run_single_hook(hook, context).await
    }
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_result_success() {
        let result = HookResult::success("test", "output".to_string(), Duration::from_secs(1));
        assert!(result.success);
        assert_eq!(result.name, "test");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_hook_result_failure() {
        let result = HookResult::failure("test", "error msg".to_string(), Duration::from_secs(1));
        assert!(!result.success);
        assert_eq!(result.name, "test");
        assert_eq!(result.error, Some("error msg".to_string()));
    }

    #[test]
    fn test_hook_context_builder() {
        let ctx = HookContext::new("prod", "aws-ecs")
            .with_image("myapp:v1")
            .with_aws_region("us-east-1")
            .with_ecs_cluster("my-cluster")
            .with_dry_run(true);

        assert_eq!(ctx.environment, "prod");
        assert_eq!(ctx.infrastructure_type, "aws-ecs");
        assert_eq!(ctx.image, Some("myapp:v1".to_string()));
        assert_eq!(ctx.aws_region, Some("us-east-1".to_string()));
        assert_eq!(ctx.ecs_cluster, Some("my-cluster".to_string()));
        assert!(ctx.dry_run);
    }

    #[test]
    fn test_hook_runner_new() {
        let runner = HookRunner::new();
        assert!(runner.executors.is_empty());
    }
}
