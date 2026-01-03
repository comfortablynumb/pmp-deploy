use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

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
    pub metadata: std::collections::HashMap<String, String>,
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
            metadata: std::collections::HashMap::new(),
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
}
