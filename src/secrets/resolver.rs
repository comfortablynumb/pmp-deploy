use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{SecretsConfig, SecretsProvider as ConfigSecretsProvider};

use super::aws::AwsSecretsManagerProvider;
use super::env::EnvironmentProvider;
use super::provider::{SecretRequest, SecretValue, SecretsProvider};
use super::vault::{VaultConfig, VaultProvider};

pub struct SecretsResolver {
    providers: HashMap<String, Arc<dyn SecretsProvider>>,
    default_provider: String,
}

impl SecretsResolver {
    pub fn new() -> Self {
        let env_provider: Arc<dyn SecretsProvider> = Arc::new(EnvironmentProvider::new());

        let mut providers: HashMap<String, Arc<dyn SecretsProvider>> = HashMap::new();
        providers.insert("environment".to_string(), env_provider);

        Self {
            providers,
            default_provider: "environment".to_string(),
        }
    }

    pub async fn from_config(config: Option<&SecretsConfig>) -> anyhow::Result<Self> {
        let mut resolver = Self::new();

        if let Some(secrets_config) = config {
            match secrets_config.provider {
                ConfigSecretsProvider::Environment => {
                    resolver.default_provider = "environment".to_string();
                }
                ConfigSecretsProvider::AwsSecretsManager => {
                    let aws_provider = AwsSecretsManagerProvider::new(None).await?;
                    resolver.register_provider(Arc::new(aws_provider));
                    resolver.default_provider = "aws-secrets-manager".to_string();
                }
                ConfigSecretsProvider::HashicorpVault => {
                    let vault_provider = VaultProvider::from_env()?;
                    resolver.register_provider(Arc::new(vault_provider));
                    resolver.default_provider = "hashicorp-vault".to_string();
                }
            }
        }

        Ok(resolver)
    }

    pub fn register_provider(&mut self, provider: Arc<dyn SecretsProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn set_default_provider(&mut self, name: impl Into<String>) {
        self.default_provider = name.into();
    }

    pub async fn add_aws_provider(&mut self, region: Option<&str>) -> anyhow::Result<()> {
        let provider = AwsSecretsManagerProvider::new(region).await?;
        self.register_provider(Arc::new(provider));
        Ok(())
    }

    pub fn add_vault_provider(&mut self, config: VaultConfig) -> anyhow::Result<()> {
        let provider = VaultProvider::new(config)?;
        self.register_provider(Arc::new(provider));
        Ok(())
    }

    pub async fn resolve(&self, key: &str) -> anyhow::Result<SecretValue> {
        self.resolve_with_provider(&self.default_provider, key, None)
            .await
    }

    pub async fn resolve_with_provider(
        &self,
        provider_name: &str,
        key: &str,
        version: Option<&str>,
    ) -> anyhow::Result<SecretValue> {
        let provider = self.providers.get(provider_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Secrets provider '{}' not found. Available: {:?}",
                provider_name,
                self.providers.keys().collect::<Vec<_>>()
            )
        })?;

        let mut request = SecretRequest::new(key);

        if let Some(v) = version {
            request = request.with_version(v);
        }

        provider.get_secret(&request).await
    }

    pub async fn resolve_all(
        &self,
        secrets: &HashMap<String, crate::config::schema::SecretReference>,
    ) -> anyhow::Result<HashMap<String, SecretValue>> {
        let mut resolved = HashMap::new();

        for (name, reference) in secrets {
            let provider_name = reference
                .provider
                .as_ref()
                .map(|p| match p {
                    ConfigSecretsProvider::Environment => "environment",
                    ConfigSecretsProvider::AwsSecretsManager => "aws-secrets-manager",
                    ConfigSecretsProvider::HashicorpVault => "hashicorp-vault",
                })
                .unwrap_or(&self.default_provider);

            let value = self
                .resolve_with_provider(provider_name, &reference.key, reference.version.as_deref())
                .await?;

            resolved.insert(name.clone(), value);
        }

        Ok(resolved)
    }

    pub async fn health_check_all(&self) -> HashMap<String, anyhow::Result<()>> {
        let mut results = HashMap::new();

        for (name, provider) in &self.providers {
            let result = provider.health_check().await;
            results.insert(name.clone(), result);
        }

        results
    }
}

impl Default for SecretsResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resolver_default_provider() {
        // SAFETY: Test runs in isolation
        unsafe {
            std::env::set_var("TEST_RESOLVER_SECRET", "resolver-secret-value");
        }

        let resolver = SecretsResolver::new();
        let result = resolver.resolve("TEST_RESOLVER_SECRET").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().expose(), "resolver-secret-value");
    }

    #[tokio::test]
    async fn test_resolver_with_explicit_provider() {
        // SAFETY: Test runs in isolation
        unsafe {
            std::env::set_var("TEST_EXPLICIT_SECRET", "explicit-value");
        }

        let resolver = SecretsResolver::new();
        let result = resolver
            .resolve_with_provider("environment", "TEST_EXPLICIT_SECRET", None)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().expose(), "explicit-value");
    }

    #[tokio::test]
    async fn test_resolver_unknown_provider() {
        let resolver = SecretsResolver::new();
        let result = resolver
            .resolve_with_provider("unknown-provider", "some-key", None)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
