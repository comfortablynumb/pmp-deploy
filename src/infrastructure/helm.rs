use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;

use crate::deployment::DeploymentResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmConfig {
    pub chart: String,

    #[serde(default)]
    pub repo: Option<String>,

    #[serde(default)]
    pub repo_name: Option<String>,

    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub release_name: Option<String>,

    #[serde(default)]
    pub values_files: Vec<String>,

    #[serde(default)]
    pub set: HashMap<String, String>,

    #[serde(default)]
    pub set_string: HashMap<String, String>,

    #[serde(default)]
    pub create_namespace: bool,

    #[serde(default)]
    pub wait: bool,

    #[serde(default)]
    pub timeout: Option<String>,

    #[serde(default)]
    pub atomic: bool,

    #[serde(default)]
    pub skip_crds: bool,
}

impl HelmConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    pub fn merge(&self, overrides: &HelmConfig) -> HelmConfig {
        let mut set = self.set.clone();
        set.extend(overrides.set.clone());

        let mut set_string = self.set_string.clone();
        set_string.extend(overrides.set_string.clone());

        let mut values_files = self.values_files.clone();
        values_files.extend(overrides.values_files.clone());

        HelmConfig {
            chart: if overrides.chart.is_empty() {
                self.chart.clone()
            } else {
                overrides.chart.clone()
            },
            repo: overrides.repo.clone().or_else(|| self.repo.clone()),
            repo_name: overrides.repo_name.clone().or_else(|| self.repo_name.clone()),
            version: overrides.version.clone().or_else(|| self.version.clone()),
            release_name: overrides
                .release_name
                .clone()
                .or_else(|| self.release_name.clone()),
            values_files,
            set,
            set_string,
            create_namespace: overrides.create_namespace || self.create_namespace,
            wait: overrides.wait || self.wait,
            timeout: overrides.timeout.clone().or_else(|| self.timeout.clone()),
            atomic: overrides.atomic || self.atomic,
            skip_crds: overrides.skip_crds || self.skip_crds,
        }
    }
}

pub trait CommandExecutor: Send + Sync {
    fn execute(
        &self,
        program: &str,
        args: &[String],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<CommandOutput>> + Send + '_>,
    >;
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub struct RealCommandExecutor;

impl CommandExecutor for RealCommandExecutor {
    fn execute(
        &self,
        program: &str,
        args: &[String],
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<CommandOutput>> + Send + '_>,
    > {
        let program = program.to_string();
        let args = args.to_vec();

        Box::pin(async move {
            let output = Command::new(&program).args(&args).output().await?;

            Ok(CommandOutput {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        })
    }
}

pub struct HelmDeployer {
    namespace: String,
    context: Option<String>,
    working_dir: Option<PathBuf>,
    executor: Box<dyn CommandExecutor>,
}

impl HelmDeployer {
    pub fn new(namespace: &str, context: Option<&str>) -> Self {
        Self {
            namespace: namespace.to_string(),
            context: context.map(String::from),
            working_dir: None,
            executor: Box::new(RealCommandExecutor),
        }
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    #[cfg(test)]
    pub fn with_executor(mut self, executor: Box<dyn CommandExecutor>) -> Self {
        self.executor = executor;
        self
    }

    pub async fn check_helm_installed(&self) -> anyhow::Result<()> {
        let output = self.executor.execute("helm", &["version".to_string()]).await?;

        if !output.success {
            anyhow::bail!(
                "Helm is not installed or not in PATH. Please install Helm: https://helm.sh/docs/intro/install/"
            );
        }

        Ok(())
    }

    pub async fn add_repo(&self, name: &str, url: &str) -> anyhow::Result<()> {
        tracing::info!("Adding Helm repository {} from {}", name, url);

        let args = vec![
            "repo".to_string(),
            "add".to_string(),
            name.to_string(),
            url.to_string(),
            "--force-update".to_string(),
        ];

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Failed to add Helm repo: {}", output.stderr);
        }

        self.update_repos().await
    }

    pub async fn update_repos(&self) -> anyhow::Result<()> {
        tracing::info!("Updating Helm repositories");

        let output = self
            .executor
            .execute("helm", &["repo".to_string(), "update".to_string()])
            .await?;

        if !output.success {
            tracing::warn!("Failed to update Helm repos: {}", output.stderr);
        }

        Ok(())
    }

    pub async fn install_or_upgrade(
        &self,
        config: &HelmConfig,
        dry_run: bool,
    ) -> anyhow::Result<DeploymentResult> {
        self.check_helm_installed().await?;

        let release_name = config
            .release_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("release_name is required for Helm deployments"))?;

        if let (Some(repo_name), Some(repo_url)) = (&config.repo_name, &config.repo) {
            self.add_repo(repo_name, repo_url).await?;
        }

        let mut args = vec!["upgrade".to_string(), "--install".to_string()];

        args.push(release_name.clone());
        args.push(config.chart.clone());

        args.extend(vec!["--namespace".to_string(), self.namespace.clone()]);

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        if let Some(version) = &config.version {
            args.extend(vec!["--version".to_string(), version.clone()]);
        }

        for values_file in &config.values_files {
            args.extend(vec!["-f".to_string(), values_file.clone()]);
        }

        for (key, value) in &config.set {
            args.extend(vec!["--set".to_string(), format!("{}={}", key, value)]);
        }

        for (key, value) in &config.set_string {
            args.extend(vec![
                "--set-string".to_string(),
                format!("{}={}", key, value),
            ]);
        }

        if config.create_namespace {
            args.push("--create-namespace".to_string());
        }

        if config.wait {
            args.push("--wait".to_string());
        }

        if let Some(timeout) = &config.timeout {
            args.extend(vec!["--timeout".to_string(), timeout.clone()]);
        }

        if config.atomic {
            args.push("--atomic".to_string());
        }

        if config.skip_crds {
            args.push("--skip-crds".to_string());
        }

        if dry_run {
            args.push("--dry-run".to_string());
        }

        tracing::info!("Executing: helm {}", args.join(" "));

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Helm upgrade failed: {}", output.stderr);
        }

        let version = self.get_release_revision(release_name).await?;

        Ok(DeploymentResult::success(format!(
            "Helm release {} upgraded to revision {}",
            release_name, version
        ))
        .with_version(version))
    }

    pub async fn rollback(
        &self,
        release_name: &str,
        revision: Option<u32>,
        dry_run: bool,
    ) -> anyhow::Result<DeploymentResult> {
        self.check_helm_installed().await?;

        let mut args = vec!["rollback".to_string(), release_name.to_string()];

        if let Some(rev) = revision {
            args.push(rev.to_string());
        }

        args.extend(vec!["--namespace".to_string(), self.namespace.clone()]);

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        args.push("--wait".to_string());

        if dry_run {
            args.push("--dry-run".to_string());
        }

        tracing::info!("Executing: helm {}", args.join(" "));

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Helm rollback failed: {}", output.stderr);
        }

        let current_rev = self.get_release_revision(release_name).await?;

        Ok(DeploymentResult::success(format!(
            "Helm release {} rolled back to revision {}",
            release_name, current_rev
        ))
        .with_version(current_rev))
    }

    pub async fn get_release_status(&self, release_name: &str) -> anyhow::Result<String> {
        self.check_helm_installed().await?;

        let mut args = vec![
            "status".to_string(),
            release_name.to_string(),
            "--namespace".to_string(),
            self.namespace.clone(),
        ];

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Failed to get Helm release status: {}", output.stderr);
        }

        Ok(output.stdout)
    }

    pub async fn get_release_history(&self, release_name: &str) -> anyhow::Result<String> {
        self.check_helm_installed().await?;

        let mut args = vec![
            "history".to_string(),
            release_name.to_string(),
            "--namespace".to_string(),
            self.namespace.clone(),
        ];

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Failed to get Helm release history: {}", output.stderr);
        }

        Ok(output.stdout)
    }

    async fn get_release_revision(&self, release_name: &str) -> anyhow::Result<String> {
        let mut args = vec![
            "list".to_string(),
            "--filter".to_string(),
            format!("^{}$", release_name),
            "--namespace".to_string(),
            self.namespace.clone(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            return Ok("unknown".to_string());
        }

        let releases: Vec<serde_json::Value> = serde_json::from_str(&output.stdout)?;

        if let Some(release) = releases.first() {
            if let Some(revision) = release.get("revision") {
                return Ok(revision.to_string());
            }
        }

        Ok("unknown".to_string())
    }

    pub async fn uninstall(&self, release_name: &str, dry_run: bool) -> anyhow::Result<()> {
        self.check_helm_installed().await?;

        let mut args = vec![
            "uninstall".to_string(),
            release_name.to_string(),
            "--namespace".to_string(),
            self.namespace.clone(),
        ];

        if let Some(ctx) = &self.context {
            args.extend(vec!["--kube-context".to_string(), ctx.clone()]);
        }

        if dry_run {
            args.push("--dry-run".to_string());
        }

        tracing::info!("Executing: helm {}", args.join(" "));

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Helm uninstall failed: {}", output.stderr);
        }

        Ok(())
    }

    pub async fn template(
        &self,
        config: &HelmConfig,
        output_dir: Option<&str>,
    ) -> anyhow::Result<String> {
        self.check_helm_installed().await?;

        let release_name = config
            .release_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("release_name is required"))?;

        let mut args = vec![
            "template".to_string(),
            release_name.clone(),
            config.chart.clone(),
            "--namespace".to_string(),
            self.namespace.clone(),
        ];

        if let Some(version) = &config.version {
            args.extend(vec!["--version".to_string(), version.clone()]);
        }

        for values_file in &config.values_files {
            args.extend(vec!["-f".to_string(), values_file.clone()]);
        }

        for (key, value) in &config.set {
            args.extend(vec!["--set".to_string(), format!("{}={}", key, value)]);
        }

        if let Some(dir) = output_dir {
            args.extend(vec!["--output-dir".to_string(), dir.to_string()]);
        }

        let output = self.executor.execute("helm", &args).await?;

        if !output.success {
            anyhow::bail!("Helm template failed: {}", output.stderr);
        }

        Ok(output.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockExecutor {
        commands: Arc<Mutex<Vec<(String, Vec<String>)>>>,
        responses: Arc<Mutex<Vec<CommandOutput>>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                commands: Arc::new(Mutex::new(Vec::new())),
                responses: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_responses(responses: Vec<CommandOutput>) -> Self {
            Self {
                commands: Arc::new(Mutex::new(Vec::new())),
                responses: Arc::new(Mutex::new(responses)),
            }
        }

        fn get_commands(&self) -> Vec<(String, Vec<String>)> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(
            &self,
            program: &str,
            args: &[String],
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = anyhow::Result<CommandOutput>> + Send + '_>,
        > {
            let program = program.to_string();
            let args = args.to_vec();
            let commands = self.commands.clone();
            let responses = self.responses.clone();

            Box::pin(async move {
                commands.lock().unwrap().push((program, args));

                let response = responses
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap_or_else(|| CommandOutput {
                        success: true,
                        stdout: String::new(),
                        stderr: String::new(),
                    });

                Ok(response)
            })
        }
    }

    #[test]
    fn test_helm_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
chart: ./charts/my-app
release_name: my-app
values_files:
  - values.yaml
set:
  replicas: "3"
wait: true
"#,
        )
        .unwrap();

        let config = HelmConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.chart, "./charts/my-app");
        assert_eq!(config.release_name, Some("my-app".to_string()));
        assert_eq!(config.values_files.len(), 1);
        assert_eq!(config.set.get("replicas"), Some(&"3".to_string()));
        assert!(config.wait);
    }

    #[test]
    fn test_helm_config_merge() {
        let base = HelmConfig {
            chart: "./charts/my-app".to_string(),
            repo: None,
            repo_name: None,
            version: Some("1.0.0".to_string()),
            release_name: Some("my-app".to_string()),
            values_files: vec!["values.yaml".to_string()],
            set: {
                let mut m = HashMap::new();
                m.insert("replicas".to_string(), "2".to_string());
                m
            },
            set_string: HashMap::new(),
            create_namespace: false,
            wait: true,
            timeout: None,
            atomic: false,
            skip_crds: false,
        };

        let overrides = HelmConfig {
            chart: String::new(),
            repo: None,
            repo_name: None,
            version: Some("2.0.0".to_string()),
            release_name: None,
            values_files: vec!["values-prod.yaml".to_string()],
            set: {
                let mut m = HashMap::new();
                m.insert("replicas".to_string(), "5".to_string());
                m.insert("image.tag".to_string(), "v2".to_string());
                m
            },
            set_string: HashMap::new(),
            create_namespace: true,
            wait: false,
            timeout: Some("10m".to_string()),
            atomic: false,
            skip_crds: false,
        };

        let merged = base.merge(&overrides);

        assert_eq!(merged.chart, "./charts/my-app");
        assert_eq!(merged.version, Some("2.0.0".to_string()));
        assert_eq!(merged.release_name, Some("my-app".to_string()));
        assert_eq!(merged.values_files.len(), 2);
        assert_eq!(merged.set.get("replicas"), Some(&"5".to_string()));
        assert_eq!(merged.set.get("image.tag"), Some(&"v2".to_string()));
        assert!(merged.create_namespace);
        assert!(merged.wait);
        assert_eq!(merged.timeout, Some("10m".to_string()));
    }

    #[tokio::test]
    async fn test_helm_check_installed() {
        let executor = MockExecutor::with_responses(vec![CommandOutput {
            success: true,
            stdout: "version.BuildInfo{Version:\"v3.12.0\"}".to_string(),
            stderr: String::new(),
        }]);

        let deployer = HelmDeployer::new("default", None).with_executor(Box::new(executor));

        let result = deployer.check_helm_installed().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_helm_check_installed_missing() {
        let executor = MockExecutor::with_responses(vec![CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: "command not found".to_string(),
        }]);

        let deployer = HelmDeployer::new("default", None).with_executor(Box::new(executor));

        let result = deployer.check_helm_installed().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_helm_install_upgrade_builds_args() {
        let executor = MockExecutor::with_responses(vec![
            CommandOutput {
                success: true,
                stdout: "[]".to_string(),
                stderr: String::new(),
            },
            CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
        ]);

        let mock_ref = executor.commands.clone();

        let deployer = HelmDeployer::new("production", Some("my-context"))
            .with_executor(Box::new(executor));

        let mut set = HashMap::new();
        set.insert("image.tag".to_string(), "v1.0.0".to_string());

        let config = HelmConfig {
            chart: "./charts/app".to_string(),
            repo: None,
            repo_name: None,
            version: Some("1.2.3".to_string()),
            release_name: Some("my-release".to_string()),
            values_files: vec!["values.yaml".to_string()],
            set,
            set_string: HashMap::new(),
            create_namespace: true,
            wait: true,
            timeout: Some("5m".to_string()),
            atomic: true,
            skip_crds: false,
        };

        let _ = deployer.install_or_upgrade(&config, false).await;

        let commands = mock_ref.lock().unwrap();
        assert!(commands.len() >= 2);

        let upgrade_cmd = commands
            .iter()
            .find(|(p, args)| p == "helm" && args.contains(&"upgrade".to_string()))
            .unwrap();
        let args = &upgrade_cmd.1;

        assert!(args.contains(&"upgrade".to_string()));
        assert!(args.contains(&"--install".to_string()));
        assert!(args.contains(&"my-release".to_string()));
        assert!(args.contains(&"--namespace".to_string()));
        assert!(args.contains(&"production".to_string()));
        assert!(args.contains(&"--kube-context".to_string()));
        assert!(args.contains(&"my-context".to_string()));
        assert!(args.contains(&"--create-namespace".to_string()));
        assert!(args.contains(&"--wait".to_string()));
        assert!(args.contains(&"--atomic".to_string()));
    }

    #[tokio::test]
    async fn test_helm_rollback_builds_args() {
        let executor = MockExecutor::with_responses(vec![
            CommandOutput {
                success: true,
                stdout: "[{\"revision\": 5}]".to_string(),
                stderr: String::new(),
            },
            CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
            CommandOutput {
                success: true,
                stdout: String::new(),
                stderr: String::new(),
            },
        ]);

        let mock_ref = executor.commands.clone();

        let deployer =
            HelmDeployer::new("default", None).with_executor(Box::new(executor));

        let _ = deployer.rollback("my-release", Some(3), false).await;

        let commands = mock_ref.lock().unwrap();

        let rollback_cmd = commands
            .iter()
            .find(|(_, args)| args.contains(&"rollback".to_string()))
            .unwrap();

        assert!(rollback_cmd.1.contains(&"my-release".to_string()));
        assert!(rollback_cmd.1.contains(&"3".to_string()));
        assert!(rollback_cmd.1.contains(&"--wait".to_string()));
    }
}
