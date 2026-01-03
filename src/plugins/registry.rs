use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::infrastructure::InfrastructureProvider;

use super::loader::{DefaultPluginLoader, Plugin, PluginLoader, PluginMetadata};

/// Trait for plugin registry operations.
pub trait PluginRegistryTrait: Send + Sync {
    fn register_plugin(&self, plugin: Box<dyn Plugin>) -> anyhow::Result<()>;
    fn unregister_plugin(&self, infrastructure_type: &str) -> anyhow::Result<()>;
    fn get_plugin(&self, infrastructure_type: &str) -> Option<Arc<dyn Plugin>>;
    fn list_plugins(&self) -> Vec<PluginMetadata>;
    fn create_provider(
        &self,
        infrastructure_type: &str,
        config: HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<Box<dyn InfrastructureProvider>>;
}

/// Registry for managing loaded plugins.
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
    loader: Box<dyn PluginLoader>,
}

impl PluginRegistry {
    pub fn new() -> anyhow::Result<Self> {
        let loader = DefaultPluginLoader::new()?;
        Ok(Self {
            plugins: RwLock::new(HashMap::new()),
            loader: Box::new(loader),
        })
    }

    pub fn with_loader(loader: Box<dyn PluginLoader>) -> Self {
        Self {
            plugins: RwLock::new(HashMap::new()),
            loader,
        }
    }

    pub fn with_plugin_directory(plugin_dir: PathBuf) -> Self {
        let loader = DefaultPluginLoader::with_directory(plugin_dir);
        Self {
            plugins: RwLock::new(HashMap::new()),
            loader: Box::new(loader),
        }
    }

    /// Load all plugins from the plugin directory.
    pub fn load_all(&self) -> anyhow::Result<()> {
        let loaded_plugins = self.loader.load_all_plugins()?;

        for plugin in loaded_plugins {
            self.register_plugin(plugin)?;
        }

        Ok(())
    }

    /// Load a single plugin from a file path.
    pub fn load_plugin(&self, path: &PathBuf) -> anyhow::Result<()> {
        let plugin = self.loader.load_plugin(path)?;
        self.register_plugin(plugin)
    }

    /// Get the plugin directory path.
    pub fn plugin_directory(&self) -> PathBuf {
        self.loader.plugin_directory()
    }

    /// Check if a plugin is registered for the given infrastructure type.
    pub fn has_plugin(&self, infrastructure_type: &str) -> bool {
        let plugins = self.plugins.read().unwrap();
        plugins.contains_key(infrastructure_type)
    }

    /// Get the number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        let plugins = self.plugins.read().unwrap();
        plugins.len()
    }

    /// Clear all registered plugins.
    pub fn clear(&self) {
        let mut plugins = self.plugins.write().unwrap();
        plugins.clear();
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new().expect("Failed to create default plugin registry")
    }
}

impl PluginRegistryTrait for PluginRegistry {
    fn register_plugin(&self, plugin: Box<dyn Plugin>) -> anyhow::Result<()> {
        let metadata = plugin.metadata();
        let infra_type = metadata.infrastructure_type.clone();

        let mut plugins = self.plugins.write().unwrap();

        if plugins.contains_key(&infra_type) {
            anyhow::bail!(
                "Plugin for infrastructure type '{}' is already registered",
                infra_type
            );
        }

        tracing::info!(
            "Registered plugin: {} v{} ({})",
            metadata.name,
            metadata.version,
            infra_type
        );

        plugins.insert(infra_type, Arc::from(plugin));
        Ok(())
    }

    fn unregister_plugin(&self, infrastructure_type: &str) -> anyhow::Result<()> {
        let mut plugins = self.plugins.write().unwrap();

        if plugins.remove(infrastructure_type).is_some() {
            tracing::info!("Unregistered plugin for: {}", infrastructure_type);
            Ok(())
        } else {
            anyhow::bail!(
                "No plugin registered for infrastructure type: {}",
                infrastructure_type
            )
        }
    }

    fn get_plugin(&self, infrastructure_type: &str) -> Option<Arc<dyn Plugin>> {
        let plugins = self.plugins.read().unwrap();
        plugins.get(infrastructure_type).cloned()
    }

    fn list_plugins(&self) -> Vec<PluginMetadata> {
        let plugins = self.plugins.read().unwrap();
        plugins.values().map(|p| p.metadata()).collect()
    }

    fn create_provider(
        &self,
        infrastructure_type: &str,
        config: HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<Box<dyn InfrastructureProvider>> {
        let plugin = self.get_plugin(infrastructure_type).ok_or_else(|| {
            anyhow::anyhow!(
                "No plugin registered for infrastructure type: {}",
                infrastructure_type
            )
        })?;

        plugin.create_provider(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_empty() {
        let temp_dir = TempDir::new().unwrap();
        let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

        assert_eq!(registry.plugin_count(), 0);
        assert!(registry.list_plugins().is_empty());
    }

    #[test]
    fn test_registry_has_plugin() {
        let temp_dir = TempDir::new().unwrap();
        let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

        assert!(!registry.has_plugin("nonexistent"));
    }

    #[test]
    fn test_registry_plugin_directory() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();
        let registry = PluginRegistry::with_plugin_directory(path.clone());

        assert_eq!(registry.plugin_directory(), path);
    }

    #[test]
    fn test_registry_clear() {
        let temp_dir = TempDir::new().unwrap();
        let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

        registry.clear();
        assert_eq!(registry.plugin_count(), 0);
    }
}
