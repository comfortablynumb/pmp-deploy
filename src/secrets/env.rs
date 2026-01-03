use async_trait::async_trait;
use std::env;

use super::provider::{SecretRequest, SecretValue, SecretsProvider};

pub struct EnvironmentProvider {
    prefix: Option<String>,
}

impl EnvironmentProvider {
    pub fn new() -> Self {
        Self { prefix: None }
    }

    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }

    fn resolve_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("{}_{}", prefix, key),
            None => key.to_string(),
        }
    }
}

impl Default for EnvironmentProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretsProvider for EnvironmentProvider {
    fn name(&self) -> &str {
        "environment"
    }

    async fn get_secret(&self, request: &SecretRequest) -> anyhow::Result<SecretValue> {
        let env_key = self.resolve_key(&request.key);

        env::var(&env_key)
            .map(SecretValue::new)
            .map_err(|_| anyhow::anyhow!("Environment variable '{}' not found", env_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_environment_provider_get_secret() {
        // SAFETY: Test runs in isolation
        unsafe {
            env::set_var("TEST_SECRET_KEY", "test-secret-value");
        }

        let provider = EnvironmentProvider::new();
        let request = SecretRequest::new("TEST_SECRET_KEY");

        let result = provider.get_secret(&request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().expose(), "test-secret-value");
    }

    #[tokio::test]
    async fn test_environment_provider_with_prefix() {
        // SAFETY: Test runs in isolation
        unsafe {
            env::set_var("MYAPP_DB_PASSWORD", "db-password-123");
        }

        let provider = EnvironmentProvider::with_prefix("MYAPP");
        let request = SecretRequest::new("DB_PASSWORD");

        let result = provider.get_secret(&request).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().expose(), "db-password-123");
    }

    #[tokio::test]
    async fn test_environment_provider_missing_secret() {
        let provider = EnvironmentProvider::new();
        let request = SecretRequest::new("NONEXISTENT_SECRET_KEY_12345");

        let result = provider.get_secret(&request).await;
        assert!(result.is_err());
    }
}
