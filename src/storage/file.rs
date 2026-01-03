//! File-based persistent storage for deployment history.
//!
//! Stores deployment records as JSON files on disk, organized by project and environment.

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use super::record::DeploymentRecord;
use super::traits::Storage;

/// File-based storage implementation.
///
/// Directory structure:
/// ```text
/// <base_path>/
/// ├── index.json           # Index of all deployments (for fast listing)
/// └── deployments/
///     ├── <id1>.json
///     ├── <id2>.json
///     └── ...
/// ```
pub struct FileStorage {
    base_path: PathBuf,
    deployments_dir: PathBuf,
    index_path: PathBuf,
}

/// Index file structure for fast listing
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct DeploymentIndex {
    deployments: Vec<IndexEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IndexEntry {
    id: String,
    project: String,
    environment: String,
    started_at_epoch: u64,
}

impl FileStorage {
    /// Create a new file storage at the given path.
    pub async fn new(base_path: PathBuf) -> anyhow::Result<Self> {
        let deployments_dir = base_path.join("deployments");
        let index_path = base_path.join("index.json");

        // Create directories if they don't exist
        fs::create_dir_all(&deployments_dir).await?;

        // Create index file if it doesn't exist
        if !index_path.exists() {
            let index = DeploymentIndex::default();
            let json = serde_json::to_string_pretty(&index)?;
            fs::write(&index_path, json).await?;
        }

        Ok(Self {
            base_path,
            deployments_dir,
            index_path,
        })
    }

    /// Create storage in the default location (~/.pmp-deploy/history/).
    pub async fn default_location() -> anyhow::Result<Self> {
        let base_path = directories::BaseDirs::new()
            .map(|d| d.data_dir().join("pmp-deploy").join("history"))
            .unwrap_or_else(|| PathBuf::from(".pmp-deploy/history"));

        Self::new(base_path).await
    }

    fn deployment_path(&self, id: &str) -> PathBuf {
        self.deployments_dir.join(format!("{}.json", id))
    }

    async fn load_index(&self) -> anyhow::Result<DeploymentIndex> {
        let contents = fs::read_to_string(&self.index_path).await?;
        let index: DeploymentIndex = serde_json::from_str(&contents)?;
        Ok(index)
    }

    async fn save_index(&self, index: &DeploymentIndex) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(index)?;

        // Write to temp file first, then rename for atomicity
        let temp_path = self.index_path.with_extension("json.tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;
        fs::rename(&temp_path, &self.index_path).await?;

        Ok(())
    }

    fn record_to_index_entry(record: &DeploymentRecord) -> IndexEntry {
        let started_at_epoch = record
            .started_at
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        IndexEntry {
            id: record.id.clone(),
            project: record.project.clone(),
            environment: record.environment.clone(),
            started_at_epoch,
        }
    }
}

#[async_trait]
impl Storage for FileStorage {
    fn name(&self) -> &str {
        "file"
    }

    async fn save_deployment(&self, record: DeploymentRecord) -> anyhow::Result<String> {
        let id = record.id.clone();
        let path = self.deployment_path(&id);

        // Save the deployment record
        let json = serde_json::to_string_pretty(&record)?;
        let temp_path = path.with_extension("json.tmp");
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.sync_all().await?;
        fs::rename(&temp_path, &path).await?;

        // Update the index
        let mut index = self.load_index().await.unwrap_or_default();

        // Remove existing entry if updating
        index.deployments.retain(|e| e.id != id);

        // Add new entry
        index
            .deployments
            .push(Self::record_to_index_entry(&record));

        // Sort by started_at descending
        index
            .deployments
            .sort_by(|a, b| b.started_at_epoch.cmp(&a.started_at_epoch));

        self.save_index(&index).await?;

        debug!("Saved deployment {} to {}", id, path.display());
        Ok(id)
    }

    async fn get_deployment(&self, id: &str) -> anyhow::Result<Option<DeploymentRecord>> {
        let path = self.deployment_path(id);

        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).await?;
        let record: DeploymentRecord = serde_json::from_str(&contents)?;
        Ok(Some(record))
    }

    async fn list_deployments(
        &self,
        project: Option<&str>,
        environment: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<DeploymentRecord>> {
        let index = self.load_index().await?;

        // Filter entries based on project and environment
        let filtered_ids: Vec<_> = index
            .deployments
            .iter()
            .filter(|e| {
                let project_match = project.map_or(true, |p| e.project == p);
                let env_match = environment.map_or(true, |env| e.environment == env);
                project_match && env_match
            })
            .take(limit.unwrap_or(usize::MAX))
            .map(|e| e.id.clone())
            .collect();

        // Load full records
        let mut records = Vec::with_capacity(filtered_ids.len());

        for id in filtered_ids {
            match self.get_deployment(&id).await {
                Ok(Some(record)) => records.push(record),
                Ok(None) => {
                    warn!("Deployment {} in index but file not found", id);
                }
                Err(e) => {
                    warn!("Failed to load deployment {}: {}", id, e);
                }
            }
        }

        Ok(records)
    }

    async fn cleanup(&self, max_age_days: u32) -> anyhow::Result<usize> {
        let max_age_secs = max_age_days as u64 * 24 * 60 * 60;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let cutoff = now.saturating_sub(max_age_secs);

        let mut index = self.load_index().await?;
        let before_count = index.deployments.len();

        // Find IDs to remove
        let to_remove: Vec<_> = index
            .deployments
            .iter()
            .filter(|e| e.started_at_epoch <= cutoff)
            .map(|e| e.id.clone())
            .collect();

        // Delete files
        for id in &to_remove {
            let path = self.deployment_path(id);

            if path.exists() {
                if let Err(e) = fs::remove_file(&path).await {
                    warn!("Failed to delete deployment file {}: {}", id, e);
                }
            }
        }

        // Update index
        index.deployments.retain(|e| e.started_at_epoch > cutoff);
        self.save_index(&index).await?;

        let removed = before_count - index.deployments.len();
        debug!("Cleaned up {} old deployments", removed);

        Ok(removed)
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        // Check that we can read and write to the storage
        if !self.base_path.exists() {
            anyhow::bail!("Storage directory does not exist: {}", self.base_path.display());
        }

        // Try to load the index
        self.load_index().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (FileStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_save_and_get_deployment() {
        let (storage, _temp) = create_test_storage().await;

        let record = DeploymentRecord::new("my-project", "production", "aws-ecs")
            .with_image("myapp:v1.0.0");

        let id = storage.save_deployment(record.clone()).await.unwrap();
        let retrieved = storage.get_deployment(&id).await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.project, "my-project");
        assert_eq!(retrieved.environment, "production");
        assert_eq!(retrieved.image, Some("myapp:v1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_deployment() {
        let (storage, _temp) = create_test_storage().await;
        let result = storage.get_deployment("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_deployments() {
        let (storage, _temp) = create_test_storage().await;

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
    async fn test_list_deployments_filtered() {
        let (storage, _temp) = create_test_storage().await;

        storage
            .save_deployment(DeploymentRecord::new("proj-a", "prod", "aws-ecs"))
            .await
            .unwrap();
        storage
            .save_deployment(DeploymentRecord::new("proj-a", "staging", "aws-ecs"))
            .await
            .unwrap();
        storage
            .save_deployment(DeploymentRecord::new("proj-b", "prod", "kubernetes"))
            .await
            .unwrap();

        let filtered = storage
            .list_deployments(Some("proj-a"), None, None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 2);

        let filtered = storage
            .list_deployments(None, Some("prod"), None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 2);

        let filtered = storage
            .list_deployments(Some("proj-a"), Some("prod"), None)
            .await
            .unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[tokio::test]
    async fn test_list_deployments_with_limit() {
        let (storage, _temp) = create_test_storage().await;

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
    async fn test_update_deployment() {
        let (storage, _temp) = create_test_storage().await;

        let record = DeploymentRecord::new("my-project", "prod", "aws-ecs");
        let id = record.id.clone();

        storage.save_deployment(record).await.unwrap();

        // Update the same record
        let mut updated = storage.get_deployment(&id).await.unwrap().unwrap();
        updated.message = "Updated".to_string();

        storage.save_deployment(updated).await.unwrap();

        let retrieved = storage.get_deployment(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.message, "Updated");

        // Index should not have duplicates
        let all = storage.list_deployments(None, None, None).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let (storage, _temp) = create_test_storage().await;

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
        let (storage, _temp) = create_test_storage().await;
        assert!(storage.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_storage_name() {
        let (storage, _temp) = create_test_storage().await;
        assert_eq!(storage.name(), "file");
    }
}
