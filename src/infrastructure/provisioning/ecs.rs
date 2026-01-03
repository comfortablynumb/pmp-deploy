use serde::{Deserialize, Serialize};

/// Configuration for fully provisioning an ECS cluster, service, and task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsProvisioningConfig {
    /// Cluster configuration
    #[serde(default)]
    pub cluster: Option<EcsClusterConfig>,

    /// Task definition configuration
    pub task: EcsTaskConfig,

    /// Service configuration
    pub service: EcsServiceConfig,
}

impl EcsProvisioningConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(cluster) = &self.cluster {
            cluster.validate()?;
        }

        self.task.validate()?;
        self.service.validate()?;

        Ok(())
    }
}

/// ECS Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsClusterConfig {
    /// Capacity providers (e.g., FARGATE, FARGATE_SPOT, EC2)
    #[serde(default = "default_capacity_providers")]
    pub capacity_providers: Vec<String>,

    /// Default capacity provider strategy
    #[serde(default)]
    pub default_capacity_provider_strategy: Vec<CapacityProviderStrategy>,

    /// Enable Container Insights
    #[serde(default)]
    pub container_insights: bool,

    /// Execute command configuration
    #[serde(default)]
    pub execute_command: Option<ExecuteCommandConfig>,
}

fn default_capacity_providers() -> Vec<String> {
    vec!["FARGATE".to_string()]
}

impl Default for EcsClusterConfig {
    fn default() -> Self {
        Self {
            capacity_providers: default_capacity_providers(),
            default_capacity_provider_strategy: vec![],
            container_insights: false,
            execute_command: None,
        }
    }
}

impl EcsClusterConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.capacity_providers.is_empty() {
            anyhow::bail!("ECS cluster requires at least one capacity provider");
        }

        let valid_providers = ["FARGATE", "FARGATE_SPOT"];
        for provider in &self.capacity_providers {
            if !valid_providers.contains(&provider.as_str()) && !provider.starts_with("arn:") {
                // Allow custom capacity provider ARNs or well-known providers
                tracing::warn!(
                    "Unknown capacity provider: {}. If this is an EC2 capacity provider, ensure it exists.",
                    provider
                );
            }
        }

        Ok(())
    }
}

/// Capacity provider strategy entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityProviderStrategy {
    pub capacity_provider: String,

    #[serde(default = "default_weight")]
    pub weight: i32,

    #[serde(default)]
    pub base: i32,
}

fn default_weight() -> i32 {
    1
}

/// Execute command configuration for ECS Exec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteCommandConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub logging: Option<String>,
}

fn default_true() -> bool {
    true
}

/// ECS Task Definition configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsTaskConfig {
    /// Task family name
    pub family: String,

    /// CPU units (256, 512, 1024, 2048, 4096, 8192, 16384)
    #[serde(default = "default_cpu")]
    pub cpu: String,

    /// Memory in MiB (512, 1024, 2048, ...)
    #[serde(default = "default_memory")]
    pub memory: String,

    /// Execution role ARN (required for Fargate)
    pub execution_role_arn: String,

    /// Task role ARN (optional, for container permissions)
    #[serde(default)]
    pub task_role_arn: Option<String>,

    /// Network mode (awsvpc, bridge, host, none)
    #[serde(default = "default_network_mode")]
    pub network_mode: String,

    /// Requires compatibilities (FARGATE, EC2)
    #[serde(default = "default_requires_compatibilities")]
    pub requires_compatibilities: Vec<String>,

    /// Container definitions
    #[serde(default)]
    pub containers: Vec<ContainerDefinition>,

    /// Volumes
    #[serde(default)]
    pub volumes: Vec<VolumeDefinition>,

    /// Runtime platform (for Fargate)
    #[serde(default)]
    pub runtime_platform: Option<RuntimePlatform>,
}

fn default_cpu() -> String {
    "256".to_string()
}

fn default_memory() -> String {
    "512".to_string()
}

fn default_network_mode() -> String {
    "awsvpc".to_string()
}

fn default_requires_compatibilities() -> Vec<String> {
    vec!["FARGATE".to_string()]
}

impl EcsTaskConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.family.is_empty() {
            anyhow::bail!("ECS task family is required");
        }

        if self.execution_role_arn.is_empty() {
            anyhow::bail!("ECS task execution_role_arn is required");
        }

        let valid_cpu = ["256", "512", "1024", "2048", "4096", "8192", "16384"];
        if !valid_cpu.contains(&self.cpu.as_str()) {
            anyhow::bail!(
                "ECS task cpu must be one of: {}",
                valid_cpu.join(", ")
            );
        }

        Ok(())
    }
}

/// Container definition for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDefinition {
    /// Container name
    pub name: String,

    /// Container image
    pub image: String,

    /// Port mappings
    #[serde(default)]
    pub port_mappings: Vec<PortMapping>,

    /// CPU units for this container
    #[serde(default)]
    pub cpu: Option<i32>,

    /// Memory in MiB for this container
    #[serde(default)]
    pub memory: Option<i32>,

    /// Memory reservation (soft limit)
    #[serde(default)]
    pub memory_reservation: Option<i32>,

    /// Essential flag
    #[serde(default = "default_true")]
    pub essential: bool,

    /// Environment variables
    #[serde(default)]
    pub environment: Vec<EnvironmentVariable>,

    /// Secrets from SSM or Secrets Manager
    #[serde(default)]
    pub secrets: Vec<SecretEnvironmentVariable>,

    /// Log configuration
    #[serde(default)]
    pub log_configuration: Option<LogConfiguration>,

    /// Health check
    #[serde(default)]
    pub health_check: Option<ContainerHealthCheck>,

    /// Command override
    #[serde(default)]
    pub command: Vec<String>,

    /// Entry point override
    #[serde(default)]
    pub entry_point: Vec<String>,

    /// Working directory
    #[serde(default)]
    pub working_directory: Option<String>,

    /// Mount points
    #[serde(default)]
    pub mount_points: Vec<MountPoint>,

    /// Depends on other containers
    #[serde(default)]
    pub depends_on: Vec<ContainerDependency>,
}

/// Port mapping for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub container_port: i32,

    #[serde(default)]
    pub host_port: Option<i32>,

    #[serde(default = "default_protocol")]
    pub protocol: String,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub app_protocol: Option<String>,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

/// Environment variable for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

/// Secret environment variable (from SSM or Secrets Manager)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEnvironmentVariable {
    pub name: String,
    pub value_from: String,
}

/// Log configuration for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfiguration {
    #[serde(default = "default_log_driver")]
    pub log_driver: String,

    #[serde(default)]
    pub options: std::collections::HashMap<String, String>,
}

fn default_log_driver() -> String {
    "awslogs".to_string()
}

/// Health check for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerHealthCheck {
    pub command: Vec<String>,

    #[serde(default = "default_health_interval")]
    pub interval: i32,

    #[serde(default = "default_health_timeout")]
    pub timeout: i32,

    #[serde(default = "default_health_retries")]
    pub retries: i32,

    #[serde(default = "default_health_start_period")]
    pub start_period: i32,
}

fn default_health_interval() -> i32 {
    30
}

fn default_health_timeout() -> i32 {
    5
}

fn default_health_retries() -> i32 {
    3
}

fn default_health_start_period() -> i32 {
    0
}

/// Mount point for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    pub source_volume: String,
    pub container_path: String,

    #[serde(default)]
    pub read_only: bool,
}

/// Container dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDependency {
    pub container_name: String,
    pub condition: String,
}

/// Volume definition for a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeDefinition {
    pub name: String,

    #[serde(default)]
    pub efs_volume_configuration: Option<EfsVolumeConfiguration>,
}

/// EFS volume configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfsVolumeConfiguration {
    pub file_system_id: String,

    #[serde(default)]
    pub root_directory: Option<String>,

    #[serde(default)]
    pub transit_encryption: Option<String>,

    #[serde(default)]
    pub authorization_config: Option<EfsAuthorizationConfig>,
}

/// EFS authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfsAuthorizationConfig {
    #[serde(default)]
    pub access_point_id: Option<String>,

    #[serde(default)]
    pub iam: Option<String>,
}

/// Runtime platform for Fargate tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePlatform {
    #[serde(default = "default_operating_system")]
    pub operating_system_family: String,

    #[serde(default)]
    pub cpu_architecture: Option<String>,
}

fn default_operating_system() -> String {
    "LINUX".to_string()
}

/// ECS Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsServiceConfig {
    /// Desired number of tasks
    #[serde(default = "default_desired_count")]
    pub desired_count: i32,

    /// Launch type (FARGATE, EC2, EXTERNAL)
    #[serde(default = "default_launch_type")]
    pub launch_type: Option<String>,

    /// Capacity provider strategy (alternative to launch_type)
    #[serde(default)]
    pub capacity_provider_strategy: Vec<CapacityProviderStrategy>,

    /// Platform version (for Fargate)
    #[serde(default = "default_platform_version")]
    pub platform_version: String,

    /// Deployment configuration
    #[serde(default)]
    pub deployment: Option<DeploymentConfiguration>,

    /// Network configuration (required for awsvpc mode)
    #[serde(default)]
    pub network: Option<EcsNetworkConfig>,

    /// Load balancer configuration
    #[serde(default)]
    pub load_balancers: Vec<ServiceLoadBalancer>,

    /// Service registries (Cloud Map)
    #[serde(default)]
    pub service_registries: Vec<ServiceRegistry>,

    /// Enable ECS Exec
    #[serde(default)]
    pub enable_execute_command: bool,

    /// Enable circuit breaker
    #[serde(default)]
    pub enable_circuit_breaker: bool,

    /// Propagate tags from task definition or service
    #[serde(default)]
    pub propagate_tags: Option<String>,

    /// Health check grace period (seconds)
    #[serde(default)]
    pub health_check_grace_period_seconds: Option<i32>,

    /// Scheduling strategy (REPLICA or DAEMON)
    #[serde(default = "default_scheduling_strategy")]
    pub scheduling_strategy: String,
}

fn default_desired_count() -> i32 {
    1
}

fn default_launch_type() -> Option<String> {
    Some("FARGATE".to_string())
}

fn default_platform_version() -> String {
    "LATEST".to_string()
}

fn default_scheduling_strategy() -> String {
    "REPLICA".to_string()
}

impl EcsServiceConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.desired_count < 0 {
            anyhow::bail!("ECS service desired_count must be non-negative");
        }

        if let Some(launch_type) = &self.launch_type {
            let valid_launch_types = ["FARGATE", "EC2", "EXTERNAL"];
            if !valid_launch_types.contains(&launch_type.as_str()) {
                anyhow::bail!(
                    "ECS service launch_type must be one of: {}",
                    valid_launch_types.join(", ")
                );
            }
        }

        if let Some(network) = &self.network {
            network.validate()?;
        }

        Ok(())
    }
}

/// Deployment configuration for ECS service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfiguration {
    #[serde(default = "default_minimum_healthy_percent")]
    pub minimum_healthy_percent: i32,

    #[serde(default = "default_maximum_percent")]
    pub maximum_percent: i32,
}

fn default_minimum_healthy_percent() -> i32 {
    100
}

fn default_maximum_percent() -> i32 {
    200
}

/// Network configuration for ECS service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsNetworkConfig {
    /// Subnet IDs
    pub subnets: Vec<String>,

    /// Security group IDs
    pub security_groups: Vec<String>,

    /// Assign public IP (for Fargate)
    #[serde(default)]
    pub assign_public_ip: bool,
}

impl EcsNetworkConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.subnets.is_empty() {
            anyhow::bail!("ECS network configuration requires at least one subnet");
        }

        if self.security_groups.is_empty() {
            anyhow::bail!("ECS network configuration requires at least one security group");
        }

        Ok(())
    }
}

/// Load balancer configuration for ECS service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceLoadBalancer {
    pub target_group_arn: String,
    pub container_name: String,
    pub container_port: i32,
}

/// Service registry (Cloud Map) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistry {
    pub registry_arn: String,

    #[serde(default)]
    pub container_name: Option<String>,

    #[serde(default)]
    pub container_port: Option<i32>,
}

/// Current state of ECS resources for comparison
#[derive(Debug, Clone)]
pub struct EcsCurrentState {
    pub cluster_exists: bool,
    pub service_exists: bool,
    pub task_definition_arn: Option<String>,
    pub cluster_arn: Option<String>,
    pub service_arn: Option<String>,
    pub current_desired_count: Option<i32>,
    pub current_task_definition: Option<String>,
}

impl EcsCurrentState {
    pub fn not_found() -> Self {
        Self {
            cluster_exists: false,
            service_exists: false,
            task_definition_arn: None,
            cluster_arn: None,
            service_arn: None,
            current_desired_count: None,
            current_task_definition: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecs_cluster_config_defaults() {
        let config = EcsClusterConfig::default();

        assert_eq!(config.capacity_providers, vec!["FARGATE"]);
        assert!(!config.container_insights);
    }

    #[test]
    fn test_ecs_cluster_config_validation() {
        let mut config = EcsClusterConfig::default();
        assert!(config.validate().is_ok());

        config.capacity_providers = vec![];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_ecs_task_config_validation() {
        let config = EcsTaskConfig {
            family: "my-task".to_string(),
            cpu: "256".to_string(),
            memory: "512".to_string(),
            execution_role_arn: "arn:aws:iam::123456789:role/ecs-exec".to_string(),
            task_role_arn: None,
            network_mode: "awsvpc".to_string(),
            requires_compatibilities: vec!["FARGATE".to_string()],
            containers: vec![],
            volumes: vec![],
            runtime_platform: None,
        };

        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.family = String::new();
        assert!(invalid_config.validate().is_err());

        let mut invalid_config = config.clone();
        invalid_config.cpu = "100".to_string();
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_ecs_service_config_validation() {
        let config = EcsServiceConfig {
            desired_count: 2,
            launch_type: Some("FARGATE".to_string()),
            capacity_provider_strategy: vec![],
            platform_version: "LATEST".to_string(),
            deployment: None,
            network: Some(EcsNetworkConfig {
                subnets: vec!["subnet-123".to_string()],
                security_groups: vec!["sg-456".to_string()],
                assign_public_ip: true,
            }),
            load_balancers: vec![],
            service_registries: vec![],
            enable_execute_command: false,
            enable_circuit_breaker: false,
            propagate_tags: None,
            health_check_grace_period_seconds: None,
            scheduling_strategy: "REPLICA".to_string(),
        };

        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.desired_count = -1;
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_ecs_network_config_validation() {
        let config = EcsNetworkConfig {
            subnets: vec![],
            security_groups: vec!["sg-123".to_string()],
            assign_public_ip: false,
        };
        assert!(config.validate().is_err());

        let config = EcsNetworkConfig {
            subnets: vec!["subnet-123".to_string()],
            security_groups: vec![],
            assign_public_ip: false,
        };
        assert!(config.validate().is_err());

        let config = EcsNetworkConfig {
            subnets: vec!["subnet-123".to_string()],
            security_groups: vec!["sg-123".to_string()],
            assign_public_ip: true,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_from_yaml_value() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
cluster:
  capacity_providers:
    - FARGATE
    - FARGATE_SPOT
  container_insights: true
task:
  family: my-app
  cpu: "512"
  memory: "1024"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
  task_role_arn: arn:aws:iam::123456789:role/ecs-task
service:
  desired_count: 3
  network:
    subnets:
      - subnet-abc123
    security_groups:
      - sg-xyz789
    assign_public_ip: true
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.cluster.is_some());
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.capacity_providers.len(), 2);
        assert!(cluster.container_insights);

        assert_eq!(config.task.family, "my-app");
        assert_eq!(config.task.cpu, "512");

        assert_eq!(config.service.desired_count, 3);
        assert!(config.service.network.is_some());
    }

    #[test]
    fn test_ecs_current_state_not_found() {
        let state = EcsCurrentState::not_found();

        assert!(!state.cluster_exists);
        assert!(!state.service_exists);
        assert!(state.task_definition_arn.is_none());
        assert!(state.cluster_arn.is_none());
        assert!(state.service_arn.is_none());
        assert!(state.current_desired_count.is_none());
    }

    #[test]
    fn test_ecs_task_config_cpu_validation() {
        let valid_cpus = ["256", "512", "1024", "2048", "4096", "8192", "16384"];

        for cpu in valid_cpus {
            let config = EcsTaskConfig {
                family: "test".to_string(),
                cpu: cpu.to_string(),
                memory: "512".to_string(),
                execution_role_arn: "arn:aws:iam::123456789:role/ecs-exec".to_string(),
                task_role_arn: None,
                network_mode: "awsvpc".to_string(),
                requires_compatibilities: vec![],
                containers: vec![],
                volumes: vec![],
                runtime_platform: None,
            };
            assert!(config.validate().is_ok(), "CPU {} should be valid", cpu);
        }

        let invalid_cpus = ["100", "300", "0", "999"];

        for cpu in invalid_cpus {
            let config = EcsTaskConfig {
                family: "test".to_string(),
                cpu: cpu.to_string(),
                memory: "512".to_string(),
                execution_role_arn: "arn:aws:iam::123456789:role/ecs-exec".to_string(),
                task_role_arn: None,
                network_mode: "awsvpc".to_string(),
                requires_compatibilities: vec![],
                containers: vec![],
                volumes: vec![],
                runtime_platform: None,
            };
            assert!(config.validate().is_err(), "CPU {} should be invalid", cpu);
        }
    }

    #[test]
    fn test_ecs_service_launch_type_validation() {
        let valid_types = ["FARGATE", "EC2", "EXTERNAL"];

        for lt in valid_types {
            let config = EcsServiceConfig {
                desired_count: 1,
                launch_type: Some(lt.to_string()),
                capacity_provider_strategy: vec![],
                platform_version: "LATEST".to_string(),
                deployment: None,
                network: None,
                load_balancers: vec![],
                service_registries: vec![],
                enable_execute_command: false,
                enable_circuit_breaker: false,
                propagate_tags: None,
                health_check_grace_period_seconds: None,
                scheduling_strategy: "REPLICA".to_string(),
            };
            assert!(config.validate().is_ok(), "Launch type {} should be valid", lt);
        }

        let config = EcsServiceConfig {
            desired_count: 1,
            launch_type: Some("INVALID".to_string()),
            capacity_provider_strategy: vec![],
            platform_version: "LATEST".to_string(),
            deployment: None,
            network: None,
            load_balancers: vec![],
            service_registries: vec![],
            enable_execute_command: false,
            enable_circuit_breaker: false,
            propagate_tags: None,
            health_check_grace_period_seconds: None,
            scheduling_strategy: "REPLICA".to_string(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_ecs_provisioning_config_validation() {
        let config = EcsProvisioningConfig {
            cluster: Some(EcsClusterConfig::default()),
            task: EcsTaskConfig {
                family: "test".to_string(),
                cpu: "256".to_string(),
                memory: "512".to_string(),
                execution_role_arn: "arn:aws:iam::123456789:role/ecs-exec".to_string(),
                task_role_arn: None,
                network_mode: "awsvpc".to_string(),
                requires_compatibilities: vec![],
                containers: vec![],
                volumes: vec![],
                runtime_platform: None,
            },
            service: EcsServiceConfig {
                desired_count: 1,
                launch_type: Some("FARGATE".to_string()),
                capacity_provider_strategy: vec![],
                platform_version: "LATEST".to_string(),
                deployment: None,
                network: Some(EcsNetworkConfig {
                    subnets: vec!["subnet-123".to_string()],
                    security_groups: vec!["sg-456".to_string()],
                    assign_public_ip: true,
                }),
                load_balancers: vec![],
                service_registries: vec![],
                enable_execute_command: false,
                enable_circuit_breaker: false,
                propagate_tags: None,
                health_check_grace_period_seconds: None,
                scheduling_strategy: "REPLICA".to_string(),
            },
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_ecs_container_definition_parsing() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
  containers:
    - name: app
      image: my-app:latest
      essential: true
      port_mappings:
        - container_port: 8080
          protocol: tcp
      environment:
        - name: ENV
          value: production
      secrets:
        - name: DB_PASSWORD
          value_from: arn:aws:secretsmanager:us-east-1:123:secret:db-pass
service:
  desired_count: 2
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.task.containers.len(), 1);

        let container = &config.task.containers[0];
        assert_eq!(container.name, "app");
        assert!(container.essential);
        assert_eq!(container.port_mappings.len(), 1);
        assert_eq!(container.port_mappings[0].container_port, 8080);
        assert_eq!(container.environment.len(), 1);
        assert_eq!(container.secrets.len(), 1);
    }

    #[test]
    fn test_ecs_load_balancer_config() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
service:
  desired_count: 2
  load_balancers:
    - target_group_arn: arn:aws:elasticloadbalancing:us-east-1:123:targetgroup/tg/abc
      container_name: app
      container_port: 8080
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.service.load_balancers.len(), 1);

        let lb = &config.service.load_balancers[0];
        assert_eq!(lb.container_name, "app");
        assert_eq!(lb.container_port, 8080);
    }

    #[test]
    fn test_ecs_capacity_provider_strategy() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
cluster:
  capacity_providers:
    - FARGATE
    - FARGATE_SPOT
  default_capacity_provider_strategy:
    - capacity_provider: FARGATE
      weight: 1
      base: 1
    - capacity_provider: FARGATE_SPOT
      weight: 4
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
service:
  desired_count: 5
  capacity_provider_strategy:
    - capacity_provider: FARGATE
      weight: 1
      base: 1
    - capacity_provider: FARGATE_SPOT
      weight: 4
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.default_capacity_provider_strategy.len(), 2);
        assert_eq!(cluster.default_capacity_provider_strategy[0].base, 1);
        assert_eq!(cluster.default_capacity_provider_strategy[1].weight, 4);

        assert_eq!(config.service.capacity_provider_strategy.len(), 2);
    }

    #[test]
    fn test_ecs_deployment_configuration() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
service:
  desired_count: 2
  deployment:
    minimum_healthy_percent: 50
    maximum_percent: 200
  enable_circuit_breaker: true
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.service.deployment.is_some());
        let deployment = config.service.deployment.unwrap();
        assert_eq!(deployment.minimum_healthy_percent, 50);
        assert_eq!(deployment.maximum_percent, 200);
        assert!(config.service.enable_circuit_breaker);
    }

    #[test]
    fn test_ecs_runtime_platform() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
  runtime_platform:
    operating_system_family: LINUX
    cpu_architecture: ARM64
service:
  desired_count: 1
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.task.runtime_platform.is_some());
        let platform = config.task.runtime_platform.unwrap();
        assert_eq!(platform.operating_system_family, "LINUX");
        assert_eq!(platform.cpu_architecture, Some("ARM64".to_string()));
    }

    #[test]
    fn test_ecs_service_registry() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
service:
  desired_count: 1
  service_registries:
    - registry_arn: arn:aws:servicediscovery:us-east-1:123:service/srv-abc
      container_name: app
      container_port: 8080
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.service.service_registries.len(), 1);

        let registry = &config.service.service_registries[0];
        assert_eq!(registry.container_name, Some("app".to_string()));
        assert_eq!(registry.container_port, Some(8080));
    }

    #[test]
    fn test_ecs_log_configuration() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
  containers:
    - name: app
      image: my-app:latest
      log_configuration:
        log_driver: awslogs
        options:
          awslogs-group: /ecs/my-app
          awslogs-region: us-east-1
          awslogs-stream-prefix: ecs
service:
  desired_count: 1
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        let container = &config.task.containers[0];
        assert!(container.log_configuration.is_some());

        let log_config = container.log_configuration.as_ref().unwrap();
        assert_eq!(log_config.log_driver, "awslogs");
        assert_eq!(log_config.options.get("awslogs-group"), Some(&"/ecs/my-app".to_string()));
    }

    #[test]
    fn test_ecs_health_check() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
task:
  family: my-app
  cpu: "256"
  memory: "512"
  execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
  containers:
    - name: app
      image: my-app:latest
      health_check:
        command:
          - CMD-SHELL
          - curl -f http://localhost:8080/health || exit 1
        interval: 30
        timeout: 5
        retries: 3
        start_period: 60
service:
  desired_count: 1
"#,
        )
        .unwrap();

        let config = EcsProvisioningConfig::from_yaml_value(&yaml).unwrap();

        let container = &config.task.containers[0];
        assert!(container.health_check.is_some());

        let health_check = container.health_check.as_ref().unwrap();
        assert_eq!(health_check.interval, 30);
        assert_eq!(health_check.timeout, 5);
        assert_eq!(health_check.retries, 3);
        assert_eq!(health_check.start_period, 60);
    }
}
