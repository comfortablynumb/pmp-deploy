//! Container-based hook executor using Docker.

use async_trait::async_trait;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::Result;

use super::executor::{HookContext, HookExecutor, HookResult};
use super::types::{ContainerHookConfig, HookConfig, HookType};

/// Container hook executor using Docker.
pub struct ContainerHookExecutor;

impl ContainerHookExecutor {
    pub fn new() -> Self {
        Self
    }

    fn get_container_config(hook: &HookConfig) -> Option<&ContainerHookConfig> {
        hook.config.container.as_ref()
    }

    async fn run_docker_container(
        &self,
        config: &ContainerHookConfig,
        hook_name: &str,
        _context: &HookContext,
    ) -> Result<HookResult> {
        let start = Instant::now();

        let mut args = vec!["run".to_string()];

        // Auto-remove container after execution
        if config.cleanup {
            args.push("--rm".to_string());
        }

        // Set working directory
        if let Some(ref working_dir) = config.working_dir {
            args.push("-w".to_string());
            args.push(working_dir.clone());
        }

        // Set network mode
        if let Some(ref network) = config.network {
            args.push("--network".to_string());
            args.push(network.clone());
        }

        // Mount volumes
        for volume in &config.volumes {
            args.push("-v".to_string());
            args.push(volume.clone());
        }

        // Set environment variables
        for (key, env_config) in &config.env {
            // For now, only handle static values
            if let Some(ref value) = env_config.value {
                args.push("-e".to_string());
                args.push(format!("{}={}", key, value));
            }
        }

        // Override entrypoint if specified
        if let Some(ref entrypoint) = config.entrypoint {
            args.push("--entrypoint".to_string());
            args.push(entrypoint.join(" "));
        }

        // Add image
        args.push(config.image.clone());

        // Add command
        args.extend(config.command.clone());

        debug!("Running Docker container: docker {}", args.join(" "));

        let output = Command::new("docker")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                crate::error::Error::Infrastructure(
                    crate::error::InfrastructureError::ProviderError {
                        provider: "Docker".to_string(),
                        message: format!("Failed to run docker: {}", e),
                    },
                )
            })?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let combined_output = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        if output.status.success() {
            info!("Container hook '{}' completed successfully", hook_name);
            Ok(HookResult::success(hook_name, combined_output, duration))
        } else {
            let exit_code = output.status.code().unwrap_or(-1);
            Ok(HookResult::failure(
                hook_name,
                format!("Container exited with code {}: {}", exit_code, combined_output),
                duration,
            )
            .with_exit_code(exit_code))
        }
    }
}

impl Default for ContainerHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookExecutor for ContainerHookExecutor {
    fn hook_type(&self) -> HookType {
        HookType::Container
    }

    async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult> {
        let config = Self::get_container_config(hook).ok_or_else(|| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: "Container hook configuration not found".to_string(),
            })
        })?;

        if config.image.is_empty() {
            return Err(crate::error::Error::Deployment(
                crate::error::DeploymentError::HookFailed {
                    hook_name: hook.name.clone(),
                    message: "Container image is required".to_string(),
                },
            ));
        }

        self.run_docker_container(config, &hook.name, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::types::HookTypeConfig;

    #[test]
    fn test_container_executor_hook_type() {
        let executor = ContainerHookExecutor::new();
        assert_eq!(executor.hook_type(), HookType::Container);
    }

    #[tokio::test]
    async fn test_container_hook_missing_config() {
        let executor = ContainerHookExecutor::new();
        let hook = HookConfig {
            name: "test".to_string(),
            hook_type: HookType::Container,
            config: HookTypeConfig::default(),
            timeout_secs: 60,
            fail_on_error: true,
            description: None,
        };
        let context = HookContext::default();

        let result = executor.execute(&hook, &context).await;
        assert!(result.is_err());
    }
}
