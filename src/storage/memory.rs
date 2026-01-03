use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use super::record::DeploymentRecord;
use super::traits::Storage;

/// In-memory storage implementation for deployment history.
/// Useful for development, testing, and single-instance deployments.
pub struct InMemoryStorage {
    records: RwLock<HashMap<String, DeploymentRecord>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    fn name(&self) -> &str {
        "in-memory"
    }

    async fn save_deployment(&self, record: DeploymentRecord) -> anyhow::Result<String> {
        let id = record.id.clone();
        let mut records = self.records.write().map_err(|e| {
            anyhow::anyhow!("Failed to acquire write lock: {}", e)
        })?;
        records.insert(id.clone(), record);
        Ok(id)
    }

    async fn get_deployment(&self, id: &str) -> anyhow::Result<Option<DeploymentRecord>> {
        let records = self.records.read().map_err(|e| {
            anyhow::anyhow!("Failed to acquire read lock: {}", e)
        })?;
        Ok(records.get(id).cloned())
    }

    async fn list_deployments(
        &self,
        project: Option<&str>,
        environment: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<DeploymentRecord>> {
        let records = self.records.read().map_err(|e| {
            anyhow::anyhow!("Failed to acquire read lock: {}", e)
        })?;

        let mut filtered: Vec<_> = records
            .values()
            .filter(|r| filter_matches(r, project, environment))
            .cloned()
            .collect();

        // Sort by started_at descending (newest first)
        filtered.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        if let Some(limit) = limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    async fn cleanup(&self, max_age_days: u32) -> anyhow::Result<usize> {
        let max_age = Duration::from_secs(max_age_days as u64 * 24 * 60 * 60);
        let mut records = self.records.write().map_err(|e| {
            anyhow::anyhow!("Failed to acquire write lock: {}", e)
        })?;

        let before_count = records.len();
        records.retain(|_, r| r.age() < max_age);
        let removed = before_count - records.len();

        Ok(removed)
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        // In-memory storage is always healthy if we can acquire a lock
        let _records = self.records.read().map_err(|e| {
            anyhow::anyhow!("Storage health check failed: {}", e)
        })?;
        Ok(())
    }
}

fn filter_matches(
    record: &DeploymentRecord,
    project: Option<&str>,
    environment: Option<&str>,
) -> bool {
    let project_match = project.map_or(true, |p| record.project == p);
    let env_match = environment.map_or(true, |e| record.environment == e);
    project_match && env_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_get_deployment() {
        let storage = InMemoryStorage::new();
        let record = DeploymentRecord::new("my-project", "production", "aws-ecs")
            .with_image("myapp:v1.0.0");

        let id = storage.save_deployment(record.clone()).await.unwrap();
        let retrieved = storage.get_deployment(&id).await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.project, "my-project");
        assert_eq!(retrieved.environment, "production");
    }

    #[tokio::test]
    async fn test_get_nonexistent_deployment() {
        let storage = InMemoryStorage::new();
        let result = storage.get_deployment("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_deployments_all() {
        let storage = InMemoryStorage::new();

        storage
            .save_deployment(DeploymentRecord::new("proj-a", "prod", "aws-ecs"))
            .await
            .unwrap();
        storage
            .save_deployment(DeploymentRecord::new("proj-b", "staging", "kubernetes"))
            .await
            .unwrap();

        let all = storage.list_deployments(None, None, None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_list_deployments_filtered_by_project() {
        let storage = InMemoryStorage::new();

        storage
            .save_deployment(DeploymentRecord::new("proj-a", "prod", "aws-ecs"))
            .await
            .unwrap();
        storage
            .save_deployment(DeploymentRecord::new("proj-b", "staging", "kubernetes"))
            .await
            .unwrap();

        let filtered = storage
            .list_deployments(Some("proj-a"), None, None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].project, "proj-a");
    }

    #[tokio::test]
    async fn test_list_deployments_filtered_by_environment() {
        let storage = InMemoryStorage::new();

        storage
            .save_deployment(DeploymentRecord::new("proj-a", "prod", "aws-ecs"))
            .await
            .unwrap();
        storage
            .save_deployment(DeploymentRecord::new("proj-a", "staging", "aws-ecs"))
            .await
            .unwrap();

        let filtered = storage
            .list_deployments(None, Some("prod"), None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].environment, "prod");
    }

    #[tokio::test]
    async fn test_list_deployments_with_limit() {
        let storage = InMemoryStorage::new();

        for i in 0..5 {
            storage
                .save_deployment(DeploymentRecord::new(
                    format!("proj-{}", i),
                    "prod",
                    "aws-ecs",
                ))
                .await
                .unwrap();
        }

        let limited = storage.list_deployments(None, None, Some(3)).await.unwrap();
        assert_eq!(limited.len(), 3);
    }

    #[tokio::test]
    async fn test_get_latest_deployment() {
        let storage = InMemoryStorage::new();

        storage
            .save_deployment(
                DeploymentRecord::new("my-project", "prod", "aws-ecs").with_image("v1"),
            )
            .await
            .unwrap();

        // Small delay to ensure different timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        storage
            .save_deployment(
                DeploymentRecord::new("my-project", "prod", "aws-ecs").with_image("v2"),
            )
            .await
            .unwrap();

        let latest = storage
            .get_latest_deployment("my-project", "prod")
            .await
            .unwrap();

        assert!(latest.is_some());
        assert_eq!(latest.unwrap().image, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn test_cleanup() {
        let storage = InMemoryStorage::new();

        storage
            .save_deployment(DeploymentRecord::new("proj", "prod", "aws-ecs"))
            .await
            .unwrap();

        // Cleanup with 0 days should remove all records
        let removed = storage.cleanup(0).await.unwrap();
        assert_eq!(removed, 1);

        let all = storage.list_deployments(None, None, None).await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn test_health_check() {
        let storage = InMemoryStorage::new();
        assert!(storage.health_check().await.is_ok());
    }

    #[test]
    fn test_storage_name() {
        let storage = InMemoryStorage::new();
        assert_eq!(storage.name(), "in-memory");
    }

    #[test]
    fn test_filter_matches() {
        let record = DeploymentRecord::new("my-project", "production", "aws-ecs");

        assert!(filter_matches(&record, None, None));
        assert!(filter_matches(&record, Some("my-project"), None));
        assert!(filter_matches(&record, None, Some("production")));
        assert!(filter_matches(
            &record,
            Some("my-project"),
            Some("production")
        ));
        assert!(!filter_matches(&record, Some("other-project"), None));
        assert!(!filter_matches(&record, None, Some("staging")));
    }
}
