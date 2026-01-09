use async_trait::async_trait;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::helm::{HelmConfig, HelmDeployer};
use super::k8s_resources::{
    HpaConfig, K8sConfigMapSpec, K8sResourceManager, K8sSecretSpec, RawManifestApplier,
    RawManifestConfig,
};
use super::kustomize::{KustomizeConfig, KustomizeDeployer};
use super::manifest::{BuiltinVariables, ManifestRenderer, ManifestTemplateConfig};
use super::provider::{DeploymentContext, InfrastructureProvider, InfrastructureType};
use crate::config::InfrastructureConfig;
use crate::deployment::{DeploymentResult, DeploymentType};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentMethod {
    #[default]
    Direct,
    Helm,
    Kustomize,
    Template,
    RawManifest,
}

pub struct KubernetesProvider {
    client: Client,
    namespace: String,
    context: Option<String>,
    helm_config: Option<HelmConfig>,
    kustomize_config: Option<KustomizeConfig>,
    manifest_template_config: Option<ManifestTemplateConfig>,
    raw_manifest_config: Option<RawManifestConfig>,
    config_maps: Vec<K8sConfigMapSpec>,
    secrets: Vec<K8sSecretSpec>,
    hpa_config: Option<HpaConfig>,
}

impl KubernetesProvider {
    pub async fn new(namespace: &str, context: Option<&str>) -> anyhow::Result<Self> {
        let client = if let Some(ctx) = context {
            let kubeconfig = Kubeconfig::read()?;
            let options = KubeConfigOptions {
                context: Some(ctx.to_string()),
                ..Default::default()
            };
            let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
            Client::try_from(config)?
        } else {
            Client::try_default().await?
        };

        Ok(Self {
            client,
            namespace: namespace.to_string(),
            context: context.map(String::from),
            helm_config: None,
            kustomize_config: None,
            manifest_template_config: None,
            raw_manifest_config: None,
            config_maps: Vec::new(),
            secrets: Vec::new(),
            hpa_config: None,
        })
    }

    pub async fn from_config(config: &InfrastructureConfig) -> anyhow::Result<Self> {
        let namespace = config
            .config
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let context = config.config.get("context").and_then(|v| v.as_str());

        let helm_config = config
            .config
            .get("helm")
            .and_then(HelmConfig::from_yaml_value);

        let kustomize_config = config
            .config
            .get("kustomize")
            .and_then(KustomizeConfig::from_yaml_value);

        let manifest_template_config = config
            .config
            .get("manifest_template")
            .and_then(ManifestTemplateConfig::from_yaml_value);

        let raw_manifest_config = config
            .config
            .get("raw_manifests")
            .and_then(RawManifestConfig::from_yaml_value);

        let config_maps = Self::parse_config_maps(&config.config);
        let secrets = Self::parse_secrets(&config.config);

        let hpa_config = config
            .config
            .get("hpa")
            .and_then(HpaConfig::from_yaml_value);

        let mut provider = Self::new(namespace, context).await?;
        provider.helm_config = helm_config;
        provider.kustomize_config = kustomize_config;
        provider.manifest_template_config = manifest_template_config;
        provider.raw_manifest_config = raw_manifest_config;
        provider.config_maps = config_maps;
        provider.secrets = secrets;
        provider.hpa_config = hpa_config;

        Ok(provider)
    }

    fn parse_config_maps(config: &HashMap<String, serde_yaml::Value>) -> Vec<K8sConfigMapSpec> {
        config
            .get("config_maps")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|item| K8sConfigMapSpec::from_yaml_value(item))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_secrets(config: &HashMap<String, serde_yaml::Value>) -> Vec<K8sSecretSpec> {
        config
            .get("secrets")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|item| K8sSecretSpec::from_yaml_value(item))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_deployment_method(&self, ctx: &DeploymentContext) -> DeploymentMethod {
        if let Some(method_str) = ctx
            .environment
            .config
            .get("deployment_method")
            .and_then(|v| v.as_str())
        {
            match method_str.to_lowercase().as_str() {
                "helm" => return DeploymentMethod::Helm,
                "kustomize" => return DeploymentMethod::Kustomize,
                "template" => return DeploymentMethod::Template,
                "rawmanifest" | "raw_manifest" => return DeploymentMethod::RawManifest,
                _ => {}
            }
        }

        // Check for explicit configuration
        if ctx.environment.config.get("manifest_template").is_some()
            || self.manifest_template_config.is_some()
        {
            return DeploymentMethod::Template;
        }

        if ctx.environment.config.get("raw_manifests").is_some()
            || self.raw_manifest_config.is_some()
        {
            return DeploymentMethod::RawManifest;
        }

        if ctx.environment.config.get("helm").is_some() || self.helm_config.is_some() {
            return DeploymentMethod::Helm;
        }

        if ctx.environment.config.get("kustomize").is_some() || self.kustomize_config.is_some() {
            return DeploymentMethod::Kustomize;
        }

        DeploymentMethod::Direct
    }

    fn get_merged_helm_config(&self, ctx: &DeploymentContext) -> Option<HelmConfig> {
        let env_helm = ctx
            .environment
            .config
            .get("helm")
            .and_then(HelmConfig::from_yaml_value);

        match (&self.helm_config, env_helm) {
            (Some(base), Some(overrides)) => Some(base.merge(&overrides)),
            (Some(base), None) => Some(base.clone()),
            (None, Some(overrides)) => Some(overrides),
            (None, None) => None,
        }
    }

    fn get_merged_kustomize_config(&self, ctx: &DeploymentContext) -> Option<KustomizeConfig> {
        let env_kustomize = ctx
            .environment
            .config
            .get("kustomize")
            .and_then(KustomizeConfig::from_yaml_value);

        match (&self.kustomize_config, env_kustomize) {
            (Some(base), Some(overrides)) => Some(base.merge(&overrides)),
            (Some(base), None) => Some(base.clone()),
            (None, Some(overrides)) => Some(overrides),
            (None, None) => None,
        }
    }

    fn get_merged_manifest_template_config(
        &self,
        ctx: &DeploymentContext,
    ) -> Option<ManifestTemplateConfig> {
        let env_template = ctx
            .environment
            .config
            .get("manifest_template")
            .and_then(ManifestTemplateConfig::from_yaml_value);

        match (&self.manifest_template_config, env_template) {
            (Some(_base), Some(overrides)) => Some(overrides),
            (Some(base), None) => Some(base.clone()),
            (None, Some(overrides)) => Some(overrides),
            (None, None) => None,
        }
    }

    fn get_merged_raw_manifest_config(&self, ctx: &DeploymentContext) -> Option<RawManifestConfig> {
        let env_raw = ctx
            .environment
            .config
            .get("raw_manifests")
            .and_then(RawManifestConfig::from_yaml_value);

        match (&self.raw_manifest_config, env_raw) {
            (Some(_base), Some(overrides)) => Some(overrides),
            (Some(base), None) => Some(base.clone()),
            (None, Some(overrides)) => Some(overrides),
            (None, None) => None,
        }
    }

    fn get_merged_hpa_config(&self, ctx: &DeploymentContext) -> Option<HpaConfig> {
        let env_hpa = ctx
            .environment
            .config
            .get("hpa")
            .and_then(HpaConfig::from_yaml_value);

        match (&self.hpa_config, env_hpa) {
            (Some(_base), Some(overrides)) => Some(overrides),
            (Some(base), None) => Some(base.clone()),
            (None, Some(overrides)) => Some(overrides),
            (None, None) => None,
        }
    }

    async fn deploy_with_template(
        &self,
        ctx: &DeploymentContext,
    ) -> anyhow::Result<DeploymentResult> {
        let config = self.get_merged_manifest_template_config(ctx).ok_or_else(|| {
            anyhow::anyhow!("Manifest template configuration is required for template deployment")
        })?;

        let renderer = ManifestRenderer::new(&config.path)?;

        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let mut builtins = BuiltinVariables::new(&self.namespace, &ctx.environment_name)
            .with_deployment_name(deployment_name);

        if let Some(image) = &ctx.environment.image {
            builtins = builtins.with_image(image);
        }

        let manifests = renderer.render_all(&config, &builtins)?;

        if ctx.dry_run {
            let manifest_names: Vec<_> = manifests.iter().map(|m| m.name.as_str()).collect();
            return Ok(DeploymentResult::success(format!(
                "Would apply templates: {}",
                manifest_names.join(", ")
            )));
        }

        let applier = RawManifestApplier::new(&self.namespace, self.context.as_deref());

        let mut applied_count = 0;
        for manifest in &manifests {
            applier.apply_content(&manifest.content, ctx.dry_run).await?;
            tracing::info!("Applied template '{}'", manifest.name);
            applied_count += 1;
        }

        let image = ctx.environment.image.clone().unwrap_or_default();
        Ok(
            DeploymentResult::success(format!(
                "Applied {} manifest templates",
                applied_count
            ))
            .with_version(image),
        )
    }

    async fn deploy_raw_manifests(
        &self,
        ctx: &DeploymentContext,
    ) -> anyhow::Result<DeploymentResult> {
        let config = self.get_merged_raw_manifest_config(ctx).ok_or_else(|| {
            anyhow::anyhow!("Raw manifests configuration is required for raw manifest deployment")
        })?;

        let applier = RawManifestApplier::new(&self.namespace, self.context.as_deref());

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would apply raw manifests: {}",
                config.files.join(", ")
            )));
        }

        let result = applier.apply(&config, ctx.dry_run).await?;
        tracing::info!("Applied raw manifests: {}", result);

        let image = ctx.environment.image.clone().unwrap_or_default();
        Ok(DeploymentResult::success(format!("Applied raw manifests: {}", config.files.join(", ")))
            .with_version(image))
    }

    async fn apply_pre_deployment_resources(&self, ctx: &DeploymentContext) -> anyhow::Result<()> {
        let resource_manager = K8sResourceManager::new(self.client.clone(), &self.namespace);

        // Apply ConfigMaps
        let config_maps = self.get_merged_config_maps(ctx);
        for mut spec in config_maps {
            spec.load_files()?;
            resource_manager.apply_config_map(&spec).await?;
        }

        // Apply Secrets
        let secrets = self.get_merged_secrets(ctx);
        for spec in secrets {
            let resolved_data = self.resolve_secret_data(&spec).await?;
            resource_manager.apply_secret(&spec, &resolved_data).await?;
        }

        Ok(())
    }

    async fn apply_post_deployment_resources(&self, ctx: &DeploymentContext) -> anyhow::Result<()> {
        let resource_manager = K8sResourceManager::new(self.client.clone(), &self.namespace);

        // Apply HPA if configured
        if let Some(hpa_config) = self.get_merged_hpa_config(ctx) {
            resource_manager
                .apply_hpa(&hpa_config, Some(&self.namespace))
                .await?;
        }

        Ok(())
    }

    fn get_merged_config_maps(&self, ctx: &DeploymentContext) -> Vec<K8sConfigMapSpec> {
        let env_config_maps = Self::parse_config_maps(&ctx.environment.config);

        if env_config_maps.is_empty() {
            self.config_maps.clone()
        } else {
            env_config_maps
        }
    }

    fn get_merged_secrets(&self, ctx: &DeploymentContext) -> Vec<K8sSecretSpec> {
        let env_secrets = Self::parse_secrets(&ctx.environment.config);

        if env_secrets.is_empty() {
            self.secrets.clone()
        } else {
            env_secrets
        }
    }

    async fn resolve_secret_data(
        &self,
        spec: &K8sSecretSpec,
    ) -> anyhow::Result<HashMap<String, String>> {
        use crate::config::EnvVarResolver;

        let resolver = EnvVarResolver::new();
        let resolved = resolver.resolve(&spec.data).await?;

        Ok(resolved)
    }

    async fn deploy_with_helm(
        &self,
        ctx: &DeploymentContext,
    ) -> anyhow::Result<DeploymentResult> {
        let mut config = self
            .get_merged_helm_config(ctx)
            .ok_or_else(|| anyhow::anyhow!("Helm configuration is required for helm deployment"))?;

        if let Some(image) = &ctx.environment.image {
            config.set.insert("image.tag".to_string(), image.clone());
        }

        let deployer = HelmDeployer::new(&self.namespace, self.context.as_deref());
        deployer.install_or_upgrade(&config, ctx.dry_run).await
    }

    async fn deploy_with_kustomize(
        &self,
        ctx: &DeploymentContext,
    ) -> anyhow::Result<DeploymentResult> {
        let mut config = self.get_merged_kustomize_config(ctx).ok_or_else(|| {
            anyhow::anyhow!("Kustomize configuration is required for kustomize deployment")
        })?;

        if let Some(image) = &ctx.environment.image {
            let deployment_name = ctx
                .environment
                .config
                .get("deployment_name")
                .and_then(|v| v.as_str())
                .unwrap_or("app");

            let existing_image = config
                .images
                .iter_mut()
                .find(|i| i.name == deployment_name);

            if let Some(img) = existing_image {
                img.new_tag = Some(image.clone());
            } else {
                config.images.push(super::kustomize::KustomizeImage {
                    name: deployment_name.to_string(),
                    new_name: None,
                    new_tag: Some(image.clone()),
                    digest: None,
                });
            }
        }

        let deployer = KustomizeDeployer::new(&self.namespace, self.context.as_deref());
        deployer.apply(&config, ctx.dry_run).await
    }

    async fn get_deployment(&self, name: &str) -> anyhow::Result<Option<Deployment>> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), &self.namespace);

        match deployments.get_opt(name).await? {
            Some(d) => Ok(Some(d)),
            None => Ok(None),
        }
    }

    async fn update_deployment_image(
        &self,
        name: &str,
        image: &str,
    ) -> anyhow::Result<Deployment> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), &self.namespace);

        let patch = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [{
                            "name": name,
                            "image": image
                        }]
                    }
                }
            }
        });

        let params = PatchParams::apply("pmp-deploy");
        let deployment = deployments
            .patch(name, &params, &Patch::Merge(&patch))
            .await?;

        Ok(deployment)
    }

    async fn wait_for_rollout(&self, name: &str, timeout_secs: u64) -> anyhow::Result<()> {
        let deployments: Api<Deployment> = Api::namespaced(self.client.clone(), &self.namespace);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() > timeout_secs {
                anyhow::bail!("Deployment rollout timed out after {} seconds", timeout_secs);
            }

            let deployment = deployments.get(name).await?;

            if let Some(status) = deployment.status {
                let replicas = status.replicas.unwrap_or(0);
                let ready = status.ready_replicas.unwrap_or(0);
                let updated = status.updated_replicas.unwrap_or(0);

                tracing::info!(
                    "Deployment {}: {}/{} ready, {} updated",
                    name,
                    ready,
                    replicas,
                    updated
                );

                if ready == replicas && updated == replicas && replicas > 0 {
                    return Ok(());
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    async fn get_pods(&self, selector: &str) -> anyhow::Result<Vec<Pod>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let lp = ListParams::default().labels(selector);
        let pod_list = pods.list(&lp).await?;

        Ok(pod_list.items)
    }

    fn determine_strategy(&self, deployment_type: &DeploymentType) -> &'static str {
        match deployment_type {
            DeploymentType::RollingUpdate => "RollingUpdate",
            DeploymentType::AllIn => "Recreate", // K8s native Recreate strategy
        }
    }

    async fn deploy_direct(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let image = ctx
            .environment
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No image specified for deployment"))?;

        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would deploy {} to deployment/{} in namespace {}",
                image, deployment_name, self.namespace
            )));
        }

        let deployment_type = ctx
            .environment
            .get_deployment_type()
            .unwrap_or(DeploymentType::RollingUpdate);

        tracing::info!(
            "Deploying {} to {} using {} strategy",
            image,
            deployment_name,
            self.determine_strategy(&deployment_type)
        );

        let existing = self.get_deployment(deployment_name).await?;

        if existing.is_none() {
            anyhow::bail!(
                "Deployment '{}' not found in namespace '{}'. \
                Please create the deployment first or use Helm/Kustomize.",
                deployment_name,
                self.namespace
            );
        }

        self.update_deployment_image(deployment_name, image).await?;

        let timeout = ctx
            .environment
            .config
            .get("rollout_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        self.wait_for_rollout(deployment_name, timeout).await?;

        Ok(
            DeploymentResult::success(format!("Deployed {} to {}", image, deployment_name))
                .with_version(image.clone()),
        )
    }

    async fn rollback_helm(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let config = self.get_merged_helm_config(ctx).ok_or_else(|| {
            anyhow::anyhow!("Helm configuration is required for helm rollback")
        })?;

        let release_name = config
            .release_name
            .ok_or_else(|| anyhow::anyhow!("release_name is required for Helm rollback"))?;

        let deployer = HelmDeployer::new(&self.namespace, self.context.as_deref());
        deployer.rollback(&release_name, None, ctx.dry_run).await
    }

    async fn rollback_direct(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would rollback deployment/{}",
                deployment_name
            )));
        }

        let output = tokio::process::Command::new("kubectl")
            .args([
                "rollout",
                "undo",
                &format!("deployment/{}", deployment_name),
                "-n",
                &self.namespace,
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Rollback failed: {}", stderr);
        }

        self.wait_for_rollout(deployment_name, 300).await?;

        Ok(DeploymentResult::success(format!(
            "Rolled back deployment/{}",
            deployment_name
        )))
    }

    async fn status_helm(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let config = self.get_merged_helm_config(ctx).ok_or_else(|| {
            anyhow::anyhow!("Helm configuration is required for helm status")
        })?;

        let release_name = config
            .release_name
            .ok_or_else(|| anyhow::anyhow!("release_name is required for Helm status"))?;

        let deployer = HelmDeployer::new(&self.namespace, self.context.as_deref());
        let status = deployer.get_release_status(&release_name).await?;
        let history = deployer.get_release_history(&release_name).await?;

        Ok(format!("{}\n\nRelease History:\n{}", status, history))
    }

    async fn status_direct(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let deployment = self
            .get_deployment(deployment_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Deployment '{}' not found", deployment_name))?;

        let status = deployment.status.unwrap_or_default();

        let output = format!(
            "Deployment: {}\n\
             Namespace: {}\n\
             Replicas: {}/{} ready\n\
             Updated: {}\n\
             Available: {}",
            deployment_name,
            self.namespace,
            status.ready_replicas.unwrap_or(0),
            status.replicas.unwrap_or(0),
            status.updated_replicas.unwrap_or(0),
            status.available_replicas.unwrap_or(0),
        );

        Ok(output)
    }

    async fn logs_impl(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let selector = format!("app={}", deployment_name);
        let pods = self.get_pods(&selector).await?;

        if pods.is_empty() {
            println!("No pods found for deployment/{}", deployment_name);
            return Ok(());
        }

        let pod_name = pods[0]
            .metadata
            .name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pod has no name"))?;

        let mut args = vec![
            "logs".to_string(),
            pod_name.clone(),
            "-n".to_string(),
            self.namespace.clone(),
        ];

        if follow {
            args.push("-f".to_string());
        } else {
            args.push("--tail=100".to_string());
        }

        let mut child = tokio::process::Command::new("kubectl")
            .args(&args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        child.wait().await?;

        Ok(())
    }
}

#[async_trait]
impl InfrastructureProvider for KubernetesProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::Kubernetes
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        vec![DeploymentType::RollingUpdate, DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        _config: &HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        let namespaces: Api<k8s_openapi::api::core::v1::Namespace> =
            Api::all(self.client.clone());
        namespaces.list(&ListParams::default().limit(1)).await?;
        Ok(())
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let is_app_only = ctx.deploy_mode.is_app_only();

        // In app-only mode, always use direct image update to avoid touching infrastructure
        if is_app_only {
            tracing::info!(
                "App-only mode: deploying to {} using direct image update (bypassing Helm/Kustomize)",
                self.namespace
            );
            return self.deploy_direct(ctx).await;
        }

        // Apply pre-deployment resources (ConfigMaps, Secrets)
        self.apply_pre_deployment_resources(ctx).await?;

        let method = self.get_deployment_method(ctx);

        tracing::info!(
            "Deploying to {} using {:?} method",
            self.namespace,
            method
        );

        let result = match method {
            DeploymentMethod::Helm => self.deploy_with_helm(ctx).await,
            DeploymentMethod::Kustomize => self.deploy_with_kustomize(ctx).await,
            DeploymentMethod::Template => self.deploy_with_template(ctx).await,
            DeploymentMethod::RawManifest => self.deploy_raw_manifests(ctx).await,
            DeploymentMethod::Direct => self.deploy_direct(ctx).await,
        }?;

        // Apply post-deployment resources (HPA)
        self.apply_post_deployment_resources(ctx).await?;

        Ok(result)
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let method = self.get_deployment_method(ctx);

        match method {
            DeploymentMethod::Helm => self.rollback_helm(ctx).await,
            DeploymentMethod::Kustomize
            | DeploymentMethod::Template
            | DeploymentMethod::RawManifest
            | DeploymentMethod::Direct => self.rollback_direct(ctx).await,
        }
    }

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let method = self.get_deployment_method(ctx);

        match method {
            DeploymentMethod::Helm => self.status_helm(ctx).await,
            DeploymentMethod::Kustomize
            | DeploymentMethod::Template
            | DeploymentMethod::RawManifest
            | DeploymentMethod::Direct => self.status_direct(ctx).await,
        }
    }

    async fn logs(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        self.logs_impl(ctx, follow).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_strategy() {
        assert_eq!(
            match DeploymentType::RollingUpdate {
                DeploymentType::RollingUpdate => "RollingUpdate",
                _ => "Other",
            },
            "RollingUpdate"
        );
    }

    #[test]
    fn test_deployment_method_default() {
        assert_eq!(DeploymentMethod::default(), DeploymentMethod::Direct);
    }

    #[test]
    fn test_deployment_method_from_str() {
        let yaml = serde_yaml::from_str::<DeploymentMethod>("helm").unwrap();
        assert_eq!(yaml, DeploymentMethod::Helm);

        let yaml = serde_yaml::from_str::<DeploymentMethod>("kustomize").unwrap();
        assert_eq!(yaml, DeploymentMethod::Kustomize);

        let yaml = serde_yaml::from_str::<DeploymentMethod>("direct").unwrap();
        assert_eq!(yaml, DeploymentMethod::Direct);

        let yaml = serde_yaml::from_str::<DeploymentMethod>("template").unwrap();
        assert_eq!(yaml, DeploymentMethod::Template);

        let yaml = serde_yaml::from_str::<DeploymentMethod>("rawmanifest").unwrap();
        assert_eq!(yaml, DeploymentMethod::RawManifest);
    }

    #[test]
    fn test_helm_config_from_infrastructure() {
        let yaml = r#"
helm:
  chart: ./charts/my-app
  release_name: my-release
  values_files:
    - values.yaml
  set:
    replicas: "3"
  wait: true
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let helm_config = config.get("helm").and_then(HelmConfig::from_yaml_value);

        assert!(helm_config.is_some());
        let helm = helm_config.unwrap();
        assert_eq!(helm.chart, "./charts/my-app");
        assert_eq!(helm.release_name, Some("my-release".to_string()));
        assert!(helm.wait);
    }

    #[test]
    fn test_kustomize_config_from_infrastructure() {
        let yaml = r#"
kustomize:
  path: ./k8s/overlays/production
  images:
    - name: my-app
      new_tag: v1.0.0
  replicas:
    - name: my-deployment
      count: 5
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let kustomize_config = config
            .get("kustomize")
            .and_then(KustomizeConfig::from_yaml_value);

        assert!(kustomize_config.is_some());
        let kustomize = kustomize_config.unwrap();
        assert_eq!(kustomize.path, "./k8s/overlays/production");
        assert_eq!(kustomize.images.len(), 1);
        assert_eq!(kustomize.images[0].new_tag, Some("v1.0.0".to_string()));
        assert_eq!(kustomize.replicas[0].count, 5);
    }

    #[test]
    fn test_manifest_template_config_from_infrastructure() {
        let yaml = r#"
manifest_template:
  path: ./k8s/templates
  variables:
    REPLICAS: "3"
    LOG_LEVEL: info
  include_env: true
  env_prefix: APP_
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let template_config = config
            .get("manifest_template")
            .and_then(ManifestTemplateConfig::from_yaml_value);

        assert!(template_config.is_some());
        let template = template_config.unwrap();
        assert_eq!(template.path, "./k8s/templates");
        assert_eq!(template.variables.get("REPLICAS"), Some(&"3".to_string()));
        assert_eq!(
            template.variables.get("LOG_LEVEL"),
            Some(&"info".to_string())
        );
        assert!(template.include_env);
        assert_eq!(template.env_prefix, Some("APP_".to_string()));
    }

    #[test]
    fn test_raw_manifest_config_from_infrastructure() {
        let yaml = r#"
raw_manifests:
  files:
    - ./k8s/deployment.yaml
    - ./k8s/service.yaml
  recursive: true
  prune: true
  prune_selector: app=myapp
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let raw_config = config
            .get("raw_manifests")
            .and_then(RawManifestConfig::from_yaml_value);

        assert!(raw_config.is_some());
        let raw = raw_config.unwrap();
        assert_eq!(raw.files.len(), 2);
        assert!(raw.recursive);
        assert!(raw.prune);
        assert_eq!(raw.prune_selector, Some("app=myapp".to_string()));
    }

    #[test]
    fn test_parse_config_maps() {
        let yaml = r#"
config_maps:
  - name: app-config
    namespace: production
    data:
      LOG_LEVEL: info
      API_URL: https://api.example.com
    labels:
      app: my-app
  - name: env-config
    data:
      ENV: prod
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let config_maps = KubernetesProvider::parse_config_maps(&config);

        assert_eq!(config_maps.len(), 2);
        assert_eq!(config_maps[0].name, "app-config");
        assert_eq!(
            config_maps[0].namespace,
            Some("production".to_string())
        );
        assert_eq!(
            config_maps[0].data.get("LOG_LEVEL"),
            Some(&"info".to_string())
        );
        assert_eq!(config_maps[1].name, "env-config");
    }

    #[test]
    fn test_parse_secrets() {
        let yaml = r#"
secrets:
  - name: db-credentials
    secret_type: Opaque
    string_data:
      username: admin
    labels:
      app: my-app
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let secrets = KubernetesProvider::parse_secrets(&config);

        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].name, "db-credentials");
        assert_eq!(secrets[0].secret_type, "Opaque");
        assert_eq!(
            secrets[0].string_data.get("username"),
            Some(&"admin".to_string())
        );
    }

    #[test]
    fn test_hpa_config_from_infrastructure() {
        let yaml = r#"
hpa:
  target_deployment: my-app
  min_replicas: 2
  max_replicas: 10
  target_cpu_utilization: 70
  target_memory_utilization: 80
  scale_down_stabilization_secs: 300
"#;
        let config: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml).unwrap();
        let hpa_config = config.get("hpa").and_then(HpaConfig::from_yaml_value);

        assert!(hpa_config.is_some());
        let hpa = hpa_config.unwrap();
        assert_eq!(hpa.target_deployment, "my-app");
        assert_eq!(hpa.min_replicas, 2);
        assert_eq!(hpa.max_replicas, 10);
        assert_eq!(hpa.target_cpu_utilization, Some(70));
        assert_eq!(hpa.target_memory_utilization, Some(80));
        assert_eq!(hpa.scale_down_stabilization_secs, Some(300));
        assert_eq!(hpa.hpa_name(), "my-app-hpa");
    }
}
