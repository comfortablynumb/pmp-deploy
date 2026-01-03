use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SecretValue {
    inner: SecretString,
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: SecretString::from(value.into()),
        }
    }

    pub fn expose(&self) -> &str {
        self.inner.expose_secret()
    }
}

impl From<String> for SecretValue {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for SecretValue {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[derive(Debug, Clone)]
pub struct SecretRequest {
    pub key: String,
    pub version: Option<String>,
}

impl SecretRequest {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            version: None,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

#[async_trait]
pub trait SecretsProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn get_secret(&self, request: &SecretRequest) -> anyhow::Result<SecretValue>;

    async fn get_secrets(
        &self,
        requests: &[SecretRequest],
    ) -> anyhow::Result<HashMap<String, SecretValue>> {
        let mut results = HashMap::new();

        for request in requests {
            let value = self.get_secret(request).await?;
            results.insert(request.key.clone(), value);
        }

        Ok(results)
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_value_expose() {
        let secret = SecretValue::new("my-secret-password");
        assert_eq!(secret.expose(), "my-secret-password");
    }

    #[test]
    fn test_secret_request_builder() {
        let request = SecretRequest::new("prod/db/password").with_version("v1");

        assert_eq!(request.key, "prod/db/password");
        assert_eq!(request.version, Some("v1".to_string()));
    }
}
