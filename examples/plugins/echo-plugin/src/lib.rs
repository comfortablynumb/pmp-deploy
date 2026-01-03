//! Echo Plugin for pmp-deploy
//!
//! This is an example plugin that demonstrates how to create
//! infrastructure plugins using the pmp-deploy SDK.

use pmp_deploy_plugin_sdk::*;

/// Echo plugin that simulates deployment operations.
#[derive(Default)]
pub struct EchoPlugin;

impl InfrastructurePlugin for EchoPlugin {
    fn validate_config(&self, config: &PluginConfig) -> PluginResult<()> {
        // Check for required fields
        if !config.contains_key("target") {
            return Err(PluginError::ConfigError(
                "Missing required field: 'target'".to_string(),
            ));
        }

        Ok(())
    }

    fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
        let target = ctx
            .config
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if ctx.dry_run {
            return Ok(DeploymentResult::success(
                &format!(
                    "[DRY RUN] Would deploy to '{}' on target '{}'",
                    ctx.environment_name, target
                ),
                "v0.0.0-dry-run",
            ));
        }

        // Simulate deployment
        if ctx.verbose {
            println!(
                "[echo-plugin] Deploying to environment '{}' on target '{}'",
                ctx.environment_name, target
            );
        }

        Ok(DeploymentResult::success_with_rollback(
            &format!(
                "Successfully deployed to '{}' on target '{}'",
                ctx.environment_name, target
            ),
            "v1.0.0",
            "v0.9.0",
        ))
    }

    fn rollback(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
        let target = ctx
            .config
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if ctx.verbose {
            println!(
                "[echo-plugin] Rolling back environment '{}' on target '{}'",
                ctx.environment_name, target
            );
        }

        Ok(DeploymentResult::success(
            &format!(
                "Successfully rolled back '{}' on target '{}'",
                ctx.environment_name, target
            ),
            "v0.9.0",
        ))
    }

    fn status(&self, ctx: &DeploymentContext) -> PluginResult<String> {
        let target = ctx
            .config
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        Ok(format!(
            "Environment '{}' on target '{}': Running (v1.0.0)",
            ctx.environment_name, target
        ))
    }

    fn logs(&self, ctx: &DeploymentContext, follow: bool) -> PluginResult<String> {
        let target = ctx
            .config
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let follow_str = if follow { " (following)" } else { "" };

        Ok(format!(
            "[echo-plugin] Logs for '{}' on target '{}'{}:\n\
             2024-01-01 00:00:00 INFO  Application started\n\
             2024-01-01 00:00:01 INFO  Listening on port 8080\n\
             2024-01-01 00:00:02 INFO  Ready to accept connections",
            ctx.environment_name, target, follow_str
        ))
    }

    fn init(&self) -> PluginResult<()> {
        println!("[echo-plugin] Plugin initialized");
        Ok(())
    }

    fn shutdown(&self) {
        println!("[echo-plugin] Plugin shutting down");
    }
}

// Export the plugin using the declare_plugin! macro
declare_plugin!(
    name: "echo-plugin",
    version: "0.1.0",
    description: "Example echo plugin for testing and demonstration",
    infrastructure_type: "echo",
    plugin: EchoPlugin,
);
