use async_trait::async_trait;
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::Arc;

use crate::deployment::{DeploymentResult, DeploymentType};
use crate::infrastructure::{DeploymentContext, InfrastructureProvider, InfrastructureType};

use super::ffi::*;

#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub infrastructure_type: String,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;

    fn create_provider(
        &self,
        config: HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<Box<dyn InfrastructureProvider>>;
}

pub trait PluginLoader: Send + Sync {
    fn plugin_directory(&self) -> PathBuf;

    fn discover_plugins(&self) -> anyhow::Result<Vec<PathBuf>>;

    fn load_plugin(&self, path: &PathBuf) -> anyhow::Result<Box<dyn Plugin>>;

    fn load_all_plugins(&self) -> anyhow::Result<Vec<Box<dyn Plugin>>> {
        let paths = self.discover_plugins()?;
        let mut plugins = Vec::new();

        for path in paths {
            match self.load_plugin(&path) {
                Ok(plugin) => plugins.push(plugin),
                Err(e) => {
                    tracing::warn!("Failed to load plugin at {:?}: {}", path, e);
                }
            }
        }

        Ok(plugins)
    }
}

pub struct DynamicPlugin {
    metadata: PluginMetadata,
    library: Arc<Library>,
    #[allow(dead_code)]
    path: PathBuf,
}

impl DynamicPlugin {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let library = unsafe { Library::new(path) }
            .map_err(|e| anyhow::anyhow!("Failed to load library {:?}: {}", path, e))?;

        let api_version = Self::get_api_version(&library)?;

        if api_version != PLUGIN_API_VERSION {
            anyhow::bail!(
                "Plugin API version mismatch: expected {}, got {}",
                PLUGIN_API_VERSION,
                api_version
            );
        }

        let metadata = Self::get_metadata(&library)?;
        Self::initialize_plugin(&library)?;

        Ok(Self {
            metadata,
            library: Arc::new(library),
            path: path.clone(),
        })
    }

    fn get_api_version(library: &Library) -> anyhow::Result<u32> {
        unsafe {
            let func: Symbol<PluginApiVersionFn> = library
                .get(SYMBOL_API_VERSION)
                .map_err(|e| anyhow::anyhow!("Missing pmp_plugin_api_version symbol: {}", e))?;
            Ok(func())
        }
    }

    fn get_metadata(library: &Library) -> anyhow::Result<PluginMetadata> {
        unsafe {
            let func: Symbol<PluginMetadataFn> = library
                .get(SYMBOL_METADATA)
                .map_err(|e| anyhow::anyhow!("Missing pmp_plugin_metadata symbol: {}", e))?;

            let ffi_metadata = func();

            Ok(PluginMetadata {
                name: ffi_metadata.name.as_str().to_string(),
                version: ffi_metadata.version.as_str().to_string(),
                description: ffi_metadata.description.as_str().to_string(),
                infrastructure_type: ffi_metadata.infrastructure_type.as_str().to_string(),
            })
        }
    }

    fn initialize_plugin(library: &Library) -> anyhow::Result<()> {
        unsafe {
            if let Ok(func) = library.get::<PluginInitFn>(SYMBOL_INIT) {
                let result = func();

                if !result.success {
                    anyhow::bail!("Plugin initialization failed: {}", result.error.as_str());
                }
            }
        }
        Ok(())
    }
}

impl Drop for DynamicPlugin {
    fn drop(&mut self) {
        unsafe {
            if let Ok(func) = self.library.get::<PluginShutdownFn>(SYMBOL_SHUTDOWN) {
                func();
            }
        }
    }
}

impl Plugin for DynamicPlugin {
    fn metadata(&self) -> PluginMetadata {
        self.metadata.clone()
    }

    fn create_provider(
        &self,
        config: HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<Box<dyn InfrastructureProvider>> {
        Ok(Box::new(DynamicPluginProvider::new(
            self.library.clone(),
            self.metadata.clone(),
            config,
        )))
    }
}

struct DynamicPluginProvider {
    library: Arc<Library>,
    metadata: PluginMetadata,
    config: HashMap<String, serde_yaml::Value>,
}

impl DynamicPluginProvider {
    fn new(
        library: Arc<Library>,
        metadata: PluginMetadata,
        config: HashMap<String, serde_yaml::Value>,
    ) -> Self {
        Self {
            library,
            metadata,
            config,
        }
    }

    fn create_ffi_context(&self, ctx: &DeploymentContext) -> FfiDeploymentContext {
        FfiDeploymentContext::from_context(
            &ctx.environment_name,
            &self.metadata.infrastructure_type,
            &self.config,
            ctx.dry_run,
            ctx.verbose,
        )
    }
}

#[async_trait]
impl InfrastructureProvider for DynamicPluginProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::Custom(self.metadata.infrastructure_type.clone())
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        // Plugins define their own behavior, so we allow all deployment types
        // The plugin itself should validate and handle unsupported types
        vec![DeploymentType::RollingUpdate, DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        config: &HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        let config_json =
            serde_json::to_string(config).map_err(|e| anyhow::anyhow!("JSON error: {}", e))?;

        let c_string =
            CString::new(config_json).map_err(|e| anyhow::anyhow!("CString error: {}", e))?;

        unsafe {
            let func: Symbol<PluginValidateConfigFn> = self
                .library
                .get(SYMBOL_VALIDATE_CONFIG)
                .map_err(|e| anyhow::anyhow!("Missing validate_config symbol: {}", e))?;

            let result = func(c_string.as_ptr());

            if result.success {
                Ok(())
            } else {
                Err(anyhow::anyhow!(result.error.into_string()))
            }
        }
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let ffi_ctx = self.create_ffi_context(ctx);

        unsafe {
            let func: Symbol<PluginDeployFn> = self
                .library
                .get(SYMBOL_DEPLOY)
                .map_err(|e| anyhow::anyhow!("Missing deploy symbol: {}", e))?;

            let result = func(&ffi_ctx);

            if result.success {
                Ok(DeploymentResult {
                    success: true,
                    message: result.message.into_string(),
                    version: Some(result.version.into_string()),
                    rollback_version: {
                        let rb = result.rollback_version.into_string();

                        if rb.is_empty() {
                            None
                        } else {
                            Some(rb)
                        }
                    },
                })
            } else {
                Ok(DeploymentResult {
                    success: false,
                    message: result.message.into_string(),
                    version: None,
                    rollback_version: None,
                })
            }
        }
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let ffi_ctx = self.create_ffi_context(ctx);

        unsafe {
            let func: Symbol<PluginRollbackFn> = self
                .library
                .get(SYMBOL_ROLLBACK)
                .map_err(|e| anyhow::anyhow!("Missing rollback symbol: {}", e))?;

            let result = func(&ffi_ctx);

            if result.success {
                Ok(DeploymentResult {
                    success: true,
                    message: result.message.into_string(),
                    version: Some(result.version.into_string()),
                    rollback_version: None,
                })
            } else {
                Ok(DeploymentResult {
                    success: false,
                    message: result.message.into_string(),
                    version: None,
                    rollback_version: None,
                })
            }
        }
    }

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let ffi_ctx = self.create_ffi_context(ctx);

        unsafe {
            let func: Symbol<PluginStatusFn> = self
                .library
                .get(SYMBOL_STATUS)
                .map_err(|e| anyhow::anyhow!("Missing status symbol: {}", e))?;

            let result = func(&ffi_ctx);
            result.into_result()
        }
    }

    async fn logs(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        let ffi_ctx = self.create_ffi_context(ctx);

        unsafe {
            let func: Symbol<PluginLogsFn> = self
                .library
                .get(SYMBOL_LOGS)
                .map_err(|e| anyhow::anyhow!("Missing logs symbol: {}", e))?;

            let result = func(&ffi_ctx, follow);

            if result.success {
                println!("{}", result.data.into_string());
                Ok(())
            } else {
                Err(anyhow::anyhow!(result.error.into_string()))
            }
        }
    }
}

pub struct DefaultPluginLoader {
    plugin_dir: PathBuf,
}

impl DefaultPluginLoader {
    pub fn new() -> anyhow::Result<Self> {
        let home = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let plugin_dir = home.home_dir().join(".pmp-deploy").join("plugins");

        Ok(Self { plugin_dir })
    }

    pub fn with_directory(plugin_dir: PathBuf) -> Self {
        Self { plugin_dir }
    }
}

impl Default for DefaultPluginLoader {
    fn default() -> Self {
        Self::new().expect("Failed to create default plugin loader")
    }
}

impl PluginLoader for DefaultPluginLoader {
    fn plugin_directory(&self) -> PathBuf {
        self.plugin_dir.clone()
    }

    fn discover_plugins(&self) -> anyhow::Result<Vec<PathBuf>> {
        if !self.plugin_dir.exists() {
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();

        for entry in std::fs::read_dir(&self.plugin_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let extension = path.extension().and_then(|e| e.to_str());

                if is_plugin_extension(extension) {
                    plugins.push(path);
                }
            }
        }

        Ok(plugins)
    }

    fn load_plugin(&self, path: &PathBuf) -> anyhow::Result<Box<dyn Plugin>> {
        let plugin = DynamicPlugin::load(path)?;
        Ok(Box::new(plugin))
    }
}

fn is_plugin_extension(ext: Option<&str>) -> bool {
    match ext {
        Some("so") => true,   // Linux
        Some("dll") => true,  // Windows
        Some("dylib") => true, // macOS
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_loader_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
        let plugins = loader.discover_plugins().unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_plugin_loader_nonexistent_directory() {
        let loader = DefaultPluginLoader::with_directory(PathBuf::from("/nonexistent/path"));
        let plugins = loader.discover_plugins().unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_is_plugin_extension() {
        assert!(is_plugin_extension(Some("so")));
        assert!(is_plugin_extension(Some("dll")));
        assert!(is_plugin_extension(Some("dylib")));
        assert!(!is_plugin_extension(Some("txt")));
        assert!(!is_plugin_extension(Some("rs")));
        assert!(!is_plugin_extension(None));
    }
}
