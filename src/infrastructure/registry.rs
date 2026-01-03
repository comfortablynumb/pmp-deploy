use std::collections::HashMap;
use std::sync::Arc;

use super::provider::{InfrastructureProvider, InfrastructureType};
use crate::config::InfrastructureConfig;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn InfrastructureProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &str, provider: Arc<dyn InfrastructureProvider>) {
        self.providers.insert(name.to_string(), provider);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn InfrastructureProvider>> {
        self.providers.get(name).cloned()
    }

    pub fn list(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub async fn create(
        _name: &str,
        config: &InfrastructureConfig,
    ) -> anyhow::Result<Arc<dyn InfrastructureProvider>> {
        let infra_type = InfrastructureType::from_str(&config.infrastructure_type);

        match infra_type {
            InfrastructureType::DockerCompose => {
                let provider = super::docker_compose::DockerComposeProvider::from_config(config)?;
                Ok(Arc::new(provider))
            }

            #[cfg(feature = "kubernetes")]
            InfrastructureType::Kubernetes => {
                let provider = super::kubernetes::KubernetesProvider::from_config(config).await?;
                Ok(Arc::new(provider))
            }

            #[cfg(not(feature = "kubernetes"))]
            InfrastructureType::Kubernetes => {
                anyhow::bail!(
                    "Kubernetes support is not enabled. \
                    Rebuild with --features kubernetes to enable it."
                )
            }

            #[cfg(feature = "kubernetes")]
            InfrastructureType::AwsEks => {
                let provider = super::aws_eks::AwsEksProvider::from_config(config).await?;
                Ok(Arc::new(provider))
            }

            #[cfg(not(feature = "kubernetes"))]
            InfrastructureType::AwsEks => {
                anyhow::bail!(
                    "AWS EKS support is not enabled. \
                    Rebuild with --features kubernetes to enable it."
                )
            }

            InfrastructureType::AwsEcs => {
                let provider = super::aws_ecs::AwsEcsProvider::from_config(config).await?;
                Ok(Arc::new(provider))
            }

            InfrastructureType::AwsLambda => {
                let provider = super::aws_lambda::AwsLambdaProvider::from_config(config).await?;
                Ok(Arc::new(provider))
            }

            InfrastructureType::Custom(type_name) => {
                anyhow::bail!(
                    "Custom infrastructure type '{}' requires a plugin. \
                    Check ~/.pmp-deploy/plugins/ for available plugins.",
                    type_name
                )
            }
        }
    }

    pub async fn create_registry(
        configs: &HashMap<String, InfrastructureConfig>,
    ) -> anyhow::Result<ProviderRegistry> {
        let mut registry = ProviderRegistry::new();

        for (name, config) in configs {
            let provider = Self::create(name, config).await?;
            registry.register(name, provider);
        }

        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_register_and_get() {
        use super::super::docker_compose::DockerComposeProvider;

        let mut registry = ProviderRegistry::new();
        let provider = DockerComposeProvider::new("docker-compose.yml", None);

        registry.register("local", Arc::new(provider));

        assert!(registry.get("local").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
