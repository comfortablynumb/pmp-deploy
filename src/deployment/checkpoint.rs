//! Checkpoint management for resumable deployments.
//!
//! This module provides the `CheckpointManager` for creating, updating, and
//! managing deployment checkpoints that enable resumption of interrupted deployments.

use std::sync::Arc;

use crate::signal::{is_shutdown_requested, SignalHandler};
use crate::storage::{
    DeployedResource, DeploymentCheckpoint, DeploymentPhase, Storage,
};

/// Error indicating the deployment was interrupted
#[derive(Debug, Clone)]
pub struct InterruptedError {
    pub deployment_id: String,
    pub phase: DeploymentPhase,
    pub message: String,
}

impl std::fmt::Display for InterruptedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Deployment {} interrupted at phase {}: {}",
            self.deployment_id, self.phase, self.message
        )
    }
}

impl std::error::Error for InterruptedError {}

/// Configuration for CheckpointManager
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Whether checkpointing is enabled
    pub enabled: bool,
    /// Whether to auto-save checkpoint on interrupt
    pub save_on_interrupt: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            save_on_interrupt: true,
        }
    }
}

/// Manages deployment checkpoints for resumable deployments.
///
/// The CheckpointManager is responsible for:
/// - Creating checkpoints at the start of a deployment
/// - Updating checkpoints as phases complete
/// - Saving checkpoints on interrupt
/// - Clearing checkpoints on successful completion
pub struct CheckpointManager {
    storage: Arc<dyn Storage>,
    signal_handler: Option<SignalHandler>,
    config: CheckpointConfig,
}

impl CheckpointManager {
    /// Create a new CheckpointManager with the given storage.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            signal_handler: None,
            config: CheckpointConfig::default(),
        }
    }

    /// Create with a signal handler for interrupt detection.
    pub fn with_signal_handler(mut self, handler: SignalHandler) -> Self {
        self.signal_handler = Some(handler);
        self
    }

    /// Create with custom configuration.
    pub fn with_config(mut self, config: CheckpointConfig) -> Self {
        self.config = config;
        self
    }

    /// Create a new checkpoint for a deployment.
    pub async fn create_checkpoint(&self, deployment_id: &str) -> anyhow::Result<DeploymentCheckpoint> {
        if !self.config.enabled {
            return Ok(DeploymentCheckpoint::new(deployment_id));
        }

        let checkpoint = DeploymentCheckpoint::new(deployment_id);
        self.storage.save_checkpoint(checkpoint.clone()).await?;

        tracing::debug!("Created checkpoint for deployment {}", deployment_id);
        Ok(checkpoint)
    }

    /// Create checkpoint with context snapshot for resumption.
    pub async fn create_with_context(
        &self,
        deployment_id: &str,
        context_json: &str,
    ) -> anyhow::Result<DeploymentCheckpoint> {
        if !self.config.enabled {
            let mut checkpoint = DeploymentCheckpoint::new(deployment_id);
            checkpoint.context_snapshot = Some(context_json.to_string());
            return Ok(checkpoint);
        }

        let checkpoint = DeploymentCheckpoint::new(deployment_id)
            .with_context_snapshot(context_json);
        self.storage.save_checkpoint(checkpoint.clone()).await?;

        tracing::debug!(
            "Created checkpoint with context for deployment {}",
            deployment_id
        );
        Ok(checkpoint)
    }

    /// Update the checkpoint to a new phase.
    pub async fn advance_phase(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
    ) -> anyhow::Result<()> {
        checkpoint.advance_phase();

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }

        tracing::debug!(
            "Advanced checkpoint {} to phase {}",
            checkpoint.deployment_id,
            checkpoint.phase
        );
        Ok(())
    }

    /// Set the checkpoint to a specific phase.
    pub async fn set_phase(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        phase: DeploymentPhase,
    ) -> anyhow::Result<()> {
        checkpoint.set_phase(phase);

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }

        tracing::debug!(
            "Set checkpoint {} to phase {}",
            checkpoint.deployment_id,
            checkpoint.phase
        );
        Ok(())
    }

    /// Record a deployed resource in the checkpoint.
    pub async fn record_resource(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        resource: DeployedResource,
    ) -> anyhow::Result<()> {
        checkpoint.add_resource(resource);

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }
        Ok(())
    }

    /// Mark a pre-deployment hook as completed.
    pub async fn mark_pre_hook_completed(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        hook_name: &str,
    ) -> anyhow::Result<()> {
        checkpoint.mark_pre_hook_completed(hook_name);

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }

        tracing::debug!(
            "Marked pre-hook '{}' completed for {}",
            hook_name,
            checkpoint.deployment_id
        );
        Ok(())
    }

    /// Mark a post-deployment hook as completed.
    pub async fn mark_post_hook_completed(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        hook_name: &str,
    ) -> anyhow::Result<()> {
        checkpoint.mark_post_hook_completed(hook_name);

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }

        tracing::debug!(
            "Marked post-hook '{}' completed for {}",
            hook_name,
            checkpoint.deployment_id
        );
        Ok(())
    }

    /// Set an error message on the checkpoint.
    pub async fn set_error(
        &self,
        checkpoint: &mut DeploymentCheckpoint,
        message: &str,
    ) -> anyhow::Result<()> {
        checkpoint.set_error(message);

        if self.config.enabled {
            self.storage.save_checkpoint(checkpoint.clone()).await?;
        }

        tracing::warn!(
            "Set error on checkpoint {}: {}",
            checkpoint.deployment_id,
            message
        );
        Ok(())
    }

    /// Check if an interrupt has been requested and save checkpoint if so.
    ///
    /// Returns `Ok(())` if no interrupt, or `Err(InterruptedError)` if interrupted.
    pub async fn check_interrupt(
        &self,
        checkpoint: &DeploymentCheckpoint,
    ) -> Result<(), InterruptedError> {
        if !is_shutdown_requested() {
            return Ok(());
        }

        // Save checkpoint before returning error
        if self.config.enabled && self.config.save_on_interrupt {
            if let Err(e) = self.storage.save_checkpoint(checkpoint.clone()).await {
                tracing::error!("Failed to save checkpoint on interrupt: {}", e);
            } else {
                tracing::info!(
                    "Saved checkpoint for {} at phase {} before interrupt",
                    checkpoint.deployment_id,
                    checkpoint.phase
                );
            }
        }

        Err(InterruptedError {
            deployment_id: checkpoint.deployment_id.clone(),
            phase: checkpoint.phase.clone(),
            message: "Deployment interrupted by user".to_string(),
        })
    }

    /// Get an existing checkpoint for a deployment.
    pub async fn get_checkpoint(
        &self,
        deployment_id: &str,
    ) -> anyhow::Result<Option<DeploymentCheckpoint>> {
        self.storage.get_checkpoint(deployment_id).await
    }

    /// Clear the checkpoint (on successful completion).
    pub async fn clear_checkpoint(&self, deployment_id: &str) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.storage.delete_checkpoint(deployment_id).await?;
        tracing::debug!("Cleared checkpoint for deployment {}", deployment_id);
        Ok(())
    }

    /// List all active (resumable) checkpoints.
    pub async fn list_resumable(&self) -> anyhow::Result<Vec<DeploymentCheckpoint>> {
        self.storage.list_active_checkpoints().await
    }

    /// Check if a deployment can be resumed.
    pub async fn can_resume(&self, deployment_id: &str) -> anyhow::Result<bool> {
        let checkpoint = self.storage.get_checkpoint(deployment_id).await?;
        Ok(checkpoint.map(|c| c.can_resume()).unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryStorage;

    fn create_test_manager() -> CheckpointManager {
        let storage = Arc::new(InMemoryStorage::new());
        CheckpointManager::new(storage)
    }

    #[tokio::test]
    async fn test_create_checkpoint() {
        let manager = create_test_manager();
        let checkpoint = manager.create_checkpoint("dep_123").await.unwrap();

        assert_eq!(checkpoint.deployment_id, "dep_123");
        assert_eq!(checkpoint.phase, DeploymentPhase::PreHooks);
    }

    #[tokio::test]
    async fn test_advance_phase() {
        let manager = create_test_manager();
        let mut checkpoint = manager.create_checkpoint("dep_123").await.unwrap();

        manager.advance_phase(&mut checkpoint).await.unwrap();

        assert_eq!(checkpoint.phase, DeploymentPhase::InfrastructureProvisioning);
        assert_eq!(checkpoint.completed_phases.len(), 1);
    }

    #[tokio::test]
    async fn test_record_resource() {
        let manager = create_test_manager();
        let mut checkpoint = manager.create_checkpoint("dep_123").await.unwrap();

        let resource = DeployedResource::new("Deployment", "my-app")
            .with_namespace("default");
        manager.record_resource(&mut checkpoint, resource).await.unwrap();

        assert_eq!(checkpoint.deployed_resources.len(), 1);
    }

    #[tokio::test]
    async fn test_mark_hooks_completed() {
        let manager = create_test_manager();
        let mut checkpoint = manager.create_checkpoint("dep_123").await.unwrap();

        manager
            .mark_pre_hook_completed(&mut checkpoint, "migrate-db")
            .await
            .unwrap();
        manager
            .mark_post_hook_completed(&mut checkpoint, "notify-slack")
            .await
            .unwrap();

        assert!(checkpoint.is_pre_hook_completed("migrate-db"));
        assert!(checkpoint.is_post_hook_completed("notify-slack"));
    }

    #[tokio::test]
    async fn test_clear_checkpoint() {
        let manager = create_test_manager();
        manager.create_checkpoint("dep_123").await.unwrap();

        manager.clear_checkpoint("dep_123").await.unwrap();

        let result = manager.get_checkpoint("dep_123").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_resumable() {
        let manager = create_test_manager();

        manager.create_checkpoint("dep_1").await.unwrap();
        manager.create_checkpoint("dep_2").await.unwrap();

        let resumable = manager.list_resumable().await.unwrap();
        assert_eq!(resumable.len(), 2);
    }

    #[tokio::test]
    async fn test_can_resume() {
        let manager = create_test_manager();
        manager.create_checkpoint("dep_123").await.unwrap();

        assert!(manager.can_resume("dep_123").await.unwrap());
        assert!(!manager.can_resume("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_disabled_checkpointing() {
        let storage = Arc::new(InMemoryStorage::new());
        let manager = CheckpointManager::new(storage.clone())
            .with_config(CheckpointConfig {
                enabled: false,
                save_on_interrupt: false,
            });

        // Create should still work but not persist
        let checkpoint = manager.create_checkpoint("dep_123").await.unwrap();
        assert_eq!(checkpoint.deployment_id, "dep_123");

        // Storage should be empty
        let stored = storage.get_checkpoint("dep_123").await.unwrap();
        assert!(stored.is_none());
    }

    #[tokio::test]
    async fn test_set_error() {
        let manager = create_test_manager();
        let mut checkpoint = manager.create_checkpoint("dep_123").await.unwrap();

        manager
            .set_error(&mut checkpoint, "Connection refused")
            .await
            .unwrap();

        assert_eq!(
            checkpoint.error_message,
            Some("Connection refused".to_string())
        );
    }

    #[tokio::test]
    async fn test_create_with_context() {
        let manager = create_test_manager();
        let context = r#"{"environment":"production","image":"myapp:v1.0.0"}"#;

        let checkpoint = manager
            .create_with_context("dep_123", context)
            .await
            .unwrap();

        assert_eq!(checkpoint.context_snapshot, Some(context.to_string()));
    }
}
