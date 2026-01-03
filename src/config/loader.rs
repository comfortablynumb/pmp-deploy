use std::path::{Path, PathBuf};

use super::schema::{Config, GlobalConfig};

const PROJECT_CONFIG_FILENAME: &str = ".pmp-deploy.yaml";
const GLOBAL_CONFIG_FILENAME: &str = ".pmp-deploy.yaml";

pub struct ConfigLoader {
    project_config_path: Option<PathBuf>,
    global_config_path: Option<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            project_config_path: None,
            global_config_path: None,
        }
    }

    pub fn with_project_config(mut self, path: PathBuf) -> Self {
        self.project_config_path = Some(path);
        self
    }

    pub fn with_global_config(mut self, path: PathBuf) -> Self {
        self.global_config_path = Some(path);
        self
    }

    pub fn load_project_config(&self) -> anyhow::Result<Config> {
        let path = self.resolve_project_config_path()?;
        self.load_config_from_path(&path)
    }

    pub fn load_global_config(&self) -> anyhow::Result<Option<GlobalConfig>> {
        let path = self.resolve_global_config_path();

        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let expanded = expand_env_vars(&content);
        let config: GlobalConfig = serde_yaml::from_str(&expanded)?;

        Ok(Some(config))
    }

    pub fn load_config_from_path(&self, path: &Path) -> anyhow::Result<Config> {
        if !path.exists() {
            anyhow::bail!("Configuration file not found: {}", path.display());
        }

        let content = std::fs::read_to_string(path)?;
        let expanded = expand_env_vars(&content);
        let config: Config = serde_yaml::from_str(&expanded)?;

        Ok(config)
    }

    fn resolve_project_config_path(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = &self.project_config_path {
            return Ok(path.clone());
        }

        let current_dir = std::env::current_dir()?;
        let config_path = current_dir.join(PROJECT_CONFIG_FILENAME);

        if config_path.exists() {
            return Ok(config_path);
        }

        anyhow::bail!(
            "No {} found in current directory: {}",
            PROJECT_CONFIG_FILENAME,
            current_dir.display()
        )
    }

    fn resolve_global_config_path(&self) -> PathBuf {
        if let Some(path) = &self.global_config_path {
            return path.clone();
        }

        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().join(GLOBAL_CONFIG_FILENAME);
        }

        PathBuf::from(GLOBAL_CONFIG_FILENAME)
    }

    pub fn find_project_configs(&self) -> anyhow::Result<Vec<PathBuf>> {
        let global_config = self.load_global_config()?;

        let Some(global) = global_config else {
            return Ok(Vec::new());
        };

        let mut configs = Vec::new();

        for project in &global.projects {
            let expanded_path = project.expanded_path();
            let config_path = expanded_path.join(PROJECT_CONFIG_FILENAME);

            if config_path.exists() {
                configs.push(config_path);
            } else {
                tracing::warn!(
                    "Project config not found for {}: {}",
                    project.display_name(),
                    config_path.display()
                );
            }
        }

        Ok(configs)
    }

    pub fn project_config_exists(&self) -> bool {
        self.resolve_project_config_path()
            .map(|p| p.exists())
            .unwrap_or(false)
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

fn expand_env_vars(content: &str) -> String {
    shellexpand::full(content)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_project_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(PROJECT_CONFIG_FILENAME);

        let config_content = r#"
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml

environments:
  dev:
    infrastructure: local
    image: my-app:latest
"#;
        std::fs::write(&config_path, config_content).unwrap();

        let loader = ConfigLoader::new().with_project_config(config_path);
        let config = loader.load_project_config().unwrap();

        assert!(config.infrastructure.contains_key("local"));
        assert!(config.environments.contains_key("dev"));
    }

    #[test]
    fn test_load_global_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(GLOBAL_CONFIG_FILENAME);

        let config_content = r#"
projects:
  - path: /path/to/project1
    name: project1
  - path: /path/to/project2
"#;
        std::fs::write(&config_path, config_content).unwrap();

        let loader = ConfigLoader::new().with_global_config(config_path);
        let config = loader.load_global_config().unwrap().unwrap();

        assert_eq!(config.projects.len(), 2);
    }

    #[test]
    fn test_env_var_expansion() {
        // SAFETY: This test runs in isolation and only sets a test-specific env var
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
        }

        let content = "value: $TEST_VAR";
        let expanded = expand_env_vars(content);

        assert_eq!(expanded, "value: test_value");
    }
}
