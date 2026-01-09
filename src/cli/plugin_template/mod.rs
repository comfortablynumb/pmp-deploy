//! Plugin template generator module.
//!
//! Generates a new pmp-deploy plugin scaffold with all necessary files.

mod generator;

pub use generator::PluginGenerator;

use crate::cli::commands::PluginNewArgs;

/// Handle the `plugin new` command.
pub async fn handle_new(args: &PluginNewArgs) -> anyhow::Result<()> {
    let generator = PluginGenerator::new(args);
    generator.generate()?;

    println!(
        "Created plugin '{}' successfully!",
        generator.plugin_name()
    );
    println!();
    println!("Next steps:");
    println!("  1. cd {}", generator.output_dir().display());
    println!("  2. cargo build --release");
    println!(
        "  3. Copy target/release/{}.dll (Windows) or lib{}.so (Linux) to ~/.pmp-deploy/plugins/",
        generator.crate_name(),
        generator.crate_name()
    );
    println!();
    println!(
        "Then use infrastructure type '{}' in your pmp-deploy.yaml.",
        args.infrastructure_type
    );

    Ok(())
}

/// Handle the `plugin list` command.
pub async fn handle_list() -> anyhow::Result<()> {
    use crate::plugins::{PluginRegistry, PluginRegistryTrait};

    let registry = PluginRegistry::new()?;

    // Load all plugins first
    if let Err(e) = registry.load_all() {
        tracing::debug!("Failed to load plugins: {}", e);
    }

    let plugins = registry.list_plugins();
    let plugin_dir = registry.plugin_directory();

    if plugins.is_empty() {
        println!("No plugins installed.");
        println!();
        println!("Install plugins by copying them to: {}", plugin_dir.display());
        return Ok(());
    }

    println!("Installed plugins:");
    println!();

    for plugin in plugins {
        println!(
            "  {} v{} - {}",
            plugin.name, plugin.version, plugin.description
        );
        println!("    Infrastructure type: {}", plugin.infrastructure_type);
        println!();
    }

    Ok(())
}
