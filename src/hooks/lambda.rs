//! Lambda invocation hook executor.

use async_trait::async_trait;
use aws_sdk_lambda::primitives::Blob;
use aws_sdk_lambda::types::InvocationType;
use aws_sdk_lambda::Client;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::error::{InfrastructureError, Result};

use super::executor::{HookContext, HookExecutor, HookResult};
use super::types::{HookConfig, HookType, LambdaHookConfig};

/// Lambda invocation hook executor.
pub struct LambdaHookExecutor {
    client: Option<Client>,
}

impl LambdaHookExecutor {
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
                .region(aws_sdk_lambda::config::Region::new(region.to_string()))
                .load()
                .await
        } else {
            config_builder.load().await
        };

        Ok(Client::new(&config))
    }

    fn get_lambda_config(hook: &HookConfig) -> Option<&LambdaHookConfig> {
        hook.config.lambda.as_ref()
    }

    fn determine_function_name(
        &self,
        config: &LambdaHookConfig,
        context: &HookContext,
    ) -> Option<String> {
        if config.invoke_self {
            // Use the deployed function name from context
            context.lambda_function.clone()
        } else {
            config.function_arn.clone()
        }
    }

    async fn invoke_lambda(
        &self,
        config: &LambdaHookConfig,
        hook_name: &str,
        context: &HookContext,
    ) -> Result<HookResult> {
        let start = Instant::now();

        let function_name = self.determine_function_name(config, context).ok_or_else(|| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook_name.to_string(),
                message: "Lambda function ARN not specified and invoke_self requires function context".to_string(),
            })
        })?;

        let region = config.region.as_deref().or(context.aws_region.as_deref());
        let client = self.get_client(region).await?;

        let invocation_type = match config.invocation_type.to_uppercase().as_str() {
            "EVENT" => InvocationType::Event,
            "DRYRUN" | "DRY_RUN" => InvocationType::DryRun,
            _ => InvocationType::RequestResponse,
        };

        debug!(
            "Invoking Lambda function '{}' (type: {:?})",
            function_name, invocation_type
        );

        let mut invoke = client
            .invoke()
            .function_name(&function_name)
            .invocation_type(invocation_type.clone());

        // Add payload if present
        if let Some(ref payload) = config.payload {
            invoke = invoke.payload(Blob::new(payload.as_bytes().to_vec()));
        }

        // Add qualifier if present
        if let Some(ref qualifier) = config.qualifier {
            invoke = invoke.qualifier(qualifier);
        }

        let result = invoke.send().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                provider: "Lambda".to_string(),
                message: format!("Failed to invoke Lambda function: {}", e),
            })
        })?;

        let duration = start.elapsed();

        // Check for function error
        if let Some(function_error) = result.function_error() {
            let error_msg = result
                .payload()
                .map(|p| String::from_utf8_lossy(p.as_ref()).to_string())
                .unwrap_or_else(|| function_error.to_string());

            warn!(
                "Lambda hook '{}' function returned error: {}",
                hook_name, error_msg
            );

            return Ok(HookResult::failure(
                hook_name,
                format!("Lambda function error: {}", error_msg),
                duration,
            )
            .with_exit_code(1));
        }

        // Get status code
        let status_code = result.status_code();

        // Get response payload
        let response_payload = result
            .payload()
            .map(|p| String::from_utf8_lossy(p.as_ref()).to_string())
            .unwrap_or_default();

        if (200..300).contains(&status_code) || invocation_type == InvocationType::Event {
            info!(
                "Lambda hook '{}' completed successfully (status: {})",
                hook_name, status_code
            );
            Ok(HookResult::success(hook_name, response_payload, duration)
                .with_exit_code(status_code))
        } else {
            Ok(HookResult::failure(
                hook_name,
                format!(
                    "Lambda returned status {}: {}",
                    status_code, response_payload
                ),
                duration,
            )
            .with_exit_code(status_code))
        }
    }
}

impl Default for LambdaHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookExecutor for LambdaHookExecutor {
    fn hook_type(&self) -> HookType {
        HookType::Lambda
    }

    async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult> {
        let config = Self::get_lambda_config(hook).ok_or_else(|| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: "Lambda hook configuration not found".to_string(),
            })
        })?;

        // Validate that we have either function_arn or invoke_self with context
        if !config.invoke_self && config.function_arn.is_none() {
            return Err(crate::error::Error::Deployment(
                crate::error::DeploymentError::HookFailed {
                    hook_name: hook.name.clone(),
                    message: "Either function_arn or invoke_self must be specified".to_string(),
                },
            ));
        }

        self.invoke_lambda(config, &hook.name, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::types::HookTypeConfig;

    #[test]
    fn test_lambda_executor_hook_type() {
        let executor = LambdaHookExecutor::new();
        assert_eq!(executor.hook_type(), HookType::Lambda);
    }

    #[tokio::test]
    async fn test_lambda_hook_missing_config() {
        let executor = LambdaHookExecutor::new();
        let hook = HookConfig {
            name: "test".to_string(),
            hook_type: HookType::Lambda,
            config: HookTypeConfig::default(),
            timeout_secs: 60,
            fail_on_error: true,
            description: None,
        };
        let context = HookContext::default();

        let result = executor.execute(&hook, &context).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_determine_function_name_from_arn() {
        let executor = LambdaHookExecutor::new();
        let config = LambdaHookConfig {
            function_arn: Some("arn:aws:lambda:us-east-1:123456789:function:test".to_string()),
            invoke_self: false,
            ..Default::default()
        };
        let context = HookContext::default();

        let name = executor.determine_function_name(&config, &context);
        assert_eq!(
            name,
            Some("arn:aws:lambda:us-east-1:123456789:function:test".to_string())
        );
    }

    #[test]
    fn test_determine_function_name_invoke_self() {
        let executor = LambdaHookExecutor::new();
        let config = LambdaHookConfig {
            function_arn: None,
            invoke_self: true,
            ..Default::default()
        };
        let context = HookContext::new("prod", "aws-lambda")
            .with_lambda_function("my-deployed-function");

        let name = executor.determine_function_name(&config, &context);
        assert_eq!(name, Some("my-deployed-function".to_string()));
    }
}
