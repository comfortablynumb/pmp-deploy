use async_trait::async_trait;

use super::record::DeploymentRecord;

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
}
