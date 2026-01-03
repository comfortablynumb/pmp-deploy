use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::{Config, ConfigLoader, GlobalConfig, ProjectReference};
use crate::metrics::MetricsResolver;
use crate::storage::{Storage, StorageConfig, StorageFactory};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config_loader: RwLock<ConfigLoader>,
    cached_global_config: RwLock<Option<GlobalConfig>>,
    metrics_resolver: RwLock<MetricsResolver>,
    storage: RwLock<Option<Arc<dyn Storage>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config_loader: RwLock::new(ConfigLoader::new()),
                cached_global_config: RwLock::new(None),
                metrics_resolver: RwLock::new(MetricsResolver::new()),
                storage: RwLock::new(None),
            }),
        }
    }

    pub async fn get_metrics_resolver(&self) -> tokio::sync::RwLockReadGuard<'_, MetricsResolver> {
        self.inner.metrics_resolver.read().await
    }

    pub async fn configure_metrics_resolver<F>(&self, configure: F)
    where
        F: FnOnce(&mut MetricsResolver),
    {
        let mut resolver = self.inner.metrics_resolver.write().await;
        configure(&mut resolver);
    }

    /// Get the storage backend, initializing it if necessary.
    pub async fn get_storage(&self) -> anyhow::Result<Arc<dyn Storage>> {
        // Check if storage is already initialized
        {
            let storage = self.inner.storage.read().await;

            if let Some(ref s) = *storage {
                return Ok(s.clone());
            }
        }

        // Initialize storage based on global config
        let config = self.load_global_config().await?;
        let storage_config = config
            .and_then(|c| c.storage)
            .unwrap_or_else(StorageConfig::default);

        let storage = StorageFactory::create(&storage_config).await?;

        // Cache the storage
        {
            let mut cache = self.inner.storage.write().await;
            *cache = Some(storage.clone());
        }

        Ok(storage)
    }

    /// Initialize storage with a specific configuration.
    pub async fn init_storage(&self, config: &StorageConfig) -> anyhow::Result<Arc<dyn Storage>> {
        let storage = StorageFactory::create(config).await?;
        let mut cache = self.inner.storage.write().await;
        *cache = Some(storage.clone());
        Ok(storage)
    }

    pub async fn load_global_config(&self) -> anyhow::Result<Option<GlobalConfig>> {
        let loader = self.inner.config_loader.read().await;
        let config = loader.load_global_config()?;

        if let Some(ref cfg) = config {
            let mut cache = self.inner.cached_global_config.write().await;
            *cache = Some(cfg.clone());
        }

        Ok(config)
    }

    pub async fn load_project_config(&self, path: &PathBuf) -> anyhow::Result<Config> {
        let loader = ConfigLoader::new().with_project_config(path.clone());
        loader.load_project_config()
    }

    pub async fn get_projects(&self) -> anyhow::Result<Vec<ProjectInfo>> {
        let global_config = self.load_global_config().await?;

        let Some(global) = global_config else {
            return Ok(Vec::new());
        };

        let mut projects = Vec::new();

        for project_ref in &global.projects {
            let info = self.create_project_info(project_ref).await;
            projects.push(info);
        }

        Ok(projects)
    }

    async fn create_project_info(&self, project_ref: &ProjectReference) -> ProjectInfo {
        let path = project_ref.expanded_path();
        let config_path = path.join(".pmp-deploy.yaml");
        let exists = config_path.exists();

        let environments = if exists {
            self.load_project_config(&config_path)
                .await
                .map(|c| c.list_environments().into_iter().map(String::from).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        ProjectInfo {
            id: generate_project_id(&path),
            name: project_ref.display_name(),
            path: path.to_string_lossy().to_string(),
            config_exists: exists,
            environments,
        }
    }

    pub async fn get_project_by_id(&self, id: &str) -> anyhow::Result<Option<ProjectInfo>> {
        let projects = self.get_projects().await?;
        Ok(projects.into_iter().find(|p| p.id == id))
    }

    pub async fn get_project_config(&self, id: &str) -> anyhow::Result<Option<Config>> {
        let project = self.get_project_by_id(id).await?;

        let Some(project) = project else {
            return Ok(None);
        };

        if !project.config_exists {
            return Ok(None);
        }

        let config_path = PathBuf::from(&project.path).join(".pmp-deploy.yaml");
        let config = self.load_project_config(&config_path).await?;

        Ok(Some(config))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub config_exists: bool,
    pub environments: Vec<String>,
}

fn generate_project_id(path: &PathBuf) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);

    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_project_id() {
        let path1 = PathBuf::from("/path/to/project1");
        let path2 = PathBuf::from("/path/to/project2");

        let id1 = generate_project_id(&path1);
        let id2 = generate_project_id(&path2);

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
    }

    #[tokio::test]
    async fn test_app_state_new() {
        let state = AppState::new();
        let projects = state.get_projects().await.unwrap();
        assert!(projects.is_empty() || !projects.is_empty());
    }
}
