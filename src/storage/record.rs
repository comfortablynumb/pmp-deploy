use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Phases of a deployment that can be checkpointed for resumption
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentPhase {
    /// Running pre-deployment hooks
    PreHooks,
    /// Provisioning or updating infrastructure
    InfrastructureProvisioning,
    /// Deploying the application
    AppDeployment,
    /// Running health checks
    HealthCheck,
    /// Running post-deployment hooks
    PostHooks,
    /// Deployment completed
    Completed,
}

impl DeploymentPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentPhase::PreHooks => "pre_hooks",
            DeploymentPhase::InfrastructureProvisioning => "infrastructure_provisioning",
            DeploymentPhase::AppDeployment => "app_deployment",
            DeploymentPhase::HealthCheck => "health_check",
            DeploymentPhase::PostHooks => "post_hooks",
            DeploymentPhase::Completed => "completed",
        }
    }

    pub fn next(&self) -> Option<Self> {
        match self {
            DeploymentPhase::PreHooks => Some(DeploymentPhase::InfrastructureProvisioning),
            DeploymentPhase::InfrastructureProvisioning => Some(DeploymentPhase::AppDeployment),
            DeploymentPhase::AppDeployment => Some(DeploymentPhase::HealthCheck),
            DeploymentPhase::HealthCheck => Some(DeploymentPhase::PostHooks),
            DeploymentPhase::PostHooks => Some(DeploymentPhase::Completed),
            DeploymentPhase::Completed => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, DeploymentPhase::Completed)
    }

    /// Get the ordinal value for phase ordering/comparison.
    pub fn ordinal(&self) -> u8 {
        match self {
            DeploymentPhase::PreHooks => 0,
            DeploymentPhase::InfrastructureProvisioning => 1,
            DeploymentPhase::AppDeployment => 2,
            DeploymentPhase::HealthCheck => 3,
            DeploymentPhase::PostHooks => 4,
            DeploymentPhase::Completed => 5,
        }
    }

    pub fn all_phases() -> Vec<Self> {
        vec![
            DeploymentPhase::PreHooks,
            DeploymentPhase::InfrastructureProvisioning,
            DeploymentPhase::AppDeployment,
            DeploymentPhase::HealthCheck,
            DeploymentPhase::PostHooks,
        ]
    }
}

impl std::fmt::Display for DeploymentPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A resource that was deployed during a deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedResource {
    /// Type of resource (e.g., "ConfigMap", "Deployment", "Service", "Lambda")
    pub resource_type: String,
    /// Name of the resource
    pub resource_name: String,
    /// Namespace (for Kubernetes resources)
    pub namespace: Option<String>,
    /// When the resource was deployed
    pub deployed_at: SystemTime,
    /// Additional metadata about the resource
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl DeployedResource {
    pub fn new(resource_type: impl Into<String>, resource_name: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_name: resource_name.into(),
            namespace: None,
            deployed_at: SystemTime::now(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Checkpoint data for resumable deployments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCheckpoint {
    /// ID of the deployment this checkpoint belongs to
    pub deployment_id: String,
    /// Current phase of the deployment
    pub phase: DeploymentPhase,
    /// Phases that have been completed
    pub completed_phases: Vec<DeploymentPhase>,
    /// Resources that have been successfully deployed
    pub deployed_resources: Vec<DeployedResource>,
    /// Names of pre-deployment hooks that have completed
    pub pre_hooks_completed: Vec<String>,
    /// Names of post-deployment hooks that have completed
    pub post_hooks_completed: Vec<String>,
    /// When this checkpoint was last updated
    pub last_updated: SystemTime,
    /// Serialized context snapshot for resumption (JSON)
    pub context_snapshot: Option<String>,
    /// Error message if the deployment failed at this checkpoint
    pub error_message: Option<String>,
}

impl DeploymentCheckpoint {
    pub fn new(deployment_id: impl Into<String>) -> Self {
        Self {
            deployment_id: deployment_id.into(),
            phase: DeploymentPhase::PreHooks,
            completed_phases: Vec::new(),
            deployed_resources: Vec::new(),
            pre_hooks_completed: Vec::new(),
            post_hooks_completed: Vec::new(),
            last_updated: SystemTime::now(),
            context_snapshot: None,
            error_message: None,
        }
    }

    pub fn with_context_snapshot(mut self, snapshot: impl Into<String>) -> Self {
        self.context_snapshot = Some(snapshot.into());
        self
    }

    /// Advance to the next phase
    pub fn advance_phase(&mut self) {
        if let Some(next) = self.phase.next() {
            self.completed_phases.push(self.phase.clone());
            self.phase = next;
            self.last_updated = SystemTime::now();
        }
    }

    /// Set the current phase directly
    pub fn set_phase(&mut self, phase: DeploymentPhase) {
        self.phase = phase;
        self.last_updated = SystemTime::now();
    }

    /// Add a deployed resource to the checkpoint
    pub fn add_resource(&mut self, resource: DeployedResource) {
        self.deployed_resources.push(resource);
        self.last_updated = SystemTime::now();
    }

    /// Mark a pre-deployment hook as completed
    pub fn mark_pre_hook_completed(&mut self, hook_name: impl Into<String>) {
        self.pre_hooks_completed.push(hook_name.into());
        self.last_updated = SystemTime::now();
    }

    /// Mark a post-deployment hook as completed
    pub fn mark_post_hook_completed(&mut self, hook_name: impl Into<String>) {
        self.post_hooks_completed.push(hook_name.into());
        self.last_updated = SystemTime::now();
    }

    /// Set an error message
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error_message = Some(message.into());
        self.last_updated = SystemTime::now();
    }

    /// Check if this deployment can be resumed
    pub fn can_resume(&self) -> bool {
        !self.phase.is_terminal()
    }

    /// Check if a specific pre-hook has been completed
    pub fn is_pre_hook_completed(&self, hook_name: &str) -> bool {
        self.pre_hooks_completed.iter().any(|h| h == hook_name)
    }

    /// Check if a specific post-hook has been completed
    pub fn is_post_hook_completed(&self, hook_name: &str) -> bool {
        self.post_hooks_completed.iter().any(|h| h == hook_name)
    }

    /// Get the age of this checkpoint
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_updated)
            .unwrap_or_default()
    }
}

fn generate_checkpoint_id() -> String {
    format!("chk_{}", uuid::Uuid::new_v4().simple())
}

/// Status of a deployment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    /// Deployment is in progress
    InProgress,
    /// Deployment completed successfully
    Success,
    /// Deployment failed
    Failed,
    /// Deployment was rolled back
    RolledBack,
}

impl DeploymentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DeploymentStatus::Success
                | DeploymentStatus::Failed
                | DeploymentStatus::RolledBack
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentStatus::InProgress => "in_progress",
            DeploymentStatus::Success => "success",
            DeploymentStatus::Failed => "failed",
            DeploymentStatus::RolledBack => "rolled_back",
        }
    }
}

/// A record of a deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    /// Unique identifier for this deployment
    pub id: String,

    /// Project name or path
    pub project: String,

    /// Environment name (e.g., "production", "staging")
    pub environment: String,

    /// Infrastructure type (e.g., "aws-ecs", "kubernetes")
    pub infrastructure_type: String,

    /// Image that was deployed
    pub image: Option<String>,

    /// Previous image (for rollback reference)
    pub previous_image: Option<String>,

    /// Deployment status
    pub status: DeploymentStatus,

    /// Human-readable message
    pub message: String,

    /// Deploy mode used (full or app-only)
    pub deploy_mode: String,

    /// Timestamp when deployment started
    pub started_at: SystemTime,

    /// Timestamp when deployment completed (if terminal)
    pub completed_at: Option<SystemTime>,

    /// Duration of the deployment
    pub duration_secs: Option<u64>,

    /// User or system that triggered the deployment
    pub triggered_by: Option<String>,

    /// Whether this was a dry run
    pub dry_run: bool,

    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl DeploymentRecord {
    pub fn new(
        project: impl Into<String>,
        environment: impl Into<String>,
        infrastructure_type: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_id(),
            project: project.into(),
            environment: environment.into(),
            infrastructure_type: infrastructure_type.into(),
            image: None,
            previous_image: None,
            status: DeploymentStatus::InProgress,
            message: String::new(),
            deploy_mode: "app-only".to_string(),
            started_at: SystemTime::now(),
            completed_at: None,
            duration_secs: None,
            triggered_by: None,
            dry_run: false,
            metadata: HashMap::new(),
        }
    }

    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = Some(image.into());
        self
    }

    pub fn with_previous_image(mut self, image: impl Into<String>) -> Self {
        self.previous_image = Some(image.into());
        self
    }

    pub fn with_deploy_mode(mut self, mode: impl Into<String>) -> Self {
        self.deploy_mode = mode.into();
        self
    }

    pub fn with_triggered_by(mut self, user: impl Into<String>) -> Self {
        self.triggered_by = Some(user.into());
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Mark the deployment as successful
    pub fn success(mut self, message: impl Into<String>) -> Self {
        self.status = DeploymentStatus::Success;
        self.message = message.into();
        self.complete()
    }

    /// Mark the deployment as failed
    pub fn failed(mut self, message: impl Into<String>) -> Self {
        self.status = DeploymentStatus::Failed;
        self.message = message.into();
        self.complete()
    }

    /// Mark the deployment as rolled back
    pub fn rolled_back(mut self, message: impl Into<String>) -> Self {
        self.status = DeploymentStatus::RolledBack;
        self.message = message.into();
        self.complete()
    }

    fn complete(mut self) -> Self {
        let now = SystemTime::now();
        self.completed_at = Some(now);

        if let Ok(duration) = now.duration_since(self.started_at) {
            self.duration_secs = Some(duration.as_secs());
        }

        self
    }

    /// Get the age of this record
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.started_at)
            .unwrap_or_default()
    }
}

fn generate_id() -> String {
    format!("dep_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_status_is_terminal() {
        assert!(!DeploymentStatus::InProgress.is_terminal());
        assert!(DeploymentStatus::Success.is_terminal());
        assert!(DeploymentStatus::Failed.is_terminal());
        assert!(DeploymentStatus::RolledBack.is_terminal());
    }

    #[test]
    fn test_deployment_record_new() {
        let record = DeploymentRecord::new("my-project", "production", "aws-ecs");

        assert!(!record.id.is_empty());
        assert_eq!(record.project, "my-project");
        assert_eq!(record.environment, "production");
        assert_eq!(record.infrastructure_type, "aws-ecs");
        assert_eq!(record.status, DeploymentStatus::InProgress);
    }

    #[test]
    fn test_deployment_record_success() {
        let record = DeploymentRecord::new("my-project", "production", "aws-ecs")
            .with_image("myapp:v1.0.0")
            .success("Deployment completed");

        assert_eq!(record.status, DeploymentStatus::Success);
        assert!(record.completed_at.is_some());
        assert!(record.duration_secs.is_some());
    }

    #[test]
    fn test_deployment_record_builder() {
        let record = DeploymentRecord::new("my-project", "staging", "kubernetes")
            .with_image("myapp:v2.0.0")
            .with_previous_image("myapp:v1.9.0")
            .with_deploy_mode("full")
            .with_triggered_by("user@example.com")
            .with_dry_run(false)
            .with_metadata("commit", "abc123");

        assert_eq!(record.image, Some("myapp:v2.0.0".to_string()));
        assert_eq!(record.previous_image, Some("myapp:v1.9.0".to_string()));
        assert_eq!(record.deploy_mode, "full");
        assert_eq!(
            record.triggered_by,
            Some("user@example.com".to_string())
        );
        assert!(!record.dry_run);
        assert_eq!(record.metadata.get("commit"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_generate_id() {
        let id1 = generate_id();
        let id2 = generate_id();

        assert!(id1.starts_with("dep_"));
        assert!(id2.starts_with("dep_"));
        // IDs should be unique (different random part)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deployment_phase_next() {
        assert_eq!(
            DeploymentPhase::PreHooks.next(),
            Some(DeploymentPhase::InfrastructureProvisioning)
        );
        assert_eq!(
            DeploymentPhase::InfrastructureProvisioning.next(),
            Some(DeploymentPhase::AppDeployment)
        );
        assert_eq!(
            DeploymentPhase::AppDeployment.next(),
            Some(DeploymentPhase::HealthCheck)
        );
        assert_eq!(
            DeploymentPhase::HealthCheck.next(),
            Some(DeploymentPhase::PostHooks)
        );
        assert_eq!(
            DeploymentPhase::PostHooks.next(),
            Some(DeploymentPhase::Completed)
        );
        assert_eq!(DeploymentPhase::Completed.next(), None);
    }

    #[test]
    fn test_deployment_phase_is_terminal() {
        assert!(!DeploymentPhase::PreHooks.is_terminal());
        assert!(!DeploymentPhase::InfrastructureProvisioning.is_terminal());
        assert!(!DeploymentPhase::AppDeployment.is_terminal());
        assert!(!DeploymentPhase::HealthCheck.is_terminal());
        assert!(!DeploymentPhase::PostHooks.is_terminal());
        assert!(DeploymentPhase::Completed.is_terminal());
    }

    #[test]
    fn test_deployed_resource_builder() {
        let resource = DeployedResource::new("Deployment", "my-app")
            .with_namespace("production")
            .with_metadata("version", "v1.0.0");

        assert_eq!(resource.resource_type, "Deployment");
        assert_eq!(resource.resource_name, "my-app");
        assert_eq!(resource.namespace, Some("production".to_string()));
        assert_eq!(
            resource.metadata.get("version"),
            Some(&"v1.0.0".to_string())
        );
    }

    #[test]
    fn test_checkpoint_new() {
        let checkpoint = DeploymentCheckpoint::new("dep_123");

        assert_eq!(checkpoint.deployment_id, "dep_123");
        assert_eq!(checkpoint.phase, DeploymentPhase::PreHooks);
        assert!(checkpoint.completed_phases.is_empty());
        assert!(checkpoint.deployed_resources.is_empty());
        assert!(checkpoint.can_resume());
    }

    #[test]
    fn test_checkpoint_advance_phase() {
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");

        checkpoint.advance_phase();
        assert_eq!(checkpoint.phase, DeploymentPhase::InfrastructureProvisioning);
        assert_eq!(checkpoint.completed_phases.len(), 1);
        assert_eq!(checkpoint.completed_phases[0], DeploymentPhase::PreHooks);

        checkpoint.advance_phase();
        assert_eq!(checkpoint.phase, DeploymentPhase::AppDeployment);
        assert_eq!(checkpoint.completed_phases.len(), 2);
    }

    #[test]
    fn test_checkpoint_hooks_tracking() {
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");

        checkpoint.mark_pre_hook_completed("migrate-db");
        checkpoint.mark_pre_hook_completed("warm-cache");

        assert!(checkpoint.is_pre_hook_completed("migrate-db"));
        assert!(checkpoint.is_pre_hook_completed("warm-cache"));
        assert!(!checkpoint.is_pre_hook_completed("non-existent"));

        checkpoint.mark_post_hook_completed("notify-slack");
        assert!(checkpoint.is_post_hook_completed("notify-slack"));
        assert!(!checkpoint.is_post_hook_completed("migrate-db"));
    }

    #[test]
    fn test_checkpoint_add_resource() {
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");

        let resource = DeployedResource::new("ConfigMap", "app-config")
            .with_namespace("default");
        checkpoint.add_resource(resource);

        assert_eq!(checkpoint.deployed_resources.len(), 1);
        assert_eq!(checkpoint.deployed_resources[0].resource_name, "app-config");
    }

    #[test]
    fn test_checkpoint_can_resume() {
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");
        assert!(checkpoint.can_resume());

        // Advance to completed
        for _ in 0..6 {
            checkpoint.advance_phase();
        }
        assert_eq!(checkpoint.phase, DeploymentPhase::Completed);
        assert!(!checkpoint.can_resume());
    }

    #[test]
    fn test_checkpoint_error_message() {
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");
        assert!(checkpoint.error_message.is_none());

        checkpoint.set_error("Connection refused");
        assert_eq!(
            checkpoint.error_message,
            Some("Connection refused".to_string())
        );
    }
}
