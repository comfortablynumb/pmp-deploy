use async_trait::async_trait;

use super::record::{DeploymentCheckpoint, DeploymentRecord};

/// Storage trait for persisting deployment history and other data.
///
/// Implementations can use different backends:
/// - InMemoryStorage (default)
/// - FilesystemStorage (future)
/// - PostgresStorage (future)
#[async_trait]
pub trait Storage: Send + Sync {
    /// Get the storage backend name
    fn name(&self) -> &str;

    /// Store a deployment record
    async fn save_deployment(&self, record: DeploymentRecord) -> anyhow::Result<String>;

    /// Get a deployment record by ID
    async fn get_deployment(&self, id: &str) -> anyhow::Result<Option<DeploymentRecord>>;

    /// List deployments for a project/environment
    async fn list_deployments(
        &self,
        project: Option<&str>,
        environment: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<DeploymentRecord>>;

    /// Get the most recent deployment for an environment
    async fn get_latest_deployment(
        &self,
        project: &str,
        environment: &str,
    ) -> anyhow::Result<Option<DeploymentRecord>> {
        let deployments = self
            .list_deployments(Some(project), Some(environment), Some(1))
            .await?;
        Ok(deployments.into_iter().next())
    }

    /// Delete old deployments (retention policy)
    async fn cleanup(&self, max_age_days: u32) -> anyhow::Result<usize>;

    /// Health check for the storage backend
    async fn health_check(&self) -> anyhow::Result<()>;

    // ========== Checkpoint Methods ==========

    /// Save or update a deployment checkpoint
    async fn save_checkpoint(&self, checkpoint: DeploymentCheckpoint) -> anyhow::Result<()>;

    /// Get checkpoint for a deployment
    async fn get_checkpoint(&self, deployment_id: &str) -> anyhow::Result<Option<DeploymentCheckpoint>>;

    /// Delete checkpoint (on success or explicit cleanup)
    async fn delete_checkpoint(&self, deployment_id: &str) -> anyhow::Result<bool>;

    /// List all active checkpoints (incomplete deployments)
    async fn list_active_checkpoints(&self) -> anyhow::Result<Vec<DeploymentCheckpoint>>;

    /// Cleanup old checkpoints (retention policy)
    async fn cleanup_checkpoints(&self, max_age_days: u32) -> anyhow::Result<usize> {
        // Default implementation: no-op for backwards compatibility
        let _ = max_age_days;
        Ok(0)
    }
}
