//! Storage factory for creating storage backends.
//!
//! Supports selecting between different storage implementations based on configuration.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use super::file::FileStorage;
use super::memory::InMemoryStorage;
use super::sqlite::SqliteStorage;
use super::traits::Storage;

/// Available storage backends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// In-memory storage (default, non-persistent).
    #[default]
    Memory,
    /// File-based JSON storage.
    File,
    /// SQLite database storage.
    Sqlite,
}

impl StorageBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageBackend::Memory => "memory",
            StorageBackend::File => "file",
            StorageBackend::Sqlite => "sqlite",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "file" | "json" => StorageBackend::File,
            "sqlite" | "db" | "database" => StorageBackend::Sqlite,
            _ => StorageBackend::Memory,
        }
    }
}

/// Configuration for storage backends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Which backend to use.
    #[serde(default)]
    pub backend: StorageBackend,

    /// Custom path for file/sqlite storage.
    /// If not specified, uses default location.
    pub path: Option<PathBuf>,

    /// Retention period in days for old deployments.
    /// Set to 0 to disable cleanup.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::default(),
            path: None,
            retention_days: default_retention_days(),
        }
    }
}

fn default_retention_days() -> u32 {
    90
}

impl StorageConfig {
    pub fn memory() -> Self {
        Self {
            backend: StorageBackend::Memory,
            path: None,
            retention_days: default_retention_days(),
        }
    }

    pub fn file(path: Option<PathBuf>) -> Self {
        Self {
            backend: StorageBackend::File,
            path,
            retention_days: default_retention_days(),
        }
    }

    pub fn sqlite(path: Option<PathBuf>) -> Self {
        Self {
            backend: StorageBackend::Sqlite,
            path,
            retention_days: default_retention_days(),
        }
    }

    pub fn with_retention_days(mut self, days: u32) -> Self {
        self.retention_days = days;
        self
    }
}

/// Factory for creating storage backends.
pub struct StorageFactory;

impl StorageFactory {
    /// Create a storage backend based on configuration.
    pub async fn create(config: &StorageConfig) -> anyhow::Result<Arc<dyn Storage>> {
        let storage: Arc<dyn Storage> = match config.backend {
            StorageBackend::Memory => {
                info!("Using in-memory storage (non-persistent)");
                Arc::new(InMemoryStorage::new())
            }

            StorageBackend::File => {
                let storage = if let Some(path) = &config.path {
                    info!("Using file storage at: {}", path.display());
                    FileStorage::new(path.clone()).await?
                } else {
                    info!("Using file storage at default location");
                    FileStorage::default_location().await?
                };
                Arc::new(storage)
            }

            StorageBackend::Sqlite => {
                let storage = if let Some(path) = &config.path {
                    info!("Using SQLite storage at: {}", path.display());
                    SqliteStorage::new(path.clone())?
                } else {
                    info!("Using SQLite storage at default location");
                    SqliteStorage::default_location()?
                };
                Arc::new(storage)
            }
        };

        Ok(storage)
    }

    /// Create the default storage backend (SQLite at default location).
    pub async fn create_default() -> anyhow::Result<Arc<dyn Storage>> {
        Self::create(&StorageConfig::sqlite(None)).await
    }

    /// Create an in-memory storage (useful for testing).
    pub fn create_memory() -> Arc<dyn Storage> {
        Arc::new(InMemoryStorage::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_backend_from_str() {
        assert_eq!(StorageBackend::from_str("memory"), StorageBackend::Memory);
        assert_eq!(StorageBackend::from_str("file"), StorageBackend::File);
        assert_eq!(StorageBackend::from_str("json"), StorageBackend::File);
        assert_eq!(StorageBackend::from_str("sqlite"), StorageBackend::Sqlite);
        assert_eq!(StorageBackend::from_str("db"), StorageBackend::Sqlite);
        assert_eq!(StorageBackend::from_str("database"), StorageBackend::Sqlite);
        assert_eq!(StorageBackend::from_str("unknown"), StorageBackend::Memory);
    }

    #[test]
    fn test_storage_backend_as_str() {
        assert_eq!(StorageBackend::Memory.as_str(), "memory");
        assert_eq!(StorageBackend::File.as_str(), "file");
        assert_eq!(StorageBackend::Sqlite.as_str(), "sqlite");
    }

    #[test]
    fn test_storage_config_builders() {
        let config = StorageConfig::memory();
        assert_eq!(config.backend, StorageBackend::Memory);
        assert!(config.path.is_none());

        let config = StorageConfig::file(Some(PathBuf::from("/tmp/test")));
        assert_eq!(config.backend, StorageBackend::File);
        assert_eq!(config.path, Some(PathBuf::from("/tmp/test")));

        let config = StorageConfig::sqlite(None).with_retention_days(30);
        assert_eq!(config.backend, StorageBackend::Sqlite);
        assert_eq!(config.retention_days, 30);
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.backend, StorageBackend::Memory);
        assert_eq!(config.retention_days, default_retention_days());
    }

    #[tokio::test]
    async fn test_factory_create_memory() {
        let config = StorageConfig::memory();
        let storage = StorageFactory::create(&config).await.unwrap();
        assert_eq!(storage.name(), "in-memory");
    }

    #[test]
    fn test_factory_create_memory_sync() {
        let storage = StorageFactory::create_memory();
        assert_eq!(storage.name(), "in-memory");
    }

    #[tokio::test]
    async fn test_factory_create_sqlite_in_memory() {
        // Use a temp path for testing
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let config = StorageConfig::sqlite(Some(db_path));
        let storage = StorageFactory::create(&config).await.unwrap();
        assert_eq!(storage.name(), "sqlite");
    }

    #[tokio::test]
    async fn test_factory_create_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();

        let config = StorageConfig::file(Some(temp_dir.path().to_path_buf()));
        let storage = StorageFactory::create(&config).await.unwrap();
        assert_eq!(storage.name(), "file");
    }

    #[test]
    fn test_storage_config_serialization() {
        let config = StorageConfig::sqlite(Some(PathBuf::from("/data/history.db")))
            .with_retention_days(60);

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("sqlite"));
        assert!(yaml.contains("60"));

        let parsed: StorageConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.backend, StorageBackend::Sqlite);
        assert_eq!(parsed.retention_days, 60);
    }
}
