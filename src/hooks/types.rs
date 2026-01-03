//! Hook type definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::env_vars::EnvVarConfig;

/// When a hook should be executed relative to deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTiming {
    /// Execute before the deployment starts.
    PreDeploy,
    /// Execute after a successful deployment.
    PostDeploy,
    /// Execute when the deployment fails.
    OnFailure,
    /// Execute on successful deployment (alias for PostDeploy).
    OnSuccess,
}

impl HookTiming {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreDeploy => "pre_deploy",
            Self::PostDeploy => "post_deploy",
            Self::OnFailure => "on_failure",
            Self::OnSuccess => "on_success",
        }
    }
}

/// Type of hook to execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookType {
    /// Run a container (Docker-based).
    Container,
    /// Make an HTTP request.
    Http,
    /// Run an ECS task (one-shot).
    EcsTask,
    /// Run a Kubernetes Job (one-shot).
    K8sJob,
    /// Invoke a Lambda function.
    Lambda,
}

impl HookType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Http => "http",
            Self::EcsTask => "ecs_task",
            Self::K8sJob => "k8s_job",
            Self::Lambda => "lambda",
        }
    }
}

/// Configuration for hooks in an environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub pre_deploy: Vec<HookConfig>,

    #[serde(default)]
    pub post_deploy: Vec<HookConfig>,

    #[serde(default)]
    pub on_failure: Vec<HookConfig>,

    #[serde(default)]
    pub on_success: Vec<HookConfig>,
}

impl HooksConfig {
    pub fn get_hooks(&self, timing: HookTiming) -> &[HookConfig] {
        match timing {
            HookTiming::PreDeploy => &self.pre_deploy,
            HookTiming::PostDeploy | HookTiming::OnSuccess => &self.post_deploy,
            HookTiming::OnFailure => &self.on_failure,
        }
    }

    pub fn has_hooks(&self, timing: HookTiming) -> bool {
        !self.get_hooks(timing).is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.pre_deploy.is_empty()
            && self.post_deploy.is_empty()
            && self.on_failure.is_empty()
            && self.on_success.is_empty()
    }
}

/// Configuration for a single hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Hook identifier.
    pub name: String,

    /// Type of hook to execute.
    #[serde(rename = "type")]
    pub hook_type: HookType,

    /// Type-specific configuration.
    #[serde(default)]
    pub config: HookTypeConfig,

    /// Maximum execution time in seconds (default: 300).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,

    /// Whether hook failure should fail the deployment (default: true).
    #[serde(default = "default_fail_on_error")]
    pub fail_on_error: bool,

    /// Optional description for logging.
    #[serde(default)]
    pub description: Option<String>,
}

fn default_timeout() -> u32 {
    300
}

fn default_fail_on_error() -> bool {
    true
}

/// Type-specific hook configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HookTypeConfig {
    /// Container hook configuration.
    #[serde(flatten)]
    pub container: Option<ContainerHookConfig>,

    /// HTTP hook configuration.
    #[serde(flatten)]
    pub http: Option<HttpHookConfig>,

    /// ECS task hook configuration.
    #[serde(flatten)]
    pub ecs_task: Option<EcsTaskHookConfig>,

    /// Kubernetes Job hook configuration.
    #[serde(flatten)]
    pub k8s_job: Option<K8sJobHookConfig>,

    /// Lambda hook configuration.
    #[serde(flatten)]
    pub lambda: Option<LambdaHookConfig>,
}

/// Configuration for container-based hooks (Docker).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerHookConfig {
    /// Container image to run.
    pub image: String,

    /// Command to execute.
    #[serde(default)]
    pub command: Vec<String>,

    /// Entry point override.
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,

    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, EnvVarConfig>,

    /// Working directory inside the container.
    #[serde(default)]
    pub working_dir: Option<String>,

    /// Network mode (host, bridge, none).
    #[serde(default)]
    pub network: Option<String>,

    /// Volume mounts (host:container format).
    #[serde(default)]
    pub volumes: Vec<String>,

    /// Whether to remove the container after execution.
    #[serde(default = "default_cleanup")]
    pub cleanup: bool,
}

fn default_cleanup() -> bool {
    true
}

impl Default for ContainerHookConfig {
    fn default() -> Self {
        Self {
            image: String::new(),
            command: Vec::new(),
            entrypoint: None,
            env: HashMap::new(),
            working_dir: None,
            network: None,
            volumes: Vec::new(),
            cleanup: true,
        }
    }
}

/// Configuration for HTTP webhook hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHookConfig {
    /// URL to make the request to.
    pub url: String,

    /// HTTP method (GET, POST, PUT, DELETE, PATCH).
    #[serde(default = "default_http_method")]
    pub method: String,

    /// Request headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Request body (JSON string or template).
    #[serde(default)]
    pub body: Option<String>,

    /// Content-Type header (default: application/json).
    #[serde(default = "default_content_type")]
    pub content_type: String,

    /// Expected status codes for success (default: 2xx).
    #[serde(default)]
    pub expected_status: Vec<u16>,

    /// Whether to ignore SSL certificate errors.
    #[serde(default)]
    pub insecure: bool,
}

fn default_http_method() -> String {
    "POST".to_string()
}

fn default_content_type() -> String {
    "application/json".to_string()
}

impl Default for HttpHookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: default_http_method(),
            headers: HashMap::new(),
            body: None,
            content_type: default_content_type(),
            expected_status: Vec::new(),
            insecure: false,
        }
    }
}

/// Configuration for ECS task hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcsTaskHookConfig {
    /// Container image (defaults to same as service if not specified).
    #[serde(default)]
    pub image: Option<String>,

    /// Image tag override.
    #[serde(default)]
    pub image_tag: Option<String>,

    /// Command override.
    #[serde(default)]
    pub command: Vec<String>,

    /// Entry point override.
    #[serde(default)]
    pub entrypoint: Option<Vec<String>>,

    /// CPU units (e.g., "256", "512").
    #[serde(default)]
    pub cpu: Option<String>,

    /// Memory in MB (e.g., "512", "1024").
    #[serde(default)]
    pub memory: Option<String>,

    /// Environment variables.
    #[serde(default)]
    pub environment: HashMap<String, EnvVarConfig>,

    /// Task role ARN override.
    #[serde(default)]
    pub task_role_arn: Option<String>,

    /// Execution role ARN override.
    #[serde(default)]
    pub execution_role_arn: Option<String>,

    /// VPC subnet IDs.
    #[serde(default)]
    pub subnets: Vec<String>,

    /// Security group IDs.
    #[serde(default)]
    pub security_groups: Vec<String>,

    /// Whether to assign a public IP.
    #[serde(default)]
    pub assign_public_ip: bool,

    /// ECS cluster name (defaults to environment's cluster).
    #[serde(default)]
    pub cluster: Option<String>,

    /// Launch type (FARGATE or EC2).
    #[serde(default = "default_launch_type")]
    pub launch_type: String,

    /// Capacity provider strategy.
    #[serde(default)]
    pub capacity_provider_strategy: Vec<CapacityProviderStrategyItem>,

    /// AWS region override.
    #[serde(default)]
    pub region: Option<String>,
}

fn default_launch_type() -> String {
    "FARGATE".to_string()
}

impl Default for EcsTaskHookConfig {
    fn default() -> Self {
        Self {
            image: None,
            image_tag: None,
            command: Vec::new(),
            entrypoint: None,
            cpu: None,
            memory: None,
            environment: HashMap::new(),
            task_role_arn: None,
            execution_role_arn: None,
            subnets: Vec::new(),
            security_groups: Vec::new(),
            assign_public_ip: false,
            cluster: None,
            launch_type: default_launch_type(),
            capacity_provider_strategy: Vec::new(),
            region: None,
        }
    }
}

/// ECS capacity provider strategy item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityProviderStrategyItem {
    pub capacity_provider: String,
    #[serde(default)]
    pub weight: u32,
    #[serde(default)]
    pub base: u32,
}

/// Configuration for Kubernetes Job hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sJobHookConfig {
    /// Container image.
    pub image: String,

    /// Image tag override.
    #[serde(default)]
    pub image_tag: Option<String>,

    /// Command array.
    #[serde(default)]
    pub command: Vec<String>,

    /// Argument array.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, EnvVarConfig>,

    /// ConfigMap references for env.
    #[serde(default)]
    pub env_from: Vec<EnvFromSource>,

    /// Resource requests and limits.
    #[serde(default)]
    pub resources: Option<K8sResourceConfig>,

    /// ServiceAccount to use.
    #[serde(default)]
    pub service_account: Option<String>,

    /// Namespace override.
    #[serde(default)]
    pub namespace: Option<String>,

    /// Whether to delete the Job after completion.
    #[serde(default = "default_cleanup")]
    pub cleanup: bool,

    /// Number of retries before marking as failed.
    #[serde(default = "default_backoff_limit")]
    pub backoff_limit: u32,

    /// Restart policy (Never or OnFailure).
    #[serde(default = "default_restart_policy")]
    pub restart_policy: String,

    /// Active deadline seconds.
    #[serde(default)]
    pub active_deadline_seconds: Option<u32>,

    /// TTL seconds after finished.
    #[serde(default)]
    pub ttl_seconds_after_finished: Option<u32>,

    /// Labels to apply to the Job.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Annotations to apply to the Job.
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

fn default_backoff_limit() -> u32 {
    2
}

fn default_restart_policy() -> String {
    "Never".to_string()
}

impl Default for K8sJobHookConfig {
    fn default() -> Self {
        Self {
            image: String::new(),
            image_tag: None,
            command: Vec::new(),
            args: Vec::new(),
            env: HashMap::new(),
            env_from: Vec::new(),
            resources: None,
            service_account: None,
            namespace: None,
            cleanup: true,
            backoff_limit: default_backoff_limit(),
            restart_policy: default_restart_policy(),
            active_deadline_seconds: None,
            ttl_seconds_after_finished: None,
            labels: HashMap::new(),
            annotations: HashMap::new(),
        }
    }
}

/// Source for environment variables from ConfigMaps or Secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvFromSource {
    ConfigMap(String),
    Secret(String),
}

/// Kubernetes resource configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sResourceConfig {
    #[serde(default)]
    pub requests: Option<K8sResources>,

    #[serde(default)]
    pub limits: Option<K8sResources>,
}

/// Kubernetes resource values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sResources {
    #[serde(default)]
    pub cpu: Option<String>,

    #[serde(default)]
    pub memory: Option<String>,
}

/// Configuration for Lambda invocation hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaHookConfig {
    /// ARN of the Lambda function to invoke.
    #[serde(default)]
    pub function_arn: Option<String>,

    /// Whether to invoke the deployed function itself.
    #[serde(default)]
    pub invoke_self: bool,

    /// JSON payload for the invocation.
    #[serde(default)]
    pub payload: Option<String>,

    /// Invocation type (RequestResponse or Event).
    #[serde(default = "default_invocation_type")]
    pub invocation_type: String,

    /// Qualifier (alias or version).
    #[serde(default)]
    pub qualifier: Option<String>,

    /// AWS region override.
    #[serde(default)]
    pub region: Option<String>,
}

fn default_invocation_type() -> String {
    "RequestResponse".to_string()
}

impl Default for LambdaHookConfig {
    fn default() -> Self {
        Self {
            function_arn: None,
            invoke_self: false,
            payload: None,
            invocation_type: default_invocation_type(),
            qualifier: None,
            region: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_timing_as_str() {
        assert_eq!(HookTiming::PreDeploy.as_str(), "pre_deploy");
        assert_eq!(HookTiming::PostDeploy.as_str(), "post_deploy");
        assert_eq!(HookTiming::OnFailure.as_str(), "on_failure");
        assert_eq!(HookTiming::OnSuccess.as_str(), "on_success");
    }

    #[test]
    fn test_hook_type_as_str() {
        assert_eq!(HookType::Container.as_str(), "container");
        assert_eq!(HookType::Http.as_str(), "http");
        assert_eq!(HookType::EcsTask.as_str(), "ecs_task");
        assert_eq!(HookType::K8sJob.as_str(), "k8s_job");
        assert_eq!(HookType::Lambda.as_str(), "lambda");
    }

    #[test]
    fn test_hooks_config_get_hooks() {
        let config = HooksConfig {
            pre_deploy: vec![HookConfig {
                name: "test".to_string(),
                hook_type: HookType::Http,
                config: HookTypeConfig::default(),
                timeout_secs: 60,
                fail_on_error: true,
                description: None,
            }],
            ..Default::default()
        };

        assert_eq!(config.get_hooks(HookTiming::PreDeploy).len(), 1);
        assert_eq!(config.get_hooks(HookTiming::PostDeploy).len(), 0);
        assert!(config.has_hooks(HookTiming::PreDeploy));
        assert!(!config.has_hooks(HookTiming::PostDeploy));
    }

    #[test]
    fn test_hooks_config_is_empty() {
        let empty = HooksConfig::default();
        assert!(empty.is_empty());

        let not_empty = HooksConfig {
            pre_deploy: vec![HookConfig {
                name: "test".to_string(),
                hook_type: HookType::Http,
                config: HookTypeConfig::default(),
                timeout_secs: 60,
                fail_on_error: true,
                description: None,
            }],
            ..Default::default()
        };
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_parse_hook_config() {
        let yaml = r#"
name: run-migrations
type: container
config:
  image: myapp/migrations:latest
  command: ["./migrate.sh"]
timeout_secs: 300
fail_on_error: true
"#;
        let hook: HookConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(hook.name, "run-migrations");
        assert_eq!(hook.hook_type, HookType::Container);
        assert_eq!(hook.timeout_secs, 300);
        assert!(hook.fail_on_error);
    }

    #[test]
    fn test_parse_http_hook_config() {
        let yaml = r#"
name: notify-slack
type: http
config:
  url: https://hooks.slack.com/webhook
  method: POST
  body: '{"text": "Deployment complete"}'
timeout_secs: 30
fail_on_error: false
"#;
        let hook: HookConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(hook.name, "notify-slack");
        assert_eq!(hook.hook_type, HookType::Http);
        assert!(!hook.fail_on_error);
    }

    #[test]
    fn test_parse_full_hooks_config() {
        let yaml = r#"
pre_deploy:
  - name: run-migrations
    type: container
    config:
      image: myapp/migrations:latest
    timeout_secs: 300
    fail_on_error: true

post_deploy:
  - name: smoke-tests
    type: http
    config:
      url: https://api.example.com/health
      method: GET
    timeout_secs: 60
    fail_on_error: false
"#;
        let hooks: HooksConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(hooks.pre_deploy.len(), 1);
        assert_eq!(hooks.post_deploy.len(), 1);
        assert_eq!(hooks.pre_deploy[0].name, "run-migrations");
        assert_eq!(hooks.post_deploy[0].name, "smoke-tests");
    }
}
