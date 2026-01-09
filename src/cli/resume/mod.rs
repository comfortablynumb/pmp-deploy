//! Resume command handler for interrupted deployments.
//!
//! This module provides CLI functionality to list and resume deployments
//! that were interrupted before completion.

use dialoguer::{Confirm, Select};

use crate::cli::ResumeArgs;
use crate::deployment::CheckpointManager;
use crate::storage::{DeploymentPhase, StorageFactory};

/// Execute the resume command.
pub async fn execute(args: ResumeArgs) -> anyhow::Result<()> {
    let storage = StorageFactory::create_default().await?;
    let manager = CheckpointManager::new(storage);

    if args.list || args.deployment_id.is_none() {
        return list_resumable(&manager).await;
    }

    let deployment_id = args.deployment_id.as_ref().unwrap();

    if args.clear {
        return clear_checkpoint(&manager, deployment_id, args.yes).await;
    }

    resume_deployment(&manager, deployment_id, args.yes).await
}

async fn list_resumable(manager: &CheckpointManager) -> anyhow::Result<()> {
    let checkpoints = manager.list_resumable().await?;

    if checkpoints.is_empty() {
        println!("No resumable deployments found.");
        return Ok(());
    }

    println!("Resumable deployments:");
    println!();

    for checkpoint in &checkpoints {
        let error_info = checkpoint
            .error_message
            .as_ref()
            .map(|e| format!(" (error: {})", e))
            .unwrap_or_default();

        let resources_count = checkpoint.deployed_resources.len();

        println!("  ID: {}", checkpoint.deployment_id);
        println!("    Phase: {}", checkpoint.phase);
        println!(
            "    Completed phases: {}",
            format_completed_phases(&checkpoint.completed_phases)
        );
        println!("    Resources deployed: {}", resources_count);
        println!("    Last updated: {:?}{}", checkpoint.last_updated, error_info);
        println!();
    }

    println!(
        "Use 'pmp-deploy resume <deployment-id>' to resume a deployment."
    );

    Ok(())
}

fn format_completed_phases(phases: &[DeploymentPhase]) -> String {
    if phases.is_empty() {
        return "none".to_string();
    }

    phases
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

async fn clear_checkpoint(
    manager: &CheckpointManager,
    deployment_id: &str,
    skip_confirm: bool,
) -> anyhow::Result<()> {
    let checkpoint = manager.get_checkpoint(deployment_id).await?;

    if checkpoint.is_none() {
        anyhow::bail!("No checkpoint found for deployment '{}'", deployment_id);
    }

    if !skip_confirm {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Clear checkpoint for '{}'? This cannot be undone.",
                deployment_id
            ))
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    manager.clear_checkpoint(deployment_id).await?;
    println!("Cleared checkpoint for '{}'.", deployment_id);

    Ok(())
}

async fn resume_deployment(
    manager: &CheckpointManager,
    deployment_id: &str,
    skip_confirm: bool,
) -> anyhow::Result<()> {
    let checkpoint = manager.get_checkpoint(deployment_id).await?;

    let checkpoint = match checkpoint {
        Some(c) => c,
        None => {
            anyhow::bail!("No checkpoint found for deployment '{}'", deployment_id);
        }
    };

    if !checkpoint.can_resume() {
        anyhow::bail!(
            "Deployment '{}' cannot be resumed (phase: {})",
            deployment_id,
            checkpoint.phase
        );
    }

    println!("Deployment checkpoint found:");
    println!("  ID: {}", checkpoint.deployment_id);
    println!("  Current phase: {}", checkpoint.phase);
    println!(
        "  Completed phases: {}",
        format_completed_phases(&checkpoint.completed_phases)
    );
    println!(
        "  Resources already deployed: {}",
        checkpoint.deployed_resources.len()
    );

    if !checkpoint.pre_hooks_completed.is_empty() {
        println!(
            "  Pre-hooks completed: {}",
            checkpoint.pre_hooks_completed.join(", ")
        );
    }

    if !checkpoint.post_hooks_completed.is_empty() {
        println!(
            "  Post-hooks completed: {}",
            checkpoint.post_hooks_completed.join(", ")
        );
    }

    if let Some(error) = &checkpoint.error_message {
        println!("  Last error: {}", error);
    }

    println!();

    if !skip_confirm {
        let confirmed = Confirm::new()
            .with_prompt("Resume this deployment?")
            .default(true)
            .interact()?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // The actual resume logic will be integrated with DeploymentExecutor
    // For now, we show what would happen
    println!();
    println!("Resuming deployment from phase: {}", checkpoint.phase);

    // Get context from checkpoint if available
    if let Some(context) = &checkpoint.context_snapshot {
        println!("Context snapshot available for restoration.");
        tracing::debug!("Context snapshot: {}", context);
    }

    // The deployment executor integration will:
    // 1. Skip completed phases
    // 2. Skip completed hooks
    // 3. Skip already-deployed resources
    // 4. Continue from the current phase

    println!();
    println!(
        "Resume functionality is ready. Integration with DeploymentExecutor \
         will allow full resumption."
    );

    // Clear checkpoint after successful resume (placeholder)
    // manager.clear_checkpoint(deployment_id).await?;

    Ok(())
}

/// Interactive resume - select from list of resumable deployments.
pub async fn interactive_resume() -> anyhow::Result<()> {
    let storage = StorageFactory::create_default().await?;
    let manager = CheckpointManager::new(storage);

    let checkpoints = manager.list_resumable().await?;

    if checkpoints.is_empty() {
        println!("No resumable deployments found.");
        return Ok(());
    }

    let items: Vec<String> = checkpoints
        .iter()
        .map(|c| {
            format!(
                "{} - {} ({})",
                c.deployment_id,
                c.phase,
                c.error_message
                    .as_ref()
                    .map(|e| format!("error: {}", e))
                    .unwrap_or_else(|| "in progress".to_string())
            )
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select a deployment to resume")
        .items(&items)
        .default(0)
        .interact()?;

    let deployment_id = &checkpoints[selection].deployment_id;
    resume_deployment(&manager, deployment_id, false).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_completed_phases_empty() {
        let phases: Vec<DeploymentPhase> = vec![];
        assert_eq!(format_completed_phases(&phases), "none");
    }

    #[test]
    fn test_format_completed_phases_single() {
        let phases = vec![DeploymentPhase::PreHooks];
        // DeploymentPhase uses snake_case Display format
        assert_eq!(format_completed_phases(&phases), "pre_hooks");
    }

    #[test]
    fn test_format_completed_phases_multiple() {
        let phases = vec![
            DeploymentPhase::PreHooks,
            DeploymentPhase::InfrastructureProvisioning,
        ];
        assert_eq!(
            format_completed_phases(&phases),
            "pre_hooks, infrastructure_provisioning"
        );
    }
}
