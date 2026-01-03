use async_trait::async_trait;
use aws_sdk_applicationautoscaling::Client as AutoScalingClient;
use aws_sdk_cloudwatchlogs::Client as CloudWatchLogsClient;
use aws_sdk_ecs::types::{
    AssignPublicIp, AwsVpcConfiguration, Compatibility, ContainerDefinition as EcsContainerDef,
    DeploymentCircuitBreaker, DeploymentConfiguration as EcsDeploymentConfig, KeyValuePair,
    LaunchType, NetworkConfiguration, PortMapping as EcsPortMapping, Secret as EcsSecret,
};
use aws_sdk_ecs::Client as EcsClient;
use aws_sdk_elasticloadbalancingv2::Client as ElbClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::provider::{DeploymentContext, InfrastructureProvider, InfrastructureType};
use super::provisioning::{
    EcsCurrentState, EcsProvisioningConfig, PlannedChange, ProvisioningPlan, ProvisioningResult,
};
use crate::config::InfrastructureConfig;
use crate::deployment::{DeploymentResult, DeploymentType};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadBalancerConfig {
    #[serde(default)]
    pub target_group_arn: Option<String>,

    #[serde(default)]
    pub container_name: Option<String>,

    #[serde(default)]
    pub container_port: Option<i32>,

    #[serde(default)]
    pub health_check: Option<HealthCheckConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    #[serde(default = "default_health_check_path")]
    pub path: String,

    #[serde(default = "default_health_check_interval")]
    pub interval_seconds: i32,

    #[serde(default = "default_health_check_timeout")]
    pub timeout_seconds: i32,

    #[serde(default = "default_healthy_threshold")]
    pub healthy_threshold: i32,

    #[serde(default = "default_unhealthy_threshold")]
    pub unhealthy_threshold: i32,

    #[serde(default)]
    pub matcher: Option<String>,
}

fn default_health_check_path() -> String {
    "/health".to_string()
}
fn default_health_check_interval() -> i32 {
    30
}
fn default_health_check_timeout() -> i32 {
    5
}
fn default_healthy_threshold() -> i32 {
    2
}
fn default_unhealthy_threshold() -> i32 {
    3
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            path: default_health_check_path(),
            interval_seconds: default_health_check_interval(),
            timeout_seconds: default_health_check_timeout(),
            healthy_threshold: default_healthy_threshold(),
            unhealthy_threshold: default_unhealthy_threshold(),
            matcher: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutoScalingConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_min_capacity")]
    pub min_capacity: i32,

    #[serde(default = "default_max_capacity")]
    pub max_capacity: i32,

    #[serde(default)]
    pub target_tracking: Option<TargetTrackingConfig>,

    #[serde(default)]
    pub step_scaling: Option<StepScalingConfig>,

    #[serde(default)]
    pub scheduled: Vec<ScheduledScalingConfig>,
}

fn default_min_capacity() -> i32 {
    1
}
fn default_max_capacity() -> i32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetTrackingConfig {
    #[serde(default = "default_target_value")]
    pub target_value: f64,

    #[serde(default = "default_metric_type")]
    pub metric_type: String,

    #[serde(default)]
    pub scale_in_cooldown: Option<i32>,

    #[serde(default)]
    pub scale_out_cooldown: Option<i32>,

    #[serde(default)]
    pub disable_scale_in: bool,
}

fn default_target_value() -> f64 {
    70.0
}
fn default_metric_type() -> String {
    "ECSServiceAverageCPUUtilization".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepScalingConfig {
    pub adjustment_type: String,
    pub steps: Vec<StepAdjustment>,
    #[serde(default)]
    pub cooldown: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepAdjustment {
    pub lower_bound: Option<f64>,
    pub upper_bound: Option<f64>,
    pub adjustment: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledScalingConfig {
    pub name: String,
    pub schedule: String,
    pub min_capacity: Option<i32>,
    pub max_capacity: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudWatchLogsConfig {
    #[serde(default)]
    pub log_group: Option<String>,

    #[serde(default)]
    pub log_stream_prefix: Option<String>,

    #[serde(default = "default_log_retention_days")]
    pub retention_days: i32,

    #[serde(default)]
    pub create_log_group: bool,
}

fn default_log_retention_days() -> i32 {
    30
}

impl LoadBalancerConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

impl AutoScalingConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

impl CloudWatchLogsConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

pub struct AwsEcsProvider {
    ecs_client: EcsClient,
    elb_client: ElbClient,
    logs_client: CloudWatchLogsClient,
    autoscaling_client: AutoScalingClient,
    cluster: String,
    region: String,
    service_name: Option<String>,
    load_balancer_config: Option<LoadBalancerConfig>,
    autoscaling_config: Option<AutoScalingConfig>,
    cloudwatch_config: Option<CloudWatchLogsConfig>,
    provisioning_config: Option<EcsProvisioningConfig>,
}

impl AwsEcsProvider {
    pub async fn new(cluster: &str, region: &str) -> anyhow::Result<Self> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_ecs::config::Region::new(region.to_string()))
            .load()
            .await;

        let ecs_client = EcsClient::new(&aws_config);
        let elb_client = ElbClient::new(&aws_config);
        let logs_client = CloudWatchLogsClient::new(&aws_config);
        let autoscaling_client = AutoScalingClient::new(&aws_config);

        Ok(Self {
            ecs_client,
            elb_client,
            logs_client,
            autoscaling_client,
            cluster: cluster.to_string(),
            region: region.to_string(),
            service_name: None,
            load_balancer_config: None,
            autoscaling_config: None,
            cloudwatch_config: None,
            provisioning_config: None,
        })
    }

    pub fn with_service_name(mut self, name: &str) -> Self {
        self.service_name = Some(name.to_string());
        self
    }

    pub fn with_provisioning(mut self, config: EcsProvisioningConfig) -> Self {
        self.provisioning_config = Some(config);
        self
    }

    pub async fn from_config(config: &InfrastructureConfig) -> anyhow::Result<Self> {
        let cluster = config
            .config
            .get("cluster")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cluster is required for aws-ecs"))?;

        let region = config
            .config
            .get("region")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("region is required for aws-ecs"))?;

        let mut provider = Self::new(cluster, region).await?;

        if let Some(service) = config.config.get("service_name").and_then(|v| v.as_str()) {
            provider.service_name = Some(service.to_string());
        }

        provider.load_balancer_config = config
            .config
            .get("load_balancer")
            .and_then(LoadBalancerConfig::from_yaml_value);

        provider.autoscaling_config = config
            .config
            .get("autoscaling")
            .and_then(AutoScalingConfig::from_yaml_value);

        provider.cloudwatch_config = config
            .config
            .get("cloudwatch_logs")
            .and_then(CloudWatchLogsConfig::from_yaml_value);

        if let Some(provision_value) = config.config.get("provision") {
            if let Some(provision_config) = EcsProvisioningConfig::from_yaml_value(provision_value) {
                provider.provisioning_config = Some(provision_config);
            }
        }

        Ok(provider)
    }

    async fn get_service_status(&self, service_name: &str) -> anyhow::Result<ServiceStatus> {
        let response = self
            .ecs_client
            .describe_services()
            .cluster(&self.cluster)
            .services(service_name)
            .send()
            .await?;

        let service = response
            .services
            .and_then(|s| s.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Service {} not found", service_name))?;

        Ok(ServiceStatus {
            name: service.service_name.unwrap_or_default(),
            status: service.status.unwrap_or_default(),
            running_count: service.running_count,
            desired_count: service.desired_count,
            pending_count: service.pending_count,
            task_definition: service.task_definition,
            load_balancers: service.load_balancers,
        })
    }

    async fn update_service_task_definition(
        &self,
        service_name: &str,
        image: &str,
        env_vars: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<String> {
        let service_status = self.get_service_status(service_name).await?;

        let task_def_arn = service_status
            .task_definition
            .ok_or_else(|| anyhow::anyhow!("Service has no task definition"))?;

        let task_def = self
            .ecs_client
            .describe_task_definition()
            .task_definition(&task_def_arn)
            .send()
            .await?
            .task_definition
            .ok_or_else(|| anyhow::anyhow!("Task definition not found"))?;

        let mut container_defs = task_def.container_definitions.unwrap_or_default();

        if container_defs.is_empty() {
            anyhow::bail!("Task definition has no container definitions");
        }

        // Update the container image
        container_defs[0].image = Some(image.to_string());

        // Update environment variables if provided
        if let Some(vars) = env_vars {
            let env_list = convert_env_vars_to_key_value_pairs(vars);
            container_defs[0].environment = Some(env_list);
            tracing::info!("Setting {} environment variables", vars.len());
        }

        let new_task_def = self
            .ecs_client
            .register_task_definition()
            .family(task_def.family.unwrap_or_default())
            .set_container_definitions(Some(container_defs))
            .set_task_role_arn(task_def.task_role_arn)
            .set_execution_role_arn(task_def.execution_role_arn)
            .set_network_mode(task_def.network_mode)
            .set_volumes(task_def.volumes)
            .set_placement_constraints(task_def.placement_constraints)
            .set_requires_compatibilities(task_def.requires_compatibilities)
            .set_cpu(task_def.cpu)
            .set_memory(task_def.memory)
            .send()
            .await?
            .task_definition
            .ok_or_else(|| anyhow::anyhow!("Failed to register task definition"))?;

        let new_task_def_arn = new_task_def
            .task_definition_arn
            .ok_or_else(|| anyhow::anyhow!("New task definition has no ARN"))?;

        self.ecs_client
            .update_service()
            .cluster(&self.cluster)
            .service(service_name)
            .task_definition(&new_task_def_arn)
            .send()
            .await?;

        Ok(new_task_def_arn)
    }

    async fn wait_for_stable(&self, service_name: &str, timeout_secs: u64) -> anyhow::Result<()> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() > timeout_secs {
                anyhow::bail!("Service deployment timed out after {} seconds", timeout_secs);
            }

            let status = self.get_service_status(service_name).await?;

            tracing::info!(
                "Service {}: {}/{} running, {} pending",
                service_name,
                status.running_count,
                status.desired_count,
                status.pending_count
            );

            if status.running_count == status.desired_count && status.pending_count == 0 {
                let deployments = self
                    .ecs_client
                    .describe_services()
                    .cluster(&self.cluster)
                    .services(service_name)
                    .send()
                    .await?
                    .services
                    .and_then(|s| s.into_iter().next())
                    .and_then(|s| s.deployments);

                if let Some(deps) = deployments {
                    let active_deployments: Vec<_> = deps
                        .iter()
                        .filter(|d| d.status.as_deref() == Some("PRIMARY"))
                        .collect();

                    if active_deployments.len() == 1 {
                        let primary = &active_deployments[0];

                        if primary.running_count == primary.desired_count {
                            return Ok(());
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }

    fn get_service_name(&self, ctx: &DeploymentContext) -> String {
        self.service_name.clone().unwrap_or_else(|| {
            ctx.environment
                .config
                .get("service_name")
                .and_then(|v| v.as_str())
                .unwrap_or("app")
                .to_string()
        })
    }

    pub async fn get_target_group_health(
        &self,
        target_group_arn: &str,
    ) -> anyhow::Result<TargetGroupHealth> {
        let response = self
            .elb_client
            .describe_target_health()
            .target_group_arn(target_group_arn)
            .send()
            .await?;

        let health_descriptions = response.target_health_descriptions.unwrap_or_default();

        let healthy = health_descriptions
            .iter()
            .filter(|t| {
                t.target_health
                    .as_ref()
                    .and_then(|h| h.state.as_ref())
                    .map(|s| s.as_str() == "healthy")
                    .unwrap_or(false)
            })
            .count();

        let unhealthy = health_descriptions
            .iter()
            .filter(|t| {
                t.target_health
                    .as_ref()
                    .and_then(|h| h.state.as_ref())
                    .map(|s| s.as_str() == "unhealthy")
                    .unwrap_or(false)
            })
            .count();

        let draining = health_descriptions
            .iter()
            .filter(|t| {
                t.target_health
                    .as_ref()
                    .and_then(|h| h.state.as_ref())
                    .map(|s| s.as_str() == "draining")
                    .unwrap_or(false)
            })
            .count();

        Ok(TargetGroupHealth {
            total: health_descriptions.len(),
            healthy,
            unhealthy,
            draining,
        })
    }

    pub async fn update_target_group_health_check(
        &self,
        target_group_arn: &str,
        config: &HealthCheckConfig,
    ) -> anyhow::Result<()> {
        let mut builder = self
            .elb_client
            .modify_target_group()
            .target_group_arn(target_group_arn)
            .health_check_path(&config.path)
            .health_check_interval_seconds(config.interval_seconds)
            .health_check_timeout_seconds(config.timeout_seconds)
            .healthy_threshold_count(config.healthy_threshold)
            .unhealthy_threshold_count(config.unhealthy_threshold);

        if let Some(matcher) = &config.matcher {
            builder = builder.matcher(
                aws_sdk_elasticloadbalancingv2::types::Matcher::builder()
                    .http_code(matcher)
                    .build(),
            );
        }

        builder.send().await?;

        tracing::info!(
            "Updated health check for target group: path={}, interval={}s",
            config.path,
            config.interval_seconds
        );

        Ok(())
    }

    pub async fn configure_autoscaling(
        &self,
        service_name: &str,
        config: &AutoScalingConfig,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        let resource_id = format!("service/{}/{}", self.cluster, service_name);

        self.autoscaling_client
            .register_scalable_target()
            .service_namespace(
                aws_sdk_applicationautoscaling::types::ServiceNamespace::Ecs,
            )
            .resource_id(&resource_id)
            .scalable_dimension(
                aws_sdk_applicationautoscaling::types::ScalableDimension::EcsServiceDesiredCount,
            )
            .min_capacity(config.min_capacity)
            .max_capacity(config.max_capacity)
            .send()
            .await?;

        tracing::info!(
            "Registered scalable target for {}: min={}, max={}",
            service_name,
            config.min_capacity,
            config.max_capacity
        );

        if let Some(target_tracking) = &config.target_tracking {
            self.configure_target_tracking(&resource_id, service_name, target_tracking)
                .await?;
        }

        for scheduled in &config.scheduled {
            self.configure_scheduled_scaling(&resource_id, scheduled)
                .await?;
        }

        Ok(())
    }

    async fn configure_target_tracking(
        &self,
        resource_id: &str,
        service_name: &str,
        config: &TargetTrackingConfig,
    ) -> anyhow::Result<()> {
        let metric_spec = match config.metric_type.as_str() {
            "ECSServiceAverageCPUUtilization" => {
                aws_sdk_applicationautoscaling::types::PredefinedMetricSpecification::builder()
                    .predefined_metric_type(
                        aws_sdk_applicationautoscaling::types::MetricType::EcsServiceAverageCpuUtilization,
                    )
                    .build()?
            }
            "ECSServiceAverageMemoryUtilization" => {
                aws_sdk_applicationautoscaling::types::PredefinedMetricSpecification::builder()
                    .predefined_metric_type(
                        aws_sdk_applicationautoscaling::types::MetricType::EcsServiceAverageMemoryUtilization,
                    )
                    .build()?
            }
            "ALBRequestCountPerTarget" => {
                aws_sdk_applicationautoscaling::types::PredefinedMetricSpecification::builder()
                    .predefined_metric_type(
                        aws_sdk_applicationautoscaling::types::MetricType::AlbRequestCountPerTarget,
                    )
                    .build()?
            }
            _ => {
                anyhow::bail!("Unsupported metric type: {}", config.metric_type);
            }
        };

        let mut target_config_builder =
            aws_sdk_applicationautoscaling::types::TargetTrackingScalingPolicyConfiguration::builder()
                .target_value(config.target_value)
                .predefined_metric_specification(metric_spec)
                .disable_scale_in(config.disable_scale_in);

        if let Some(cooldown) = config.scale_in_cooldown {
            target_config_builder = target_config_builder.scale_in_cooldown(cooldown);
        }

        if let Some(cooldown) = config.scale_out_cooldown {
            target_config_builder = target_config_builder.scale_out_cooldown(cooldown);
        }

        self.autoscaling_client
            .put_scaling_policy()
            .service_namespace(
                aws_sdk_applicationautoscaling::types::ServiceNamespace::Ecs,
            )
            .resource_id(resource_id)
            .scalable_dimension(
                aws_sdk_applicationautoscaling::types::ScalableDimension::EcsServiceDesiredCount,
            )
            .policy_name(format!("{}-target-tracking", service_name))
            .policy_type(aws_sdk_applicationautoscaling::types::PolicyType::TargetTrackingScaling)
            .target_tracking_scaling_policy_configuration(target_config_builder.build()?)
            .send()
            .await?;

        tracing::info!(
            "Configured target tracking scaling: metric={}, target={}",
            config.metric_type,
            config.target_value
        );

        Ok(())
    }

    async fn configure_scheduled_scaling(
        &self,
        resource_id: &str,
        config: &ScheduledScalingConfig,
    ) -> anyhow::Result<()> {
        let mut builder = self
            .autoscaling_client
            .put_scheduled_action()
            .service_namespace(
                aws_sdk_applicationautoscaling::types::ServiceNamespace::Ecs,
            )
            .resource_id(resource_id)
            .scalable_dimension(
                aws_sdk_applicationautoscaling::types::ScalableDimension::EcsServiceDesiredCount,
            )
            .scheduled_action_name(&config.name)
            .schedule(&config.schedule);

        let mut scalable_target_action =
            aws_sdk_applicationautoscaling::types::ScalableTargetAction::builder();

        if let Some(min) = config.min_capacity {
            scalable_target_action = scalable_target_action.min_capacity(min);
        }

        if let Some(max) = config.max_capacity {
            scalable_target_action = scalable_target_action.max_capacity(max);
        }

        builder = builder.scalable_target_action(scalable_target_action.build());
        builder.send().await?;

        tracing::info!(
            "Configured scheduled scaling action: name={}, schedule={}",
            config.name,
            config.schedule
        );

        Ok(())
    }

    pub async fn get_autoscaling_status(
        &self,
        service_name: &str,
    ) -> anyhow::Result<Option<AutoScalingStatus>> {
        let resource_id = format!("service/{}/{}", self.cluster, service_name);

        let targets = self
            .autoscaling_client
            .describe_scalable_targets()
            .service_namespace(
                aws_sdk_applicationautoscaling::types::ServiceNamespace::Ecs,
            )
            .resource_ids(&resource_id)
            .send()
            .await?;

        let target = targets.scalable_targets.and_then(|t| t.into_iter().next());

        if let Some(target) = target {
            let policies = self
                .autoscaling_client
                .describe_scaling_policies()
                .service_namespace(
                    aws_sdk_applicationautoscaling::types::ServiceNamespace::Ecs,
                )
                .resource_id(&resource_id)
                .send()
                .await?;

            let policy_names: Vec<String> = policies
                .scaling_policies
                .unwrap_or_default()
                .iter()
                .map(|p| p.policy_name.clone())
                .collect();

            return Ok(Some(AutoScalingStatus {
                min_capacity: target.min_capacity,
                max_capacity: target.max_capacity,
                policies: policy_names,
            }));
        }

        Ok(None)
    }

    pub async fn ensure_log_group(&self, log_group: &str) -> anyhow::Result<()> {
        let exists = self
            .logs_client
            .describe_log_groups()
            .log_group_name_prefix(log_group)
            .send()
            .await?
            .log_groups
            .unwrap_or_default()
            .iter()
            .any(|g| g.log_group_name.as_deref() == Some(log_group));

        if !exists {
            self.logs_client
                .create_log_group()
                .log_group_name(log_group)
                .send()
                .await?;

            tracing::info!("Created CloudWatch log group: {}", log_group);

            if let Some(config) = &self.cloudwatch_config {
                if config.retention_days > 0 {
                    self.logs_client
                        .put_retention_policy()
                        .log_group_name(log_group)
                        .retention_in_days(config.retention_days)
                        .send()
                        .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn get_recent_logs(
        &self,
        log_group: &str,
        log_stream_prefix: Option<&str>,
        limit: i32,
    ) -> anyhow::Result<Vec<LogEvent>> {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64
            - (3600 * 1000);

        let mut builder = self
            .logs_client
            .filter_log_events()
            .log_group_name(log_group)
            .start_time(start_time)
            .limit(limit);

        if let Some(prefix) = log_stream_prefix {
            builder = builder.log_stream_name_prefix(prefix);
        }

        let response = builder.send().await?;

        let events: Vec<LogEvent> = response
            .events
            .unwrap_or_default()
            .into_iter()
            .map(|e| LogEvent {
                timestamp: e.timestamp.unwrap_or(0),
                message: e.message.unwrap_or_default(),
                log_stream: e.log_stream_name.unwrap_or_default(),
            })
            .collect();

        Ok(events)
    }

    fn get_log_group_name(&self, service_name: &str) -> String {
        if let Some(config) = &self.cloudwatch_config {
            if let Some(log_group) = &config.log_group {
                return log_group.clone();
            }
        }

        format!("/ecs/{}", service_name)
    }

    // ========== Provisioning Methods ==========

    /// Check if the ECS cluster exists
    pub async fn cluster_exists(&self) -> anyhow::Result<bool> {
        let response = self
            .ecs_client
            .describe_clusters()
            .clusters(&self.cluster)
            .send()
            .await?;

        let cluster = response
            .clusters
            .and_then(|c| c.into_iter().next());

        Ok(cluster.map(|c| c.status.unwrap_or_default() == "ACTIVE").unwrap_or(false))
    }

    /// Check if the ECS service exists
    pub async fn service_exists(&self, service_name: &str) -> anyhow::Result<bool> {
        let response = self
            .ecs_client
            .describe_services()
            .cluster(&self.cluster)
            .services(service_name)
            .send()
            .await?;

        let service = response
            .services
            .and_then(|s| s.into_iter().next());

        Ok(service
            .map(|s| {
                let status = s.status.unwrap_or_default();
                status == "ACTIVE" || status == "DRAINING"
            })
            .unwrap_or(false))
    }

    /// Get the current state of ECS resources for comparison
    pub async fn get_current_state(&self, service_name: &str) -> anyhow::Result<EcsCurrentState> {
        let cluster_exists = self.cluster_exists().await?;

        if !cluster_exists {
            return Ok(EcsCurrentState::not_found());
        }

        let cluster_arn = self
            .ecs_client
            .describe_clusters()
            .clusters(&self.cluster)
            .send()
            .await?
            .clusters
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.cluster_arn);

        let service_exists = self.service_exists(service_name).await?;

        if !service_exists {
            return Ok(EcsCurrentState {
                cluster_exists: true,
                service_exists: false,
                task_definition_arn: None,
                cluster_arn,
                service_arn: None,
                current_desired_count: None,
                current_task_definition: None,
            });
        }

        let service_status = self.get_service_status(service_name).await?;

        Ok(EcsCurrentState {
            cluster_exists: true,
            service_exists: true,
            task_definition_arn: service_status.task_definition.clone(),
            cluster_arn,
            service_arn: None,
            current_desired_count: Some(service_status.desired_count),
            current_task_definition: service_status.task_definition,
        })
    }

    /// Plan provisioning changes without applying them
    pub async fn plan_provisioning(&self, service_name: &str) -> anyhow::Result<ProvisioningPlan> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        config.validate()?;

        let current = self.get_current_state(service_name).await?;

        let mut plan = ProvisioningPlan::new();

        if !current.cluster_exists {
            plan.add(PlannedChange::create(
                "ECS Cluster",
                &self.cluster,
                "Cluster does not exist",
            ));
        }

        // Always plan to register a new task definition (they are immutable)
        plan.add(PlannedChange::create(
            "ECS Task Definition",
            &config.task.family,
            "New task definition revision will be registered",
        ));

        if !current.service_exists {
            plan.add(PlannedChange::create(
                "ECS Service",
                service_name,
                "Service does not exist",
            ));
        } else {
            if let Some(current_count) = current.current_desired_count {
                if current_count != config.service.desired_count {
                    plan.add(PlannedChange::update(
                        "ECS Service",
                        service_name,
                        &current_count.to_string(),
                        &config.service.desired_count.to_string(),
                        "Desired count will be changed",
                    ));
                }
            }
        }

        Ok(plan)
    }

    /// Create a new ECS cluster
    pub async fn create_cluster(&self) -> anyhow::Result<ProvisioningResult> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        tracing::info!("Creating ECS cluster: {}", self.cluster);

        let mut builder = self.ecs_client.create_cluster().cluster_name(&self.cluster);

        if let Some(cluster_config) = &config.cluster {
            // Add capacity providers
            for provider in &cluster_config.capacity_providers {
                builder = builder.capacity_providers(provider.clone());
            }

            // Add default capacity provider strategy
            for strategy in &cluster_config.default_capacity_provider_strategy {
                builder = builder.default_capacity_provider_strategy(
                    aws_sdk_ecs::types::CapacityProviderStrategyItem::builder()
                        .capacity_provider(&strategy.capacity_provider)
                        .weight(strategy.weight)
                        .base(strategy.base)
                        .build()?,
                );
            }

            // Enable Container Insights
            if cluster_config.container_insights {
                builder = builder.settings(
                    aws_sdk_ecs::types::ClusterSetting::builder()
                        .name(aws_sdk_ecs::types::ClusterSettingName::ContainerInsights)
                        .value("enabled")
                        .build(),
                );
            }
        }

        builder.send().await?;

        tracing::info!("Successfully created ECS cluster: {}", self.cluster);

        Ok(ProvisioningResult::created(
            "ECS Cluster",
            &self.cluster,
            vec!["Cluster created".to_string()],
        ))
    }

    /// Register a new task definition
    pub async fn register_task_definition(
        &self,
        image_uri: &str,
    ) -> anyhow::Result<String> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        tracing::info!(
            "Registering task definition: {} with image: {}",
            config.task.family,
            image_uri
        );

        let mut builder = self
            .ecs_client
            .register_task_definition()
            .family(&config.task.family)
            .cpu(&config.task.cpu)
            .memory(&config.task.memory)
            .execution_role_arn(&config.task.execution_role_arn)
            .network_mode(aws_sdk_ecs::types::NetworkMode::Awsvpc);

        if let Some(task_role) = &config.task.task_role_arn {
            builder = builder.task_role_arn(task_role);
        }

        // Add requires compatibilities
        for compat in &config.task.requires_compatibilities {
            let compatibility = match compat.as_str() {
                "EC2" => Compatibility::Ec2,
                _ => Compatibility::Fargate,
            };
            builder = builder.requires_compatibilities(compatibility);
        }

        // Add runtime platform
        if let Some(platform) = &config.task.runtime_platform {
            let mut runtime_builder = aws_sdk_ecs::types::RuntimePlatform::builder()
                .operating_system_family(
                    aws_sdk_ecs::types::OsFamily::from(platform.operating_system_family.as_str()),
                );

            if let Some(cpu_arch) = &platform.cpu_architecture {
                runtime_builder = runtime_builder.cpu_architecture(
                    aws_sdk_ecs::types::CpuArchitecture::from(cpu_arch.as_str()),
                );
            }

            builder = builder.runtime_platform(runtime_builder.build());
        }

        // Build container definitions
        let container_defs = self.build_container_definitions(image_uri)?;
        builder = builder.set_container_definitions(Some(container_defs));

        let response = builder.send().await?;

        let task_def_arn = response
            .task_definition
            .and_then(|td| td.task_definition_arn)
            .ok_or_else(|| anyhow::anyhow!("Failed to get task definition ARN"))?;

        tracing::info!("Registered task definition: {}", task_def_arn);

        Ok(task_def_arn)
    }

    /// Build container definitions from provisioning config
    fn build_container_definitions(&self, image_uri: &str) -> anyhow::Result<Vec<EcsContainerDef>> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        let mut result = Vec::new();

        if config.task.containers.is_empty() {
            // Create a default container with the provided image
            let container = EcsContainerDef::builder()
                .name("app")
                .image(image_uri)
                .essential(true)
                .build();
            result.push(container);
        } else {
            for container_config in &config.task.containers {
                let mut builder = EcsContainerDef::builder()
                    .name(&container_config.name)
                    .image(if container_config.name == "app" || config.task.containers.len() == 1 {
                        image_uri
                    } else {
                        &container_config.image
                    })
                    .essential(container_config.essential);

                if let Some(cpu) = container_config.cpu {
                    builder = builder.cpu(cpu);
                }

                if let Some(memory) = container_config.memory {
                    builder = builder.memory(memory);
                }

                if let Some(memory_reservation) = container_config.memory_reservation {
                    builder = builder.memory_reservation(memory_reservation);
                }

                // Add port mappings
                for pm in &container_config.port_mappings {
                    let mut pm_builder = EcsPortMapping::builder()
                        .container_port(pm.container_port)
                        .protocol(aws_sdk_ecs::types::TransportProtocol::from(pm.protocol.as_str()));

                    if let Some(host_port) = pm.host_port {
                        pm_builder = pm_builder.host_port(host_port);
                    }

                    if let Some(name) = &pm.name {
                        pm_builder = pm_builder.name(name);
                    }

                    builder = builder.port_mappings(pm_builder.build());
                }

                // Add environment variables
                for env_var in &container_config.environment {
                    builder = builder.environment(
                        KeyValuePair::builder()
                            .name(&env_var.name)
                            .value(&env_var.value)
                            .build(),
                    );
                }

                // Add secrets
                for secret in &container_config.secrets {
                    builder = builder.secrets(
                        EcsSecret::builder()
                            .name(&secret.name)
                            .value_from(&secret.value_from)
                            .build()?,
                    );
                }

                // Add log configuration
                if let Some(log_config) = &container_config.log_configuration {
                    let mut options = HashMap::new();
                    for (key, value) in &log_config.options {
                        options.insert(key.clone(), value.clone());
                    }

                    builder = builder.log_configuration(
                        aws_sdk_ecs::types::LogConfiguration::builder()
                            .log_driver(aws_sdk_ecs::types::LogDriver::from(log_config.log_driver.as_str()))
                            .set_options(Some(options))
                            .build()?,
                    );
                }

                // Add command
                if !container_config.command.is_empty() {
                    builder = builder.set_command(Some(container_config.command.clone()));
                }

                // Add entry point
                if !container_config.entry_point.is_empty() {
                    builder = builder.set_entry_point(Some(container_config.entry_point.clone()));
                }

                if let Some(work_dir) = &container_config.working_directory {
                    builder = builder.working_directory(work_dir);
                }

                result.push(builder.build());
            }
        }

        Ok(result)
    }

    /// Create a new ECS service
    pub async fn create_service(
        &self,
        service_name: &str,
        task_definition_arn: &str,
    ) -> anyhow::Result<ProvisioningResult> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        tracing::info!("Creating ECS service: {}", service_name);

        let mut builder = self
            .ecs_client
            .create_service()
            .cluster(&self.cluster)
            .service_name(service_name)
            .task_definition(task_definition_arn)
            .desired_count(config.service.desired_count)
            .scheduling_strategy(
                aws_sdk_ecs::types::SchedulingStrategy::from(config.service.scheduling_strategy.as_str()),
            );

        // Set launch type or capacity provider strategy
        if config.service.capacity_provider_strategy.is_empty() {
            if let Some(launch_type) = &config.service.launch_type {
                let lt = match launch_type.as_str() {
                    "EC2" => LaunchType::Ec2,
                    "EXTERNAL" => LaunchType::External,
                    _ => LaunchType::Fargate,
                };
                builder = builder.launch_type(lt);
            }
        } else {
            for strategy in &config.service.capacity_provider_strategy {
                builder = builder.capacity_provider_strategy(
                    aws_sdk_ecs::types::CapacityProviderStrategyItem::builder()
                        .capacity_provider(&strategy.capacity_provider)
                        .weight(strategy.weight)
                        .base(strategy.base)
                        .build()?,
                );
            }
        }

        // Set platform version for Fargate
        builder = builder.platform_version(&config.service.platform_version);

        // Set deployment configuration
        if let Some(deployment) = &config.service.deployment {
            let mut deployment_config = EcsDeploymentConfig::builder()
                .minimum_healthy_percent(deployment.minimum_healthy_percent)
                .maximum_percent(deployment.maximum_percent);

            if config.service.enable_circuit_breaker {
                deployment_config = deployment_config.deployment_circuit_breaker(
                    DeploymentCircuitBreaker::builder()
                        .enable(true)
                        .rollback(true)
                        .build(),
                );
            }

            builder = builder.deployment_configuration(deployment_config.build());
        }

        // Set network configuration
        if let Some(network) = &config.service.network {
            let assign_public_ip = if network.assign_public_ip {
                AssignPublicIp::Enabled
            } else {
                AssignPublicIp::Disabled
            };

            let vpc_config = AwsVpcConfiguration::builder()
                .set_subnets(Some(network.subnets.clone()))
                .set_security_groups(Some(network.security_groups.clone()))
                .assign_public_ip(assign_public_ip)
                .build()?;

            builder = builder.network_configuration(
                NetworkConfiguration::builder()
                    .awsvpc_configuration(vpc_config)
                    .build(),
            );
        }

        // Set load balancers
        for lb in &config.service.load_balancers {
            builder = builder.load_balancers(
                aws_sdk_ecs::types::LoadBalancer::builder()
                    .target_group_arn(&lb.target_group_arn)
                    .container_name(&lb.container_name)
                    .container_port(lb.container_port)
                    .build(),
            );
        }

        // Set service registries
        for registry in &config.service.service_registries {
            let mut reg_builder = aws_sdk_ecs::types::ServiceRegistry::builder()
                .registry_arn(&registry.registry_arn);

            if let Some(container_name) = &registry.container_name {
                reg_builder = reg_builder.container_name(container_name);
            }

            if let Some(container_port) = registry.container_port {
                reg_builder = reg_builder.container_port(container_port);
            }

            builder = builder.service_registries(reg_builder.build());
        }

        // Enable ECS Exec
        if config.service.enable_execute_command {
            builder = builder.enable_execute_command(true);
        }

        // Set health check grace period
        if let Some(grace_period) = config.service.health_check_grace_period_seconds {
            builder = builder.health_check_grace_period_seconds(grace_period);
        }

        builder.send().await?;

        tracing::info!("Successfully created ECS service: {}", service_name);

        Ok(ProvisioningResult::created(
            "ECS Service",
            service_name,
            vec![
                format!("Desired count: {}", config.service.desired_count),
                format!("Task definition: {}", task_definition_arn),
            ],
        ))
    }

    /// Update an existing ECS service
    pub async fn update_service_provisioning(
        &self,
        service_name: &str,
        task_definition_arn: &str,
    ) -> anyhow::Result<ProvisioningResult> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        tracing::info!("Updating ECS service: {}", service_name);

        let mut builder = self
            .ecs_client
            .update_service()
            .cluster(&self.cluster)
            .service(service_name)
            .task_definition(task_definition_arn)
            .desired_count(config.service.desired_count);

        // Update deployment configuration
        if let Some(deployment) = &config.service.deployment {
            let mut deployment_config = EcsDeploymentConfig::builder()
                .minimum_healthy_percent(deployment.minimum_healthy_percent)
                .maximum_percent(deployment.maximum_percent);

            if config.service.enable_circuit_breaker {
                deployment_config = deployment_config.deployment_circuit_breaker(
                    DeploymentCircuitBreaker::builder()
                        .enable(true)
                        .rollback(true)
                        .build(),
                );
            }

            builder = builder.deployment_configuration(deployment_config.build());
        }

        // Update network configuration
        if let Some(network) = &config.service.network {
            let assign_public_ip = if network.assign_public_ip {
                AssignPublicIp::Enabled
            } else {
                AssignPublicIp::Disabled
            };

            let vpc_config = AwsVpcConfiguration::builder()
                .set_subnets(Some(network.subnets.clone()))
                .set_security_groups(Some(network.security_groups.clone()))
                .assign_public_ip(assign_public_ip)
                .build()?;

            builder = builder.network_configuration(
                NetworkConfiguration::builder()
                    .awsvpc_configuration(vpc_config)
                    .build(),
            );
        }

        builder.send().await?;

        tracing::info!("Successfully updated ECS service: {}", service_name);

        Ok(ProvisioningResult::updated(
            "ECS Service",
            service_name,
            vec![format!("Updated to task definition: {}", task_definition_arn)],
        ))
    }

    /// Provision ECS resources (create cluster/service if not exists, update if exists)
    pub async fn provision(
        &self,
        service_name: &str,
        image_uri: &str,
    ) -> anyhow::Result<Vec<ProvisioningResult>> {
        let config = self.provisioning_config.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provisioning configuration set"))?;

        config.validate()?;

        let mut results = Vec::new();

        // 1. Create cluster if it doesn't exist
        let cluster_exists = self.cluster_exists().await?;
        if !cluster_exists {
            results.push(self.create_cluster().await?);
        } else {
            results.push(ProvisioningResult::unchanged("ECS Cluster", &self.cluster));
        }

        // 2. Register new task definition (always creates a new revision)
        let task_def_arn = self.register_task_definition(image_uri).await?;
        results.push(ProvisioningResult::created(
            "ECS Task Definition",
            &config.task.family,
            vec![format!("ARN: {}", task_def_arn)],
        ));

        // 3. Create or update service
        let service_exists = self.service_exists(service_name).await?;
        if service_exists {
            results.push(self.update_service_provisioning(service_name, &task_def_arn).await?);
        } else {
            results.push(self.create_service(service_name, &task_def_arn).await?);
        }

        // 4. Wait for service to stabilize
        let timeout = config.service.health_check_grace_period_seconds
            .map(|s| s as u64)
            .unwrap_or(600);
        self.wait_for_stable(service_name, timeout).await?;

        Ok(results)
    }
}

struct ServiceStatus {
    name: String,
    status: String,
    running_count: i32,
    desired_count: i32,
    pending_count: i32,
    task_definition: Option<String>,
    load_balancers: Option<Vec<aws_sdk_ecs::types::LoadBalancer>>,
}

#[derive(Debug)]
pub struct TargetGroupHealth {
    pub total: usize,
    pub healthy: usize,
    pub unhealthy: usize,
    pub draining: usize,
}

#[derive(Debug)]
pub struct AutoScalingStatus {
    pub min_capacity: i32,
    pub max_capacity: i32,
    pub policies: Vec<String>,
}

#[derive(Debug)]
pub struct LogEvent {
    pub timestamp: i64,
    pub message: String,
    pub log_stream: String,
}

#[async_trait]
impl InfrastructureProvider for AwsEcsProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::AwsEcs
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        vec![DeploymentType::RollingUpdate, DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        _config: &HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        let response = self
            .ecs_client
            .describe_clusters()
            .clusters(&self.cluster)
            .send()
            .await?;

        let cluster = response
            .clusters
            .and_then(|c| c.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("Cluster {} not found", self.cluster))?;

        let status = cluster.status.unwrap_or_default();

        if status != "ACTIVE" {
            anyhow::bail!("Cluster {} is not active (status: {})", self.cluster, status);
        }

        Ok(())
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let image = ctx
            .environment
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No image specified for deployment"))?;

        let service_name = self.get_service_name(ctx);
        let is_app_only = ctx.deploy_mode.is_app_only();

        if ctx.dry_run {
            let mode_msg = if is_app_only { " (app-only)" } else { "" };
            return Ok(DeploymentResult::success(format!(
                "Would deploy {} to ECS service {} in cluster {}{}",
                image, service_name, self.cluster, mode_msg
            )));
        }

        tracing::info!(
            "Deploying {} to ECS service {} in cluster {} (mode: {:?})",
            image,
            service_name,
            self.cluster,
            ctx.deploy_mode
        );

        // Log group creation is safe even in app-only mode (needed for logging)
        if let Some(config) = &self.cloudwatch_config {
            if config.create_log_group {
                let log_group = self.get_log_group_name(&service_name);
                self.ensure_log_group(&log_group).await?;
            }
        }

        // Resolve environment variables from configuration
        let env_vars = ctx.resolve_env_vars().await?;
        let env_vars_opt = if env_vars.is_empty() {
            None
        } else {
            Some(&env_vars)
        };

        // Update the container image and environment variables
        let new_task_def = self
            .update_service_task_definition(&service_name, image, env_vars_opt)
            .await?;

        tracing::info!("Registered new task definition: {}", new_task_def);

        let timeout = ctx
            .environment
            .config
            .get("deployment_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(600);

        self.wait_for_stable(&service_name, timeout).await?;

        // Health check monitoring is informational, safe in any mode
        if let Some(lb_config) = &self.load_balancer_config {
            if let Some(target_group_arn) = &lb_config.target_group_arn {
                let health = self.get_target_group_health(target_group_arn).await?;
                tracing::info!(
                    "Target group health: {}/{} healthy, {} unhealthy, {} draining",
                    health.healthy,
                    health.total,
                    health.unhealthy,
                    health.draining
                );
            }
        }

        // Skip infrastructure configuration in app-only mode
        if !is_app_only {
            let env_autoscaling = ctx
                .environment
                .config
                .get("autoscaling")
                .and_then(AutoScalingConfig::from_yaml_value);

            if let Some(autoscaling) = env_autoscaling.or_else(|| self.autoscaling_config.clone()) {
                self.configure_autoscaling(&service_name, &autoscaling)
                    .await?;
            }
        } else {
            tracing::info!("App-only mode: skipping infrastructure configuration");
        }

        let mode_msg = if is_app_only { " (app-only)" } else { "" };

        Ok(DeploymentResult::success(format!(
            "Deployed {} to ECS service {} in cluster {}{}",
            image, service_name, self.cluster, mode_msg
        ))
        .with_version(image.clone()))
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let service_name = self.get_service_name(ctx);

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would rollback ECS service {} in cluster {}",
                service_name, self.cluster
            )));
        }

        let status = self.get_service_status(&service_name).await?;

        let task_def_arn = status
            .task_definition
            .ok_or_else(|| anyhow::anyhow!("Service has no task definition"))?;

        let parts: Vec<&str> = task_def_arn.split(':').collect();

        if parts.len() < 2 {
            anyhow::bail!("Invalid task definition ARN format");
        }

        let current_revision: i32 = parts
            .last()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("Could not parse task definition revision"))?;

        if current_revision <= 1 {
            anyhow::bail!("No previous revision to rollback to");
        }

        let previous_revision = current_revision - 1;
        let base_arn = parts[..parts.len() - 1].join(":");
        let previous_task_def = format!("{}:{}", base_arn, previous_revision);

        tracing::info!("Rolling back to task definition: {}", previous_task_def);

        self.ecs_client
            .update_service()
            .cluster(&self.cluster)
            .service(&service_name)
            .task_definition(&previous_task_def)
            .send()
            .await?;

        self.wait_for_stable(&service_name, 600).await?;

        Ok(DeploymentResult::success(format!(
            "Rolled back ECS service {} to revision {}",
            service_name, previous_revision
        )))
    }

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let service_name = self.get_service_name(ctx);
        let status = self.get_service_status(&service_name).await?;

        let mut output = format!(
            "ECS Cluster: {}\n\
             Region: {}\n\
             Service: {} ({})\n\
             Running: {}/{}\n\
             Pending: {}\n\
             Task Definition: {}",
            self.cluster,
            self.region,
            status.name,
            status.status,
            status.running_count,
            status.desired_count,
            status.pending_count,
            status.task_definition.clone().unwrap_or_else(|| "N/A".to_string())
        );

        if let Some(lbs) = &status.load_balancers {
            if !lbs.is_empty() {
                output.push_str("\n\nLoad Balancers:");

                for lb in lbs {
                    if let Some(tg_arn) = &lb.target_group_arn {
                        let health = self.get_target_group_health(tg_arn).await.ok();

                        output.push_str(&format!(
                            "\n  - Target Group: {}",
                            tg_arn.split('/').last().unwrap_or(tg_arn)
                        ));

                        if let Some(h) = health {
                            output.push_str(&format!(
                                "\n    Health: {}/{} healthy",
                                h.healthy, h.total
                            ));
                        }
                    }
                }
            }
        }

        if let Ok(Some(autoscaling)) = self.get_autoscaling_status(&service_name).await {
            output.push_str(&format!(
                "\n\nAuto Scaling:\n  Capacity: {} - {}\n  Policies: {}",
                autoscaling.min_capacity,
                autoscaling.max_capacity,
                if autoscaling.policies.is_empty() {
                    "None".to_string()
                } else {
                    autoscaling.policies.join(", ")
                }
            ));
        }

        Ok(output)
    }

    async fn logs(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        let service_name = self.get_service_name(ctx);
        let log_group = self.get_log_group_name(&service_name);

        if follow {
            let mut args = vec![
                "logs".to_string(),
                "tail".to_string(),
                log_group.clone(),
                "--follow".to_string(),
            ];

            if self.cloudwatch_config.is_some() {
                args.push("--format".to_string());
                args.push("short".to_string());
            }

            let mut child = tokio::process::Command::new("aws")
                .args(&args)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .spawn()?;

            child.wait().await?;
        } else {
            let log_stream_prefix = self
                .cloudwatch_config
                .as_ref()
                .and_then(|c| c.log_stream_prefix.as_deref());

            let events = self
                .get_recent_logs(&log_group, log_stream_prefix, 100)
                .await?;

            for event in events {
                let timestamp = chrono_like_format(event.timestamp);
                println!("[{}] {}: {}", timestamp, event.log_stream, event.message);
            }
        }

        Ok(())
    }
}

fn chrono_like_format(timestamp_ms: i64) -> String {
    let secs = timestamp_ms / 1000;
    let nanos = ((timestamp_ms % 1000) * 1_000_000) as u32;

    if let Some(dt) = std::time::UNIX_EPOCH.checked_add(std::time::Duration::new(secs as u64, nanos))
    {
        let datetime: std::time::SystemTime = dt;
        format!("{:?}", datetime)
    } else {
        format!("{}", timestamp_ms)
    }
}

/// Convert a HashMap of environment variables to ECS KeyValuePair list
fn convert_env_vars_to_key_value_pairs(vars: &HashMap<String, String>) -> Vec<KeyValuePair> {
    vars.iter()
        .map(|(k, v)| {
            KeyValuePair::builder()
                .name(k.clone())
                .value(v.clone())
                .build()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_ecs_provider_config_parsing() {
        use crate::config::InfrastructureConfig;

        let mut config_map = HashMap::new();
        config_map.insert(
            "cluster".to_string(),
            serde_yaml::Value::String("my-cluster".to_string()),
        );
        config_map.insert(
            "region".to_string(),
            serde_yaml::Value::String("us-east-1".to_string()),
        );
        config_map.insert(
            "service_name".to_string(),
            serde_yaml::Value::String("my-service".to_string()),
        );

        let config = InfrastructureConfig {
            infrastructure_type: "aws-ecs".to_string(),
            config: config_map,
        };

        assert!(config.config.contains_key("cluster"));
        assert!(config.config.contains_key("region"));
        assert!(config.config.contains_key("service_name"));
    }

    #[test]
    fn test_load_balancer_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
target_group_arn: arn:aws:elasticloadbalancing:us-east-1:123456789:targetgroup/my-tg/abc123
container_name: app
container_port: 8080
health_check:
  path: /api/health
  interval_seconds: 15
  timeout_seconds: 5
"#,
        )
        .unwrap();

        let config = LoadBalancerConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.target_group_arn.is_some());
        assert_eq!(config.container_port, Some(8080));

        let health = config.health_check.unwrap();
        assert_eq!(health.path, "/api/health");
        assert_eq!(health.interval_seconds, 15);
    }

    #[test]
    fn test_autoscaling_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
enabled: true
min_capacity: 2
max_capacity: 20
target_tracking:
  target_value: 75.0
  metric_type: ECSServiceAverageCPUUtilization
  scale_in_cooldown: 300
  scale_out_cooldown: 60
scheduled:
  - name: scale-up-morning
    schedule: cron(0 8 * * ? *)
    min_capacity: 5
"#,
        )
        .unwrap();

        let config = AutoScalingConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.enabled);
        assert_eq!(config.min_capacity, 2);
        assert_eq!(config.max_capacity, 20);

        let target = config.target_tracking.unwrap();
        assert_eq!(target.target_value, 75.0);
        assert_eq!(target.metric_type, "ECSServiceAverageCPUUtilization");

        assert_eq!(config.scheduled.len(), 1);
        assert_eq!(config.scheduled[0].name, "scale-up-morning");
    }

    #[test]
    fn test_cloudwatch_logs_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
log_group: /ecs/my-service
log_stream_prefix: ecs/app
retention_days: 14
create_log_group: true
"#,
        )
        .unwrap();

        let config = CloudWatchLogsConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.log_group, Some("/ecs/my-service".to_string()));
        assert_eq!(config.retention_days, 14);
        assert!(config.create_log_group);
    }

    #[test]
    fn test_health_check_defaults() {
        let config = HealthCheckConfig::default();

        assert_eq!(config.path, "/health");
        assert_eq!(config.interval_seconds, 30);
        assert_eq!(config.timeout_seconds, 5);
        assert_eq!(config.healthy_threshold, 2);
        assert_eq!(config.unhealthy_threshold, 3);
    }
}
