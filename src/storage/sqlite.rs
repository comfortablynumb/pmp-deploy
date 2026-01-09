//! SQLite-based persistent storage for deployment history.
//!
//! Provides a reliable, ACID-compliant storage backend using SQLite.

use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::debug;

use super::record::{DeploymentCheckpoint, DeploymentPhase, DeploymentRecord, DeploymentStatus};
use super::traits::Storage;

/// SQLite-based storage implementation.
pub struct SqliteStorage {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl SqliteStorage {
    /// Create a new SQLite storage at the given path.
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;

        let storage = Self {
            conn: Mutex::new(conn),
            path,
        };

        storage.initialize_schema()?;
        Ok(storage)
    }

    /// Create storage in the default location (~/.pmp-deploy/history.db).
    pub fn default_location() -> anyhow::Result<Self> {
        let path = directories::BaseDirs::new()
            .map(|d| d.data_dir().join("pmp-deploy").join("history.db"))
            .unwrap_or_else(|| PathBuf::from(".pmp-deploy/history.db"));

        Self::new(path)
    }

    /// Create an in-memory SQLite database (useful for testing).
    pub fn in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;

        let storage = Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        };

        storage.initialize_schema()?;
        Ok(storage)
    }

    fn initialize_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS deployments (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                environment TEXT NOT NULL,
                infrastructure_type TEXT NOT NULL,
                image TEXT,
                previous_image TEXT,
                status TEXT NOT NULL,
                message TEXT NOT NULL,
                deploy_mode TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                duration_secs INTEGER,
                triggered_by TEXT,
                dry_run INTEGER NOT NULL DEFAULT 0,
                metadata TEXT NOT NULL DEFAULT '{}'
            );

            CREATE INDEX IF NOT EXISTS idx_deployments_project
                ON deployments(project);
            CREATE INDEX IF NOT EXISTS idx_deployments_environment
                ON deployments(environment);
            CREATE INDEX IF NOT EXISTS idx_deployments_started_at
                ON deployments(started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_deployments_project_env
                ON deployments(project, environment);

            CREATE TABLE IF NOT EXISTS checkpoints (
                deployment_id TEXT PRIMARY KEY,
                phase TEXT NOT NULL,
                completed_phases TEXT NOT NULL,
                deployed_resources TEXT NOT NULL,
                pre_hooks_completed TEXT NOT NULL,
                post_hooks_completed TEXT NOT NULL,
                last_updated INTEGER NOT NULL,
                context_snapshot TEXT,
                error_message TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_checkpoints_phase
                ON checkpoints(phase);
            CREATE INDEX IF NOT EXISTS idx_checkpoints_last_updated
                ON checkpoints(last_updated DESC);
            "#,
        )?;

        Ok(())
    }

    fn status_to_string(status: &DeploymentStatus) -> &'static str {
        match status {
            DeploymentStatus::InProgress => "in_progress",
            DeploymentStatus::Success => "success",
            DeploymentStatus::Failed => "failed",
            DeploymentStatus::RolledBack => "rolled_back",
        }
    }

    fn string_to_status(s: &str) -> DeploymentStatus {
        match s {
            "success" => DeploymentStatus::Success,
            "failed" => DeploymentStatus::Failed,
            "rolled_back" => DeploymentStatus::RolledBack,
            _ => DeploymentStatus::InProgress,
        }
    }

    fn system_time_to_epoch(time: SystemTime) -> i64 {
        time.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn epoch_to_system_time(epoch: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(epoch as u64)
    }

    fn phase_to_string(phase: &DeploymentPhase) -> &'static str {
        phase.as_str()
    }

    fn string_to_phase(s: &str) -> DeploymentPhase {
        match s {
            "pre_hooks" => DeploymentPhase::PreHooks,
            "infrastructure_provisioning" => DeploymentPhase::InfrastructureProvisioning,
            "app_deployment" => DeploymentPhase::AppDeployment,
            "health_check" => DeploymentPhase::HealthCheck,
            "post_hooks" => DeploymentPhase::PostHooks,
            "completed" => DeploymentPhase::Completed,
            _ => DeploymentPhase::PreHooks,
        }
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    fn name(&self) -> &str {
        "sqlite"
    }

    async fn save_deployment(&self, record: DeploymentRecord) -> anyhow::Result<String> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let metadata_json = serde_json::to_string(&record.metadata)?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO deployments (
                id, project, environment, infrastructure_type,
                image, previous_image, status, message, deploy_mode,
                started_at, completed_at, duration_secs, triggered_by,
                dry_run, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                record.id,
                record.project,
                record.environment,
                record.infrastructure_type,
                record.image,
                record.previous_image,
                Self::status_to_string(&record.status),
                record.message,
                record.deploy_mode,
                Self::system_time_to_epoch(record.started_at),
                record.completed_at.map(Self::system_time_to_epoch),
                record.duration_secs.map(|d| d as i64),
                record.triggered_by,
                record.dry_run as i32,
                metadata_json,
            ],
        )?;

        debug!("Saved deployment {} to SQLite", record.id);
        Ok(record.id)
    }

    async fn get_deployment(&self, id: &str) -> anyhow::Result<Option<DeploymentRecord>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, project, environment, infrastructure_type,
                   image, previous_image, status, message, deploy_mode,
                   started_at, completed_at, duration_secs, triggered_by,
                   dry_run, metadata
            FROM deployments
            WHERE id = ?1
            "#,
        )?;

        let result = stmt.query_row(params![id], |row| {
            Ok(Self::row_to_record(row))
        });

        match result {
            Ok(record) => Ok(Some(record?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_deployments(
        &self,
        project: Option<&str>,
        environment: Option<&str>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<DeploymentRecord>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut sql = String::from(
            r#"
            SELECT id, project, environment, infrastructure_type,
                   image, previous_image, status, message, deploy_mode,
                   started_at, completed_at, duration_secs, triggered_by,
                   dry_run, metadata
            FROM deployments
            WHERE 1=1
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(p) = project {
            sql.push_str(" AND project = ?");
            params_vec.push(Box::new(p.to_string()));
        }

        if let Some(e) = environment {
            sql.push_str(" AND environment = ?");
            params_vec.push(Box::new(e.to_string()));
        }

        sql.push_str(" ORDER BY started_at DESC");

        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {}", l));
        }

        let mut stmt = conn.prepare(&sql)?;

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| Ok(Self::row_to_record(row)))?;

        let mut records = Vec::new();

        for row_result in rows {
            match row_result {
                Ok(record_result) => match record_result {
                    Ok(record) => records.push(record),
                    Err(e) => tracing::warn!("Failed to parse deployment record: {}", e),
                },
                Err(e) => tracing::warn!("Failed to read row: {}", e),
            }
        }

        Ok(records)
    }

    async fn cleanup(&self, max_age_days: u32) -> anyhow::Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let max_age_secs = max_age_days as i64 * 24 * 60 * 60;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let cutoff = now - max_age_secs;

        let deleted = conn.execute(
            "DELETE FROM deployments WHERE started_at <= ?1",
            params![cutoff],
        )?;

        debug!("Cleaned up {} old deployments from SQLite", deleted);
        Ok(deleted)
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Simple query to verify database is accessible
        conn.query_row("SELECT 1", [], |_| Ok(()))?;

        Ok(())
    }

    async fn save_checkpoint(&self, checkpoint: DeploymentCheckpoint) -> anyhow::Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let completed_phases_json = serde_json::to_string(&checkpoint.completed_phases)?;
        let deployed_resources_json = serde_json::to_string(&checkpoint.deployed_resources)?;
        let pre_hooks_json = serde_json::to_string(&checkpoint.pre_hooks_completed)?;
        let post_hooks_json = serde_json::to_string(&checkpoint.post_hooks_completed)?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO checkpoints (
                deployment_id, phase, completed_phases, deployed_resources,
                pre_hooks_completed, post_hooks_completed, last_updated,
                context_snapshot, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                checkpoint.deployment_id,
                Self::phase_to_string(&checkpoint.phase),
                completed_phases_json,
                deployed_resources_json,
                pre_hooks_json,
                post_hooks_json,
                Self::system_time_to_epoch(checkpoint.last_updated),
                checkpoint.context_snapshot,
                checkpoint.error_message,
            ],
        )?;

        debug!("Saved checkpoint for {} to SQLite", checkpoint.deployment_id);
        Ok(())
    }

    async fn get_checkpoint(&self, deployment_id: &str) -> anyhow::Result<Option<DeploymentCheckpoint>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT deployment_id, phase, completed_phases, deployed_resources,
                   pre_hooks_completed, post_hooks_completed, last_updated,
                   context_snapshot, error_message
            FROM checkpoints
            WHERE deployment_id = ?1
            "#,
        )?;

        let result = stmt.query_row(params![deployment_id], |row| {
            Ok(Self::row_to_checkpoint(row))
        });

        match result {
            Ok(checkpoint) => Ok(Some(checkpoint?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete_checkpoint(&self, deployment_id: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let deleted = conn.execute(
            "DELETE FROM checkpoints WHERE deployment_id = ?1",
            params![deployment_id],
        )?;

        debug!("Deleted checkpoint for {} from SQLite", deployment_id);
        Ok(deleted > 0)
    }

    async fn list_active_checkpoints(&self) -> anyhow::Result<Vec<DeploymentCheckpoint>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT deployment_id, phase, completed_phases, deployed_resources,
                   pre_hooks_completed, post_hooks_completed, last_updated,
                   context_snapshot, error_message
            FROM checkpoints
            WHERE phase != 'completed'
            ORDER BY last_updated DESC
            "#,
        )?;

        let rows = stmt.query_map([], |row| Ok(Self::row_to_checkpoint(row)))?;

        let mut checkpoints = Vec::new();

        for row_result in rows {
            match row_result {
                Ok(checkpoint_result) => match checkpoint_result {
                    Ok(checkpoint) => checkpoints.push(checkpoint),
                    Err(e) => tracing::warn!("Failed to parse checkpoint: {}", e),
                },
                Err(e) => tracing::warn!("Failed to read row: {}", e),
            }
        }

        Ok(checkpoints)
    }

    async fn cleanup_checkpoints(&self, max_age_days: u32) -> anyhow::Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("{}", e))?;

        let max_age_secs = max_age_days as i64 * 24 * 60 * 60;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let cutoff = now - max_age_secs;

        let deleted = conn.execute(
            "DELETE FROM checkpoints WHERE last_updated <= ?1",
            params![cutoff],
        )?;

        debug!("Cleaned up {} old checkpoints from SQLite", deleted);
        Ok(deleted)
    }
}

impl SqliteStorage {
    fn row_to_record(row: &rusqlite::Row<'_>) -> anyhow::Result<DeploymentRecord> {
        let metadata_json: String = row.get(14)?;
        let metadata: std::collections::HashMap<String, String> =
            serde_json::from_str(&metadata_json).unwrap_or_default();

        let completed_at: Option<i64> = row.get(10)?;
        let duration_secs: Option<i64> = row.get(11)?;

        Ok(DeploymentRecord {
            id: row.get(0)?,
            project: row.get(1)?,
            environment: row.get(2)?,
            infrastructure_type: row.get(3)?,
            image: row.get(4)?,
            previous_image: row.get(5)?,
            status: Self::string_to_status(row.get::<_, String>(6)?.as_str()),
            message: row.get(7)?,
            deploy_mode: row.get(8)?,
            started_at: Self::epoch_to_system_time(row.get(9)?),
            completed_at: completed_at.map(Self::epoch_to_system_time),
            duration_secs: duration_secs.map(|d| d as u64),
            triggered_by: row.get(12)?,
            dry_run: row.get::<_, i32>(13)? != 0,
            metadata,
        })
    }

    fn row_to_checkpoint(row: &rusqlite::Row<'_>) -> anyhow::Result<DeploymentCheckpoint> {
        let completed_phases_json: String = row.get(2)?;
        let deployed_resources_json: String = row.get(3)?;
        let pre_hooks_json: String = row.get(4)?;
        let post_hooks_json: String = row.get(5)?;

        Ok(DeploymentCheckpoint {
            deployment_id: row.get(0)?,
            phase: Self::string_to_phase(row.get::<_, String>(1)?.as_str()),
            completed_phases: serde_json::from_str(&completed_phases_json).unwrap_or_default(),
            deployed_resources: serde_json::from_str(&deployed_resources_json).unwrap_or_default(),
            pre_hooks_completed: serde_json::from_str(&pre_hooks_json).unwrap_or_default(),
            post_hooks_completed: serde_json::from_str(&post_hooks_json).unwrap_or_default(),
            last_updated: Self::epoch_to_system_time(row.get(6)?),
            context_snapshot: row.get(7)?,
            error_message: row.get(8)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_storage() -> SqliteStorage {
        SqliteStorage::in_memory().unwrap()
    }

    #[tokio::test]
    async fn test_save_and_get_deployment() {
        let storage = create_test_storage();

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
        let storage = create_test_storage();
        let result = storage.get_deployment("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_deployments() {
        let storage = create_test_storage();

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
        let storage = create_test_storage();

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
        let storage = create_test_storage();

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
        let storage = create_test_storage();

        let record = DeploymentRecord::new("my-project", "prod", "aws-ecs");
        let id = record.id.clone();

        storage.save_deployment(record).await.unwrap();

        // Update the same record
        let mut updated = storage.get_deployment(&id).await.unwrap().unwrap();
        updated.message = "Updated".to_string();

        storage.save_deployment(updated).await.unwrap();

        let retrieved = storage.get_deployment(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.message, "Updated");

        // Should not create duplicate
        let all = storage.list_deployments(None, None, None).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let storage = create_test_storage();

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
        let storage = create_test_storage();
        assert!(storage.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_storage_name() {
        let storage = create_test_storage();
        assert_eq!(storage.name(), "sqlite");
    }

    #[tokio::test]
    async fn test_deployment_with_metadata() {
        let storage = create_test_storage();

        let record = DeploymentRecord::new("my-project", "prod", "aws-ecs")
            .with_metadata("commit", "abc123")
            .with_metadata("branch", "main");

        let id = storage.save_deployment(record).await.unwrap();
        let retrieved = storage.get_deployment(&id).await.unwrap().unwrap();

        assert_eq!(retrieved.metadata.get("commit"), Some(&"abc123".to_string()));
        assert_eq!(retrieved.metadata.get("branch"), Some(&"main".to_string()));
    }

    #[tokio::test]
    async fn test_deployment_status_roundtrip() {
        let storage = create_test_storage();

        let record = DeploymentRecord::new("proj", "prod", "aws-ecs")
            .success("Completed successfully");

        let id = storage.save_deployment(record).await.unwrap();
        let retrieved = storage.get_deployment(&id).await.unwrap().unwrap();

        assert_eq!(retrieved.status, DeploymentStatus::Success);
        assert!(retrieved.completed_at.is_some());
    }

    // ========== Checkpoint Tests ==========

    #[tokio::test]
    async fn test_save_and_get_checkpoint() {
        let storage = create_test_storage();
        let checkpoint = DeploymentCheckpoint::new("dep_123");

        storage.save_checkpoint(checkpoint).await.unwrap();
        let retrieved = storage.get_checkpoint("dep_123").await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().deployment_id, "dep_123");
    }

    #[tokio::test]
    async fn test_get_nonexistent_checkpoint() {
        let storage = create_test_storage();
        let result = storage.get_checkpoint("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let storage = create_test_storage();
        let checkpoint = DeploymentCheckpoint::new("dep_123");

        storage.save_checkpoint(checkpoint).await.unwrap();

        let deleted = storage.delete_checkpoint("dep_123").await.unwrap();
        assert!(deleted);

        let retrieved = storage.get_checkpoint("dep_123").await.unwrap();
        assert!(retrieved.is_none());

        // Deleting again should return false
        let deleted_again = storage.delete_checkpoint("dep_123").await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_list_active_checkpoints() {
        let storage = create_test_storage();

        // Active checkpoint
        let active = DeploymentCheckpoint::new("dep_active");
        storage.save_checkpoint(active).await.unwrap();

        // Completed checkpoint
        let mut completed = DeploymentCheckpoint::new("dep_completed");
        completed.set_phase(DeploymentPhase::Completed);
        storage.save_checkpoint(completed).await.unwrap();

        let active_list = storage.list_active_checkpoints().await.unwrap();
        assert_eq!(active_list.len(), 1);
        assert_eq!(active_list[0].deployment_id, "dep_active");
    }

    #[tokio::test]
    async fn test_update_checkpoint() {
        let storage = create_test_storage();
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");

        storage.save_checkpoint(checkpoint.clone()).await.unwrap();

        // Update the checkpoint
        checkpoint.advance_phase();
        storage.save_checkpoint(checkpoint).await.unwrap();

        let retrieved = storage.get_checkpoint("dep_123").await.unwrap().unwrap();
        assert_eq!(retrieved.phase, DeploymentPhase::InfrastructureProvisioning);
    }

    #[tokio::test]
    async fn test_cleanup_checkpoints() {
        let storage = create_test_storage();

        storage
            .save_checkpoint(DeploymentCheckpoint::new("dep_123"))
            .await
            .unwrap();

        // Cleanup with 0 days should remove all checkpoints
        let removed = storage.cleanup_checkpoints(0).await.unwrap();
        assert_eq!(removed, 1);

        let active = storage.list_active_checkpoints().await.unwrap();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn test_checkpoint_with_resources() {
        use super::super::record::DeployedResource;

        let storage = create_test_storage();
        let mut checkpoint = DeploymentCheckpoint::new("dep_123");

        checkpoint.add_resource(
            DeployedResource::new("Deployment", "my-app")
                .with_namespace("production"),
        );
        checkpoint.mark_pre_hook_completed("migrate-db");
        checkpoint.mark_post_hook_completed("notify-slack");

        storage.save_checkpoint(checkpoint).await.unwrap();

        let retrieved = storage.get_checkpoint("dep_123").await.unwrap().unwrap();
        assert_eq!(retrieved.deployed_resources.len(), 1);
        assert_eq!(retrieved.deployed_resources[0].resource_name, "my-app");
        assert!(retrieved.is_pre_hook_completed("migrate-db"));
        assert!(retrieved.is_post_hook_completed("notify-slack"));
    }
}
