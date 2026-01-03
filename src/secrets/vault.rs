use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

use super::provider::{SecretRequest, SecretValue, SecretsProvider};

#[derive(Debug, Clone)]
pub struct VaultConfig {
    pub address: String,
    pub token: Option<String>,
    pub namespace: Option<String>,
    pub mount_path: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            address: std::env::var("VAULT_ADDR")
                .unwrap_or_else(|_| "http://127.0.0.1:8200".to_string()),
            token: std::env::var("VAULT_TOKEN").ok(),
            namespace: std::env::var("VAULT_NAMESPACE").ok(),
            mount_path: "secret".to_string(),
        }
    }
}

impl VaultConfig {
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            ..Default::default()
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn with_mount_path(mut self, mount_path: impl Into<String>) -> Self {
        self.mount_path = mount_path.into();
        self
    }
}

pub struct VaultProvider {
    client: Client,
    config: VaultConfig,
}

#[derive(Debug, Deserialize)]
struct VaultResponse {
    data: VaultData,
}

#[derive(Debug, Deserialize)]
struct VaultData {
    data: HashMap<String, serde_json::Value>,
}

impl VaultProvider {
    pub fn new(config: VaultConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { client, config })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Self::new(VaultConfig::default())
    }

    fn build_url(&self, path: &str) -> String {
        format!(
            "{}/v1/{}/data/{}",
            self.config.address.trim_end_matches('/'),
            self.config.mount_path,
            path.trim_start_matches('/')
        )
    }

    fn get_token(&self) -> anyhow::Result<&str> {
        self.config
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Vault token not configured"))
    }
}

#[async_trait]
impl SecretsProvider for VaultProvider {
    fn name(&self) -> &str {
        "hashicorp-vault"
    }

    async fn get_secret(&self, request: &SecretRequest) -> anyhow::Result<SecretValue> {
        let url = self.build_url(&request.key);
        let token = self.get_token()?;

        let mut req = self.client.get(&url).header("X-Vault-Token", token);

        if let Some(ns) = &self.config.namespace {
            req = req.header("X-Vault-Namespace", ns);
        }

        if let Some(version) = &request.version {
            req = req.query(&[("version", version)]);
        }

        let response = req.send().await.map_err(|e| {
            anyhow::anyhow!("Failed to connect to Vault at {}: {}", self.config.address, e)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            return Err(anyhow::anyhow!(
                "Vault returned error {} for secret '{}': {}",
                status,
                request.key,
                body
            ));
        }

        let vault_response: VaultResponse = response.json().await.map_err(|e| {
            anyhow::anyhow!("Failed to parse Vault response for '{}': {}", request.key, e)
        })?;

        // Try to get "value" key first, then fall back to JSON serialization of all data
        let secret_value = if let Some(value) = vault_response.data.data.get("value") {
            match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }
        } else {
            serde_json::to_string(&vault_response.data.data)?
        };

        Ok(SecretValue::new(secret_value))
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        let url = format!(
            "{}/v1/sys/health",
            self.config.address.trim_end_matches('/')
        );

        let response = self.client.get(&url).send().await.map_err(|e| {
            anyhow::anyhow!("Failed to connect to Vault at {}: {}", self.config.address, e)
        })?;

        if response.status().is_success() || response.status().as_u16() == 429 {
            // 429 means Vault is unsealed but standby
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Vault health check failed with status: {}",
                response.status()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_config_default() {
        let config = VaultConfig::default();
        assert_eq!(config.mount_path, "secret");
    }

    #[test]
    fn test_vault_config_builder() {
        let config = VaultConfig::new("https://vault.example.com")
            .with_token("my-token")
            .with_namespace("my-namespace")
            .with_mount_path("kv");

        assert_eq!(config.address, "https://vault.example.com");
        assert_eq!(config.token, Some("my-token".to_string()));
        assert_eq!(config.namespace, Some("my-namespace".to_string()));
        assert_eq!(config.mount_path, "kv");
    }

    #[test]
    fn test_vault_build_url() {
        let config = VaultConfig::new("https://vault.example.com")
            .with_token("token")
            .with_mount_path("secret");

        let provider = VaultProvider::new(config).unwrap();

        assert_eq!(
            provider.build_url("my/secret/path"),
            "https://vault.example.com/v1/secret/data/my/secret/path"
        );
    }
}
