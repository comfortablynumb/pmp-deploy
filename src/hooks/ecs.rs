//! ECS task hook executor for one-shot ECS tasks.

use async_trait::async_trait;
use aws_sdk_ecs::types::{
    AssignPublicIp, AwsVpcConfiguration, ContainerOverride, KeyValuePair, NetworkConfiguration,
    TaskOverride,
};
use aws_sdk_ecs::Client;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::error::{InfrastructureError, Result};

use super::executor::{HookContext, HookExecutor, HookResult};
use super::types::{EcsTaskHookConfig, HookConfig, HookType};

/// ECS task hook executor for running one-shot ECS tasks.
pub struct EcsTaskHookExecutor {
    client: Option<Client>,
}

impl EcsTaskHookExecutor {
    pub fn new() -> Self {
        Self { client: None }
    }

    pub fn with_client(client: Client) -> Self {
        Self {
            client: Some(client),
        }
    }

    async fn get_client(&self, region: Option<&str>) -> Result<Client> {
        if let Some(ref client) = self.client {
            return Ok(client.clone());
        }

        let config_builder = aws_config::defaults(aws_config::BehaviorVersion::latest());

        let config = if let Some(region) = region {
            config_builder
                .region(aws_sdk_ecs::config::Region::new(region.to_string()))
                .load()
                .await
        } else {
            config_builder.load().await
        };

        Ok(Client::new(&config))
    }

    fn get_ecs_config(hook: &HookConfig) -> Option<&EcsTaskHookConfig> {
        hook.config.ecs_task.as_ref()
    }

    async fn run_ecs_task(
        &self,
        config: &EcsTaskHookConfig,
        hook_name: &str,
        context: &HookContext,
        timeout_secs: u32,
    ) -> Result<HookResult> {
        let start = Instant::now();

        let region = config.region.as_deref().or(context.aws_region.as_deref());
        let client = self.get_client(region).await?;

        let cluster = config
            .cluster
            .as_ref()
            .or(context.ecs_cluster.as_ref())
            .ok_or_else(|| {
                crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                    hook_name: hook_name.to_string(),
                    message: "ECS cluster not specified".to_string(),
                })
            })?;

        // Build the task definition name
        // For now, we require a task definition to be registered separately
        // In the future, we could create a task definition on-the-fly
        let task_def_family = format!("{}-hook-{}", cluster, hook_name);

        // Build container overrides
        let mut container_override = ContainerOverride::builder().name("main");

        if !config.command.is_empty() {
            container_override = container_override.set_command(Some(config.command.clone()));
        }

        // Add environment variables
        for (key, env_config) in &config.environment {
            if let Some(ref value) = env_config.value {
                container_override = container_override.environment(
                    KeyValuePair::builder()
                        .name(key)
                        .value(value)
                        .build(),
                );
            }
        }

        let container_override = container_override.build();

        // Build task override
        let mut task_override = TaskOverride::builder().container_overrides(container_override);

        if let Some(ref cpu) = config.cpu {
            task_override = task_override.cpu(cpu);
        }

        if let Some(ref memory) = config.memory {
            task_override = task_override.memory(memory);
        }

        if let Some(ref task_role) = config.task_role_arn {
            task_override = task_override.task_role_arn(task_role);
        }

        if let Some(ref exec_role) = config.execution_role_arn {
            task_override = task_override.execution_role_arn(exec_role);
        }

        let task_override = task_override.build();

        // Build network configuration if subnets are specified
        let network_config = if !config.subnets.is_empty() {
            let mut vpc_config = AwsVpcConfiguration::builder()
                .set_subnets(Some(config.subnets.clone()))
                .set_security_groups(Some(config.security_groups.clone()));

            if config.assign_public_ip {
                vpc_config = vpc_config.assign_public_ip(AssignPublicIp::Enabled);
            } else {
                vpc_config = vpc_config.assign_public_ip(AssignPublicIp::Disabled);
            }

            Some(
                NetworkConfiguration::builder()
                    .awsvpc_configuration(vpc_config.build().map_err(|e| {
                        crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                            provider: "ECS".to_string(),
                            message: format!("Invalid VPC configuration: {}", e),
                        })
                    })?)
                    .build(),
            )
        } else {
            None
        };

        debug!("Running ECS task in cluster '{}' for hook '{}'", cluster, hook_name);

        // Start the task
        let mut run_task = client
            .run_task()
            .cluster(cluster)
            .task_definition(&task_def_family)
            .overrides(task_override)
            .launch_type(config.launch_type.parse().unwrap_or(
                aws_sdk_ecs::types::LaunchType::Fargate,
            ))
            .count(1);

        if let Some(network_config) = network_config {
            run_task = run_task.network_configuration(network_config);
        }

        let run_result = run_task.send().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                provider: "ECS".to_string(),
                message: format!("Failed to run ECS task: {}", e),
            })
        })?;

        // Get the task ARN
        let task_arn = run_result
            .tasks()
            .first()
            .and_then(|t| t.task_arn())
            .ok_or_else(|| {
                crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                    provider: "ECS".to_string(),
                    message: "No task ARN returned".to_string(),
                })
            })?
            .to_string();

        info!("Started ECS task: {}", task_arn);

        // Wait for task completion
        let timeout_duration = Duration::from_secs(timeout_secs as u64);
        let poll_interval = Duration::from_secs(5);

        loop {
            if start.elapsed() > timeout_duration {
                // Stop the task on timeout
                let _ = client
                    .stop_task()
                    .cluster(cluster)
                    .task(&task_arn)
                    .reason("Hook timeout")
                    .send()
                    .await;

                return Ok(HookResult::failure(
                    hook_name,
                    format!("ECS task timed out after {} seconds", timeout_secs),
                    start.elapsed(),
                ));
            }

            let describe_result = client
                .describe_tasks()
                .cluster(cluster)
                .tasks(&task_arn)
                .send()
                .await
                .map_err(|e| {
                    crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                        provider: "ECS".to_string(),
                        message: format!("Failed to describe task: {}", e),
                    })
                })?;

            if let Some(task) = describe_result.tasks().first() {
                let status = task.last_status().unwrap_or("UNKNOWN");
                debug!("Task status: {}", status);

                if status == "STOPPED" {
                    // Check if the task succeeded
                    let stop_code = task.stop_code().map(|c| c.as_str()).unwrap_or("Unknown");
                    let stop_reason = task.stopped_reason().unwrap_or("No reason provided");

                    // Check container exit codes
                    let exit_code = task
                        .containers()
                        .first()
                        .and_then(|c| c.exit_code())
                        .unwrap_or(-1);

                    let duration = start.elapsed();

                    if exit_code == 0 {
                        info!("ECS task '{}' completed successfully", hook_name);
                        return Ok(HookResult::success(
                            hook_name,
                            format!("Task completed: {} - {}", stop_code, stop_reason),
                            duration,
                        )
                        .with_exit_code(exit_code));
                    } else {
                        warn!("ECS task '{}' failed with exit code {}", hook_name, exit_code);
                        return Ok(HookResult::failure(
                            hook_name,
                            format!(
                                "Task failed with exit code {}: {} - {}",
                                exit_code, stop_code, stop_reason
                            ),
                            duration,
                        )
                        .with_exit_code(exit_code));
                    }
                }
            }

            sleep(poll_interval).await;
        }
    }
}

impl Default for EcsTaskHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookExecutor for EcsTaskHookExecutor {
    fn hook_type(&self) -> HookType {
        HookType::EcsTask
    }

    async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult> {
        let config = Self::get_ecs_config(hook).ok_or_else(|| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: "ECS task hook configuration not found".to_string(),
            })
        })?;

        self.run_ecs_task(config, &hook.name, context, hook.timeout_secs)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::types::HookTypeConfig;

    #[test]
    fn test_ecs_executor_hook_type() {
        let executor = EcsTaskHookExecutor::new();
        assert_eq!(executor.hook_type(), HookType::EcsTask);
    }

    #[tokio::test]
    async fn test_ecs_hook_missing_config() {
        let executor = EcsTaskHookExecutor::new();
        let hook = HookConfig {
            name: "test".to_string(),
            hook_type: HookType::EcsTask,
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
