use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::deployment::DeploymentResult;

use super::helm::{CommandExecutor, CommandOutput, RealCommandExecutor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KustomizeConfig {
    pub path: String,

    #[serde(default)]
    pub images: Vec<KustomizeImage>,

    #[serde(default)]
    pub replicas: Vec<KustomizeReplica>,

    #[serde(default)]
    pub labels: HashMap<String, String>,

    #[serde(default)]
    pub annotations: HashMap<String, String>,

    #[serde(default)]
    pub name_prefix: Option<String>,

    #[serde(default)]
    pub name_suffix: Option<String>,

    #[serde(default)]
    pub namespace: Option<String>,

    #[serde(default)]
    pub enable_helm: bool,

    #[serde(default)]
    pub load_restrictor: Option<String>,

    #[serde(default)]
    pub prune: bool,

    #[serde(default)]
    pub prune_whitelist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KustomizeImage {
    pub name: String,
    #[serde(default)]
    pub new_name: Option<String>,
    #[serde(default)]
    pub new_tag: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KustomizeReplica {
    pub name: String,
    pub count: u32,
}

impl KustomizeConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    pub fn merge(&self, overrides: &KustomizeConfig) -> KustomizeConfig {
        let mut images = self.images.clone();
        for override_img in &overrides.images {
            if let Some(existing) = images.iter_mut().find(|i| i.name == override_img.name) {
                if override_img.new_name.is_some() {
                    existing.new_name = override_img.new_name.clone();
                }

                if override_img.new_tag.is_some() {
                    existing.new_tag = override_img.new_tag.clone();
                }

                if override_img.digest.is_some() {
                    existing.digest = override_img.digest.clone();
                }
            } else {
                images.push(override_img.clone());
            }
        }

        let mut replicas = self.replicas.clone();
        for override_rep in &overrides.replicas {
            if let Some(existing) = replicas.iter_mut().find(|r| r.name == override_rep.name) {
                existing.count = override_rep.count;
            } else {
                replicas.push(override_rep.clone());
            }
        }

        let mut labels = self.labels.clone();
        labels.extend(overrides.labels.clone());

        let mut annotations = self.annotations.clone();
        annotations.extend(overrides.annotations.clone());

        let mut prune_whitelist = self.prune_whitelist.clone();
        prune_whitelist.extend(overrides.prune_whitelist.clone());

        KustomizeConfig {
            path: if overrides.path.is_empty() {
                self.path.clone()
            } else {
                overrides.path.clone()
            },
            images,
            replicas,
            labels,
            annotations,
            name_prefix: overrides
                .name_prefix
                .clone()
                .or_else(|| self.name_prefix.clone()),
            name_suffix: overrides
                .name_suffix
                .clone()
                .or_else(|| self.name_suffix.clone()),
            namespace: overrides
                .namespace
                .clone()
                .or_else(|| self.namespace.clone()),
            enable_helm: overrides.enable_helm || self.enable_helm,
            load_restrictor: overrides
                .load_restrictor
                .clone()
                .or_else(|| self.load_restrictor.clone()),
            prune: overrides.prune || self.prune,
            prune_whitelist,
        }
    }
}

pub struct KustomizeDeployer {
    namespace: String,
    context: Option<String>,
    working_dir: Option<PathBuf>,
    executor: Box<dyn CommandExecutor>,
}

impl KustomizeDeployer {
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

    pub async fn check_kustomize_installed(&self) -> anyhow::Result<()> {
        let output = self
            .executor
            .execute("kubectl", &["kustomize".to_string(), "--help".to_string()])
            .await?;

        if !output.success {
            let standalone = self
                .executor
                .execute("kustomize", &["version".to_string()])
                .await?;

            if !standalone.success {
                anyhow::bail!(
                    "Kustomize is not installed. It's built into kubectl (v1.14+) or install standalone: \
                    https://kubectl.docs.kubernetes.io/installation/kustomize/"
                );
            }
        }

        Ok(())
    }

    async fn build(&self, config: &KustomizeConfig) -> anyhow::Result<String> {
        let mut args = vec!["kustomize".to_string()];

        args.push(config.path.clone());

        if config.enable_helm {
            args.push("--enable-helm".to_string());
        }

        if let Some(restrictor) = &config.load_restrictor {
            args.extend(vec!["--load-restrictor".to_string(), restrictor.clone()]);
        }

        tracing::info!("Executing: kubectl {}", args.join(" "));

        let output = self.executor.execute("kubectl", &args).await?;

        if !output.success {
            anyhow::bail!("Kustomize build failed: {}", output.stderr);
        }

        Ok(output.stdout)
    }

    fn build_kubectl_args(&self, config: &KustomizeConfig) -> Vec<String> {
        let mut args = Vec::new();

        for img in &config.images {
            let mut img_spec = img.name.clone();

            if let Some(new_name) = &img.new_name {
                img_spec = format!("{}={}", img_spec, new_name);

                if let Some(tag) = &img.new_tag {
                    img_spec = format!("{}:{}", img_spec, tag);
                }
            } else if let Some(tag) = &img.new_tag {
                img_spec = format!("{}:{}", img_spec, tag);
            }

            if let Some(digest) = &img.digest {
                img_spec = format!("{}@{}", img_spec, digest);
            }

            args.extend(vec!["--set-image".to_string(), img_spec]);
        }

        args
    }

    pub async fn apply(
        &self,
        config: &KustomizeConfig,
        dry_run: bool,
    ) -> anyhow::Result<DeploymentResult> {
        self.check_kustomize_installed().await?;

        let manifest = self.build(config).await?;

        let mut args = vec!["apply".to_string(), "-f".to_string(), "-".to_string()];

        args.extend(vec![
            "--namespace".to_string(),
            config.namespace.clone().unwrap_or_else(|| self.namespace.clone()),
        ]);

        if let Some(ctx) = &self.context {
            args.extend(vec!["--context".to_string(), ctx.clone()]);
        }

        args.extend(self.build_kubectl_args(config));

        if config.prune {
            args.push("--prune".to_string());

            for whitelist in &config.prune_whitelist {
                args.extend(vec!["--prune-whitelist".to_string(), whitelist.clone()]);
            }
        }

        if dry_run {
            args.extend(vec![
                "--dry-run".to_string(),
                "client".to_string(),
                "-o".to_string(),
                "yaml".to_string(),
            ]);
        }

        tracing::info!("Executing: kubectl {}", args.join(" "));

        let output = self.apply_manifest(&manifest, &args).await?;

        if !output.success {
            anyhow::bail!("Kubectl apply failed: {}", output.stderr);
        }

        let resources = self.count_resources(&output.stdout);

        Ok(DeploymentResult::success(format!(
            "Applied kustomization from {} ({} resources)",
            config.path, resources
        )))
    }

    async fn apply_manifest(&self, manifest: &str, args: &[String]) -> anyhow::Result<CommandOutput> {
        let temp_file = std::env::temp_dir().join(format!(
            "pmp-deploy-kustomize-{}.yaml",
            std::process::id()
        ));

        tokio::fs::write(&temp_file, manifest).await?;

        let mut modified_args = args.to_vec();

        if let Some(pos) = modified_args.iter().position(|a| a == "-") {
            modified_args[pos] = temp_file.to_string_lossy().to_string();
        }

        let result = self.executor.execute("kubectl", &modified_args).await;

        let _ = tokio::fs::remove_file(&temp_file).await;

        result
    }

    fn count_resources(&self, output: &str) -> usize {
        output
            .lines()
            .filter(|line| {
                line.contains("created")
                    || line.contains("configured")
                    || line.contains("unchanged")
            })
            .count()
    }

    pub async fn delete(
        &self,
        config: &KustomizeConfig,
        dry_run: bool,
    ) -> anyhow::Result<DeploymentResult> {
        self.check_kustomize_installed().await?;

        let manifest = self.build(config).await?;

        let mut args = vec!["delete".to_string(), "-f".to_string(), "-".to_string()];

        args.extend(vec![
            "--namespace".to_string(),
            config.namespace.clone().unwrap_or_else(|| self.namespace.clone()),
        ]);

        if let Some(ctx) = &self.context {
            args.extend(vec!["--context".to_string(), ctx.clone()]);
        }

        if dry_run {
            args.push("--dry-run".to_string());
        }

        tracing::info!("Executing: kubectl {}", args.join(" "));

        let output = self.apply_manifest(&manifest, &args).await?;

        if !output.success {
            anyhow::bail!("Kubectl delete failed: {}", output.stderr);
        }

        Ok(DeploymentResult::success(format!(
            "Deleted resources from kustomization {}",
            config.path
        )))
    }

    pub async fn diff(&self, config: &KustomizeConfig) -> anyhow::Result<String> {
        self.check_kustomize_installed().await?;

        let manifest = self.build(config).await?;

        let mut args = vec!["diff".to_string(), "-f".to_string(), "-".to_string()];

        args.extend(vec![
            "--namespace".to_string(),
            config.namespace.clone().unwrap_or_else(|| self.namespace.clone()),
        ]);

        if let Some(ctx) = &self.context {
            args.extend(vec!["--context".to_string(), ctx.clone()]);
        }

        let output = self.apply_manifest(&manifest, &args).await?;

        Ok(output.stdout)
    }

    pub async fn render(&self, config: &KustomizeConfig) -> anyhow::Result<String> {
        self.build(config).await
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
        fn with_responses(responses: Vec<CommandOutput>) -> Self {
            Self {
                commands: Arc::new(Mutex::new(Vec::new())),
                responses: Arc::new(Mutex::new(responses)),
            }
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
    fn test_kustomize_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
path: ./k8s/overlays/production
images:
  - name: my-app
    new_tag: v1.0.0
replicas:
  - name: my-deployment
    count: 5
labels:
  environment: production
"#,
        )
        .unwrap();

        let config = KustomizeConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.path, "./k8s/overlays/production");
        assert_eq!(config.images.len(), 1);
        assert_eq!(config.images[0].name, "my-app");
        assert_eq!(config.images[0].new_tag, Some("v1.0.0".to_string()));
        assert_eq!(config.replicas.len(), 1);
        assert_eq!(config.replicas[0].count, 5);
        assert_eq!(
            config.labels.get("environment"),
            Some(&"production".to_string())
        );
    }

    #[test]
    fn test_kustomize_config_merge() {
        let base = KustomizeConfig {
            path: "./k8s/base".to_string(),
            images: vec![KustomizeImage {
                name: "my-app".to_string(),
                new_name: None,
                new_tag: Some("v1.0.0".to_string()),
                digest: None,
            }],
            replicas: vec![KustomizeReplica {
                name: "my-deployment".to_string(),
                count: 3,
            }],
            labels: {
                let mut m = HashMap::new();
                m.insert("app".to_string(), "my-app".to_string());
                m
            },
            annotations: HashMap::new(),
            name_prefix: None,
            name_suffix: None,
            namespace: Some("default".to_string()),
            enable_helm: false,
            load_restrictor: None,
            prune: false,
            prune_whitelist: Vec::new(),
        };

        let overrides = KustomizeConfig {
            path: "./k8s/overlays/prod".to_string(),
            images: vec![KustomizeImage {
                name: "my-app".to_string(),
                new_name: None,
                new_tag: Some("v2.0.0".to_string()),
                digest: None,
            }],
            replicas: vec![KustomizeReplica {
                name: "my-deployment".to_string(),
                count: 10,
            }],
            labels: {
                let mut m = HashMap::new();
                m.insert("environment".to_string(), "production".to_string());
                m
            },
            annotations: HashMap::new(),
            name_prefix: Some("prod-".to_string()),
            name_suffix: None,
            namespace: Some("production".to_string()),
            enable_helm: false,
            load_restrictor: None,
            prune: true,
            prune_whitelist: Vec::new(),
        };

        let merged = base.merge(&overrides);

        assert_eq!(merged.path, "./k8s/overlays/prod");
        assert_eq!(merged.images[0].new_tag, Some("v2.0.0".to_string()));
        assert_eq!(merged.replicas[0].count, 10);
        assert_eq!(merged.labels.len(), 2);
        assert_eq!(merged.name_prefix, Some("prod-".to_string()));
        assert_eq!(merged.namespace, Some("production".to_string()));
        assert!(merged.prune);
    }

    #[tokio::test]
    async fn test_kustomize_check_installed() {
        let executor = MockExecutor::with_responses(vec![CommandOutput {
            success: true,
            stdout: "kubectl kustomize help...".to_string(),
            stderr: String::new(),
        }]);

        let deployer =
            KustomizeDeployer::new("default", None).with_executor(Box::new(executor));

        let result = deployer.check_kustomize_installed().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_kubectl_image_args() {
        let deployer = KustomizeDeployer::new("default", None);

        let config = KustomizeConfig {
            path: "./k8s".to_string(),
            images: vec![
                KustomizeImage {
                    name: "my-app".to_string(),
                    new_name: Some("my-registry/my-app".to_string()),
                    new_tag: Some("v1.0.0".to_string()),
                    digest: None,
                },
                KustomizeImage {
                    name: "sidecar".to_string(),
                    new_name: None,
                    new_tag: Some("latest".to_string()),
                    digest: None,
                },
            ],
            replicas: Vec::new(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            name_prefix: None,
            name_suffix: None,
            namespace: None,
            enable_helm: false,
            load_restrictor: None,
            prune: false,
            prune_whitelist: Vec::new(),
        };

        let args = deployer.build_kubectl_args(&config);

        assert!(args.contains(&"--set-image".to_string()));
        assert!(args.iter().any(|a| a.contains("my-registry/my-app:v1.0.0")));
        assert!(args.iter().any(|a| a.contains("sidecar:latest")));
    }

    #[test]
    fn test_count_resources() {
        let deployer = KustomizeDeployer::new("default", None);

        let output = r#"
deployment.apps/my-app created
service/my-app configured
configmap/my-config unchanged
"#;

        assert_eq!(deployer.count_resources(output), 3);
    }
}
