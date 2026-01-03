use async_trait::async_trait;
use aws_sdk_lambda::types::{
    Architecture, Environment, EphemeralStorage, PackageType, Runtime, TracingConfig,
    TracingMode, VpcConfig,
};
use aws_sdk_lambda::Client as LambdaClient;
use std::collections::HashMap;

use super::provider::{DeploymentContext, InfrastructureProvider, InfrastructureType};
use super::provisioning::{
    compute_lambda_diff, LambdaCurrentState, LambdaProvisioningConfig, LambdaVpcConfig,
    PlannedChange, ProvisioningPlan, ProvisioningResult,
};
use crate::config::InfrastructureConfig;
use crate::deployment::{DeploymentResult, DeploymentType};

pub struct AwsLambdaProvider {
    lambda_client: LambdaClient,
    function_name: String,
    region: String,
    alias: Option<String>,
    provisioning_config: Option<LambdaProvisioningConfig>,
}

impl AwsLambdaProvider {
    pub async fn new(function_name: &str, region: &str) -> anyhow::Result<Self> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_lambda::config::Region::new(region.to_string()))
            .load()
            .await;

        let lambda_client = LambdaClient::new(&aws_config);

        Ok(Self {
            lambda_client,
            function_name: function_name.to_string(),
            region: region.to_string(),
            alias: None,
            provisioning_config: None,
        })
    }

    pub fn with_alias(mut self, alias: &str) -> Self {
        self.alias = Some(alias.to_string());
        self
    }

    pub fn with_provisioning(mut self, config: LambdaProvisioningConfig) -> Self {
        self.provisioning_config = Some(config);
        self
    }

    pub async fn from_config(config: &InfrastructureConfig) -> anyhow::Result<Self> {
        let function_name = config
            .config
            .get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("function_name is required for aws-lambda"))?;

        let region = config
            .config
            .get("region")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("region is required for aws-lambda"))?;

        let mut provider = Self::new(function_name, region).await?;

        if let Some(alias) = config.config.get("alias").and_then(|v| v.as_str()) {
            provider = provider.with_alias(alias);
        }

        if let Some(provision_value) = config.config.get("provision") {
            if let Some(provision_config) = LambdaProvisioningConfig::from_yaml_value(provision_value) {
                provider.provisioning_config = Some(provision_config);
            }
        }

        Ok(provider)
    }

    async fn get_function_info(&self) -> anyhow::Result<FunctionInfo> {
        let response = self
            .lambda_client
            .get_function()
            .function_name(&self.function_name)
            .send()
            .await?;

        let config = response
            .configuration
            .ok_or_else(|| anyhow::anyhow!("Function has no configuration"))?;

        Ok(FunctionInfo {
            name: config.function_name.unwrap_or_default(),
            runtime: config.runtime.map(|r| r.as_str().to_string()),
            state: config.state.map(|s| s.as_str().to_string()),
            version: config.version,
            code_sha256: config.code_sha256,
            memory_size: config.memory_size,
            timeout: config.timeout,
        })
    }

    async fn update_function_image(&self, image_uri: &str) -> anyhow::Result<String> {
        let response = self
            .lambda_client
            .update_function_code()
            .function_name(&self.function_name)
            .image_uri(image_uri)
            .send()
            .await?;

        let version = response.version.unwrap_or_else(|| "$LATEST".to_string());

        Ok(version)
    }

    async fn update_environment_variables(
        &self,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        if env_vars.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "Updating environment variables ({} variables)",
            env_vars.len()
        );

        let environment = Environment::builder()
            .set_variables(Some(env_vars.clone()))
            .build();

        self.lambda_client
            .update_function_configuration()
            .function_name(&self.function_name)
            .environment(environment)
            .send()
            .await?;

        Ok(())
    }

    async fn publish_version(&self, description: &str) -> anyhow::Result<String> {
        let response = self
            .lambda_client
            .publish_version()
            .function_name(&self.function_name)
            .description(description)
            .send()
            .await?;

        let version = response
            .version
            .ok_or_else(|| anyhow::anyhow!("Failed to get published version"))?;

        Ok(version)
    }

    async fn update_alias(&self, alias: &str, version: &str) -> anyhow::Result<()> {
        let existing = self
            .lambda_client
            .get_alias()
            .function_name(&self.function_name)
            .name(alias)
            .send()
            .await;

        if existing.is_ok() {
            self.lambda_client
                .update_alias()
                .function_name(&self.function_name)
                .name(alias)
                .function_version(version)
                .send()
                .await?;
        } else {
            self.lambda_client
                .create_alias()
                .function_name(&self.function_name)
                .name(alias)
                .function_version(version)
                .send()
                .await?;
        }

        Ok(())
    }

    async fn wait_for_update(&self, timeout_secs: u64) -> anyhow::Result<()> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() > timeout_secs {
                anyhow::bail!("Lambda update timed out after {} seconds", timeout_secs);
            }

            let info = self.get_function_info().await?;

            if let Some(state) = &info.state {
                match state.as_str() {
                    "Active" => return Ok(()),
                    "Failed" => anyhow::bail!("Lambda function update failed"),
                    "Pending" => {
                        tracing::info!("Lambda function state: Pending...");
                    }
                    _ => {}
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    fn get_alias_or_default(&self, ctx: &DeploymentContext) -> Option<String> {
        self.alias.clone().or_else(|| {
            ctx.environment
                .config
                .get("alias")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
    }

    async fn configure_provisioned_concurrency(
        &self,
        version: &str,
        concurrency: i32,
    ) -> anyhow::Result<()> {
        if concurrency <= 0 {
            return Ok(());
        }

        tracing::info!(
            "Configuring provisioned concurrency: {} for version {}",
            concurrency,
            version
        );

        self.lambda_client
            .put_provisioned_concurrency_config()
            .function_name(&self.function_name)
            .qualifier(version)
            .provisioned_concurrent_executions(concurrency)
            .send()
            .await?;

        Ok(())
    }

    async fn configure_reserved_concurrency(&self, concurrency: i32) -> anyhow::Result<()> {
        if concurrency < 0 {
            return Ok(());
        }

        tracing::info!("Configuring reserved concurrency: {}", concurrency);

        self.lambda_client
            .put_function_concurrency()
            .function_name(&self.function_name)
            .reserved_concurrent_executions(concurrency)
            .send()
            .await?;

        Ok(())
    }

    // ========== Provisioning Methods ==========

    /// Check if the Lambda function exists
    pub async fn function_exists(&self) -> anyhow::Result<bool> {
        match self.lambda_client
            .get_function()
            .function_name(&self.function_name)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_resource_not_found_exception() {
                    Ok(false)
                } else {
                    Err(anyhow::anyhow!("Failed to check function: {}", service_error))
                }
            }
        }
    }

    /// Get the current state of the Lambda function for comparison
    pub async fn get_current_state(&self) -> anyhow::Result<LambdaCurrentState> {
        let response = match self.lambda_client
            .get_function()
            .function_name(&self.function_name)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_resource_not_found_exception() {
                    return Ok(LambdaCurrentState::not_found());
                }
                return Err(anyhow::anyhow!("Failed to get function: {}", service_error));
            }
        };

        let config = response.configuration
            .ok_or_else(|| anyhow::anyhow!("Function has no configuration"))?;

        let vpc_config = config.vpc_config.map(|v| LambdaVpcConfig {
            subnet_ids: v.subnet_ids.unwrap_or_default(),
            security_group_ids: v.security_group_ids.unwrap_or_default(),
        });

        let layers: Vec<String> = config.layers
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| l.arn)
            .collect();

        Ok(LambdaCurrentState {
            exists: true,
            runtime: config.runtime.map(|r| r.as_str().to_string()),
            handler: config.handler,
            memory_size: config.memory_size,
            timeout: config.timeout,
            role: config.role,
            description: config.description,
            vpc_config,
            layers,
            architecture: config.architectures
                .and_then(|a| a.first().map(|arch| arch.as_str().to_string())),
            ephemeral_storage: config.ephemeral_storage.map(|e| e.size),
            package_type: config.package_type.map(|p| p.as_str().to_string()),
        })
    }

    /// Plan provisioning changes without applying them
    pub async fn plan_provisioning(&self) -> anyhow::Result<ProvisioningPlan> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        config.validate()?;

        let current = self.get_current_state().await?;
        let diff = compute_lambda_diff(&current, config);

        let mut plan = ProvisioningPlan::new();

        if diff.needs_create {
            plan.add(PlannedChange::create(
                "Lambda Function",
                &self.function_name,
                "Function does not exist",
            ));
        } else {
            for (field, current_val, desired_val) in &diff.changes {
                plan.add(PlannedChange::update(
                    "Lambda Function",
                    &self.function_name,
                    current_val,
                    desired_val,
                    &format!("{} will be changed", field),
                ));
            }

            if diff.vpc_changed {
                plan.add(PlannedChange::update(
                    "Lambda VPC Config",
                    &self.function_name,
                    "current",
                    "desired",
                    "VPC configuration will be changed",
                ));
            }

            if diff.layers_changed {
                plan.add(PlannedChange::update(
                    "Lambda Layers",
                    &self.function_name,
                    &format!("{} layers", current.layers.len()),
                    &format!("{} layers", config.layers.len()),
                    "Layers will be changed",
                ));
            }

            if diff.has_immutable_changes() {
                tracing::warn!(
                    "Architecture change detected. This requires recreating the function."
                );
            }

            if !diff.needs_update() && !diff.has_immutable_changes() {
                plan.add(PlannedChange::no_change("Lambda Function", &self.function_name));
            }
        }

        Ok(plan)
    }

    /// Create a new Lambda function with the provisioning configuration
    pub async fn create_function(&self, image_uri: &str) -> anyhow::Result<ProvisioningResult> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        config.validate()?;

        tracing::info!("Creating Lambda function: {}", self.function_name);

        let architecture = match config.architecture.as_str() {
            "arm64" => Architecture::Arm64,
            _ => Architecture::X8664,
        };

        let is_image_package = config.package_type.as_str() != "Zip";
        let package_type = if is_image_package {
            PackageType::Image
        } else {
            PackageType::Zip
        };

        let mut builder = self.lambda_client
            .create_function()
            .function_name(&self.function_name)
            .role(&config.role_arn)
            .package_type(package_type)
            .memory_size(config.memory_mb)
            .timeout(config.timeout_secs)
            .architectures(architecture);

        // Set code based on package type
        if is_image_package {
            builder = builder.code(
                aws_sdk_lambda::types::FunctionCode::builder()
                    .image_uri(image_uri)
                    .build()
            );
        }

        // Set runtime and handler for Zip packages
        if !is_image_package {
            if let Some(runtime) = &config.runtime {
                builder = builder.runtime(Runtime::from(runtime.as_str()));
            }

            if let Some(handler) = &config.handler {
                builder = builder.handler(handler);
            }
        }

        // Set description
        if let Some(description) = &config.description {
            builder = builder.description(description);
        }

        // Set VPC configuration
        if let Some(vpc) = &config.vpc {
            let vpc_config = VpcConfig::builder()
                .set_subnet_ids(Some(vpc.subnet_ids.clone()))
                .set_security_group_ids(Some(vpc.security_group_ids.clone()))
                .build();
            builder = builder.vpc_config(vpc_config);
        }

        // Set layers
        if !config.layers.is_empty() {
            builder = builder.set_layers(Some(config.layers.clone()));
        }

        // Set ephemeral storage
        if config.ephemeral_storage_mb != 512 {
            let ephemeral = EphemeralStorage::builder()
                .size(config.ephemeral_storage_mb)
                .build()?;
            builder = builder.ephemeral_storage(ephemeral);
        }

        // Set tracing configuration
        if let Some(tracing_mode) = &config.tracing_mode {
            let mode = match tracing_mode.as_str() {
                "Active" => TracingMode::Active,
                _ => TracingMode::PassThrough,
            };
            builder = builder.tracing_config(
                TracingConfig::builder().mode(mode).build()
            );
        }

        // Set dead letter config
        if let Some(dlq) = &config.dead_letter_queue {
            builder = builder.dead_letter_config(
                aws_sdk_lambda::types::DeadLetterConfig::builder()
                    .target_arn(&dlq.target_arn)
                    .build()
            );
        }

        builder.send().await?;

        tracing::info!("Successfully created Lambda function: {}", self.function_name);

        Ok(ProvisioningResult::created(
            "Lambda Function",
            &self.function_name,
            vec![
                format!("Memory: {} MB", config.memory_mb),
                format!("Timeout: {} seconds", config.timeout_secs),
                format!("Architecture: {}", config.architecture),
            ],
        ))
    }

    /// Update an existing Lambda function with the provisioning configuration
    pub async fn update_function_config(&self) -> anyhow::Result<ProvisioningResult> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        config.validate()?;

        let current = self.get_current_state().await?;

        if !current.exists {
            anyhow::bail!("Function does not exist. Use create_function instead.");
        }

        let diff = compute_lambda_diff(&current, config);

        if !diff.needs_update() {
            tracing::info!("No configuration changes needed for Lambda function: {}", self.function_name);
            return Ok(ProvisioningResult::unchanged("Lambda Function", &self.function_name));
        }

        tracing::info!("Updating Lambda function configuration: {}", self.function_name);

        let mut builder = self.lambda_client
            .update_function_configuration()
            .function_name(&self.function_name);

        if diff.memory_changed {
            builder = builder.memory_size(config.memory_mb);
        }

        if diff.timeout_changed {
            builder = builder.timeout(config.timeout_secs);
        }

        if diff.role_changed {
            builder = builder.role(&config.role_arn);
        }

        if diff.description_changed {
            if let Some(desc) = &config.description {
                builder = builder.description(desc);
            }
        }

        if diff.vpc_changed {
            if let Some(vpc) = &config.vpc {
                let vpc_config = VpcConfig::builder()
                    .set_subnet_ids(Some(vpc.subnet_ids.clone()))
                    .set_security_group_ids(Some(vpc.security_group_ids.clone()))
                    .build();
                builder = builder.vpc_config(vpc_config);
            } else {
                // Remove VPC config
                builder = builder.vpc_config(VpcConfig::builder().build());
            }
        }

        if diff.layers_changed {
            builder = builder.set_layers(Some(config.layers.clone()));
        }

        if diff.ephemeral_storage_changed {
            let ephemeral = EphemeralStorage::builder()
                .size(config.ephemeral_storage_mb)
                .build()?;
            builder = builder.ephemeral_storage(ephemeral);
        }

        builder.send().await?;

        let changes: Vec<String> = diff.changes
            .iter()
            .map(|(field, from, to)| format!("{}: {} -> {}", field, from, to))
            .collect();

        tracing::info!(
            "Successfully updated Lambda function configuration: {}",
            self.function_name
        );

        Ok(ProvisioningResult::updated(
            "Lambda Function",
            &self.function_name,
            changes,
        ))
    }

    /// Provision the Lambda function (create if not exists, update if exists)
    pub async fn provision(&self, image_uri: &str) -> anyhow::Result<ProvisioningResult> {
        let exists = self.function_exists().await?;

        if exists {
            // Update existing function configuration
            let config_result = self.update_function_config().await?;

            // Also update the function code
            self.update_function_image(image_uri).await?;

            Ok(config_result)
        } else {
            // Create new function
            self.create_function(image_uri).await
        }
    }
}

struct FunctionInfo {
    name: String,
    runtime: Option<String>,
    state: Option<String>,
    version: Option<String>,
    code_sha256: Option<String>,
    memory_size: Option<i32>,
    timeout: Option<i32>,
}

#[async_trait]
impl InfrastructureProvider for AwsLambdaProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::AwsLambda
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        // Lambda deployments are atomic - the code/image is updated all at once
        vec![DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        _config: &HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        let info = self.get_function_info().await?;

        if let Some(state) = info.state {
            if state != "Active" {
                anyhow::bail!(
                    "Lambda function {} is not active (state: {})",
                    self.function_name,
                    state
                );
            }
        }

        Ok(())
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let image = ctx
            .environment
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No image URI specified for deployment"))?;

        let alias = self.get_alias_or_default(ctx);
        let is_app_only = ctx.deploy_mode.is_app_only();

        if ctx.dry_run {
            let mode_msg = if is_app_only { " (app-only)" } else { "" };
            let msg = if let Some(alias_name) = &alias {
                format!(
                    "Would deploy {} to Lambda function {} (alias: {}){}",
                    image, self.function_name, alias_name, mode_msg
                )
            } else {
                format!(
                    "Would deploy {} to Lambda function {}{}",
                    image, self.function_name, mode_msg
                )
            };
            return Ok(DeploymentResult::success(msg));
        }

        tracing::info!(
            "Deploying {} to Lambda function {} in {} (mode: {:?})",
            image,
            self.function_name,
            self.region,
            ctx.deploy_mode
        );

        // Update the function code (image) - this is the core of any deployment
        self.update_function_image(image).await?;

        let timeout = ctx
            .environment
            .config
            .get("deployment_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);

        self.wait_for_update(timeout).await?;

        // Resolve and update environment variables
        let env_vars = ctx.resolve_env_vars().await?;
        self.update_environment_variables(&env_vars).await?;

        if !env_vars.is_empty() {
            // Wait for configuration update to complete
            self.wait_for_update(timeout).await?;
        }

        let mut deployed_version = "$LATEST".to_string();

        if let Some(alias_name) = &alias {
            let description = format!("Deployed via pmp-deploy: {}", image);
            deployed_version = self.publish_version(&description).await?;

            tracing::info!("Published version: {}", deployed_version);

            self.update_alias(alias_name, &deployed_version).await?;

            tracing::info!("Updated alias {} to version {}", alias_name, deployed_version);
        }

        // Skip infrastructure configuration in app-only mode
        if !is_app_only {
            // Configure provisioned concurrency if specified
            if let Some(provisioned) = ctx
                .environment
                .config
                .get("provisioned_concurrency")
                .and_then(|v| v.as_u64())
            {
                self.configure_provisioned_concurrency(&deployed_version, provisioned as i32)
                    .await?;
            }

            // Configure reserved concurrency if specified
            if let Some(reserved) = ctx
                .environment
                .config
                .get("reserved_concurrency")
                .and_then(|v| v.as_u64())
            {
                self.configure_reserved_concurrency(reserved as i32).await?;
            }
        } else {
            tracing::info!("App-only mode: skipping infrastructure configuration");
        }

        let mode_msg = if is_app_only { " (app-only)" } else { "" };

        Ok(DeploymentResult::success(format!(
            "Deployed {} to Lambda function {} (version: {}){}",
            image, self.function_name, deployed_version, mode_msg
        ))
        .with_version(deployed_version))
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let alias = self.get_alias_or_default(ctx);

        if alias.is_none() {
            anyhow::bail!(
                "Lambda rollback requires an alias. Direct $LATEST deployments cannot be rolled back."
            );
        }

        let alias_name = alias.unwrap();

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would rollback Lambda function {} alias {}",
                self.function_name, alias_name
            )));
        }

        let alias_info = self
            .lambda_client
            .get_alias()
            .function_name(&self.function_name)
            .name(&alias_name)
            .send()
            .await?;

        let current_version = alias_info
            .function_version
            .ok_or_else(|| anyhow::anyhow!("Alias has no function version"))?;

        let current_ver_num: i32 = current_version
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid version number"))?;

        if current_ver_num <= 1 {
            anyhow::bail!("No previous version to rollback to");
        }

        let previous_version = (current_ver_num - 1).to_string();

        tracing::info!(
            "Rolling back alias {} from version {} to {}",
            alias_name,
            current_version,
            previous_version
        );

        self.update_alias(&alias_name, &previous_version).await?;

        Ok(DeploymentResult::success(format!(
            "Rolled back Lambda function {} alias {} to version {}",
            self.function_name, alias_name, previous_version
        )))
    }

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let info = self.get_function_info().await?;
        let alias = self.get_alias_or_default(ctx);

        let mut output = format!(
            "Lambda Function: {}\\n\
             Region: {}\\n\
             State: {}\\n\
             Runtime: {}\\n\
             Memory: {} MB\\n\
             Timeout: {} seconds\\n\
             Version: {}\\n\
             Code SHA256: {}",
            info.name,
            self.region,
            info.state.unwrap_or_else(|| "Unknown".to_string()),
            info.runtime.unwrap_or_else(|| "N/A".to_string()),
            info.memory_size.unwrap_or(0),
            info.timeout.unwrap_or(0),
            info.version.unwrap_or_else(|| "$LATEST".to_string()),
            info.code_sha256
                .map(|s| s[..12].to_string())
                .unwrap_or_else(|| "N/A".to_string())
        );

        if let Some(alias_name) = alias {
            if let Ok(alias_info) = self
                .lambda_client
                .get_alias()
                .function_name(&self.function_name)
                .name(&alias_name)
                .send()
                .await
            {
                output.push_str(&format!(
                    "\\nAlias: {} -> version {}",
                    alias_name,
                    alias_info.function_version.unwrap_or_default()
                ));
            }
        }

        Ok(output)
    }

    async fn logs(&self, _ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        let log_group = format!("/aws/lambda/{}", self.function_name);

        let mut args = vec![
            "logs".to_string(),
            "tail".to_string(),
            log_group,
            "--region".to_string(),
            self.region.clone(),
        ];

        if follow {
            args.push("--follow".to_string());
        }

        let mut child = tokio::process::Command::new("aws")
            .args(&args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        child.wait().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_aws_lambda_provider_config_parsing() {
        use crate::config::InfrastructureConfig;
        use std::collections::HashMap;

        let mut config_map = HashMap::new();
        config_map.insert(
            "function_name".to_string(),
            serde_yaml::Value::String("my-function".to_string()),
        );
        config_map.insert(
            "region".to_string(),
            serde_yaml::Value::String("us-east-1".to_string()),
        );
        config_map.insert(
            "alias".to_string(),
            serde_yaml::Value::String("prod".to_string()),
        );

        let config = InfrastructureConfig {
            infrastructure_type: "aws-lambda".to_string(),
            config: config_map,
        };

        assert!(config.config.contains_key("function_name"));
        assert!(config.config.contains_key("region"));
        assert!(config.config.contains_key("alias"));
    }
}
