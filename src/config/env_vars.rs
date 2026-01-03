use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::secrets::{
    AwsSecretsManagerProvider, SecretRequest, SecretsProvider as SecretsProviderTrait,
    VaultConfig, VaultProvider,
};

/// Source for an environment variable value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EnvVarSource {
    /// Static value defined inline
    StaticValue,
    /// From OS environment variable
    Environment,
    /// From AWS Secrets Manager
    AwsSecretsManager,
    /// From HashiCorp Vault
    Vault,
}

impl Default for EnvVarSource {
    fn default() -> Self {
        Self::StaticValue
    }
}

/// Configuration for a single environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarConfig {
    /// Source of the value
    #[serde(default)]
    pub source: EnvVarSource,

    /// Static value (when source is static_value)
    #[serde(default)]
    pub value: Option<String>,

    /// Secret ARN or key (when source is aws_secrets_manager or vault)
    #[serde(default)]
    pub arn: Option<String>,

    /// Secret key path (alternative to arn, for vault or generic references)
    #[serde(default)]
    pub key: Option<String>,

    /// Environment variable name to read from (when source is environment)
    #[serde(default)]
    pub env_var: Option<String>,

    /// Secret version (optional)
    #[serde(default)]
    pub version: Option<String>,

    /// JSON field to extract from secret (for structured secrets)
    #[serde(default)]
    pub json_field: Option<String>,
}

impl EnvVarConfig {
    pub fn static_value(value: impl Into<String>) -> Self {
        Self {
            source: EnvVarSource::StaticValue,
            value: Some(value.into()),
            arn: None,
            key: None,
            env_var: None,
            version: None,
            json_field: None,
        }
    }

    pub fn from_env(env_var: impl Into<String>) -> Self {
        Self {
            source: EnvVarSource::Environment,
            value: None,
            arn: None,
            key: None,
            env_var: Some(env_var.into()),
            version: None,
            json_field: None,
        }
    }

    pub fn from_aws_secrets_manager(arn: impl Into<String>) -> Self {
        Self {
            source: EnvVarSource::AwsSecretsManager,
            value: None,
            arn: Some(arn.into()),
            key: None,
            env_var: None,
            version: None,
            json_field: None,
        }
    }

    pub fn from_vault(key: impl Into<String>) -> Self {
        Self {
            source: EnvVarSource::Vault,
            value: None,
            arn: None,
            key: Some(key.into()),
            env_var: None,
            version: None,
            json_field: None,
        }
    }

    pub fn with_json_field(mut self, field: impl Into<String>) -> Self {
        self.json_field = Some(field.into());
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }
}

/// Resolves environment variables from various sources
pub struct EnvVarResolver {
    aws_region: Option<String>,
    vault_addr: Option<String>,
    vault_token: Option<String>,
}

impl EnvVarResolver {
    pub fn new() -> Self {
        Self {
            aws_region: std::env::var("AWS_REGION").ok(),
            vault_addr: std::env::var("VAULT_ADDR").ok(),
            vault_token: std::env::var("VAULT_TOKEN").ok(),
        }
    }

    pub fn with_aws_region(mut self, region: impl Into<String>) -> Self {
        self.aws_region = Some(region.into());
        self
    }

    pub fn with_vault(mut self, addr: impl Into<String>, token: impl Into<String>) -> Self {
        self.vault_addr = Some(addr.into());
        self.vault_token = Some(token.into());
        self
    }

    /// Resolve all environment variables to their actual values
    pub async fn resolve(
        &self,
        env_vars: &HashMap<String, EnvVarConfig>,
    ) -> anyhow::Result<HashMap<String, String>> {
        let mut resolved = HashMap::new();

        for (name, config) in env_vars {
            let value = self.resolve_single(name, config).await?;
            resolved.insert(name.clone(), value);
        }

        Ok(resolved)
    }

    async fn resolve_single(&self, name: &str, config: &EnvVarConfig) -> anyhow::Result<String> {
        match config.source {
            EnvVarSource::StaticValue => config.value.clone().ok_or_else(|| {
                anyhow::anyhow!("Environment variable '{}' has static_value source but no value", name)
            }),

            EnvVarSource::Environment => {
                let default_name = name.to_string();
                let env_var_name = config.env_var.as_ref().unwrap_or(&default_name);
                std::env::var(env_var_name).map_err(|_| {
                    anyhow::anyhow!(
                        "Environment variable '{}' not found (looking for '{}')",
                        name,
                        env_var_name
                    )
                })
            }

            EnvVarSource::AwsSecretsManager => {
                let secret_id = config
                    .arn
                    .as_ref()
                    .or(config.key.as_ref())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Environment variable '{}' has aws_secrets_manager source but no arn or key",
                            name
                        )
                    })?;

                let region = self.aws_region.as_deref();
                let provider = AwsSecretsManagerProvider::new(region).await?;
                let request = SecretRequest::new(secret_id);
                let secret = provider.get_secret(&request).await?;

                self.extract_value(secret.expose(), &config.json_field)
            }

            EnvVarSource::Vault => {
                let key = config.key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Environment variable '{}' has vault source but no key",
                        name
                    )
                })?;

                let addr = self.vault_addr.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("VAULT_ADDR not set for vault secret '{}'", name)
                })?;

                let token = self.vault_token.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("VAULT_TOKEN not set for vault secret '{}'", name)
                })?;

                let vault_config = VaultConfig::new(addr).with_token(token);
                let provider = VaultProvider::new(vault_config)?;
                let request = SecretRequest::new(key);
                let secret = provider.get_secret(&request).await?;

                self.extract_value(secret.expose(), &config.json_field)
            }
        }
    }

    fn extract_value(
        &self,
        raw_value: &str,
        json_field: &Option<String>,
    ) -> anyhow::Result<String> {
        match json_field {
            Some(field) => {
                let json: serde_json::Value = serde_json::from_str(raw_value).map_err(|e| {
                    anyhow::anyhow!("Failed to parse secret as JSON: {}", e)
                })?;

                json.get(field)
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .or_else(|| json.get(field).map(|v| v.to_string()))
                    .ok_or_else(|| anyhow::anyhow!("JSON field '{}' not found in secret", field))
            }
            None => Ok(raw_value.to_string()),
        }
    }
}

impl Default for EnvVarResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to convert legacy HashMap<String, String> to new format
pub fn from_legacy_env(env: &HashMap<String, String>) -> HashMap<String, EnvVarConfig> {
    env.iter()
        .map(|(k, v)| (k.clone(), EnvVarConfig::static_value(v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_source_default() {
        let source: EnvVarSource = Default::default();
        assert_eq!(source, EnvVarSource::StaticValue);
    }

    #[test]
    fn test_env_var_config_static() {
        let config = EnvVarConfig::static_value("my-value");
        assert_eq!(config.source, EnvVarSource::StaticValue);
        assert_eq!(config.value, Some("my-value".to_string()));
    }

    #[test]
    fn test_env_var_config_from_env() {
        let config = EnvVarConfig::from_env("MY_VAR");
        assert_eq!(config.source, EnvVarSource::Environment);
        assert_eq!(config.env_var, Some("MY_VAR".to_string()));
    }

    #[test]
    fn test_env_var_config_from_aws() {
        let config = EnvVarConfig::from_aws_secrets_manager("arn:aws:secretsmanager:...");
        assert_eq!(config.source, EnvVarSource::AwsSecretsManager);
        assert_eq!(
            config.arn,
            Some("arn:aws:secretsmanager:...".to_string())
        );
    }

    #[test]
    fn test_env_var_config_from_vault() {
        let config = EnvVarConfig::from_vault("secret/data/myapp").with_json_field("password");
        assert_eq!(config.source, EnvVarSource::Vault);
        assert_eq!(config.key, Some("secret/data/myapp".to_string()));
        assert_eq!(config.json_field, Some("password".to_string()));
    }

    #[test]
    fn test_env_var_config_parsing() {
        let yaml = r#"
MY_STATIC:
  source: static_value
  value: hello-world
MY_ENV:
  source: environment
  env_var: DATABASE_URL
MY_SECRET:
  source: aws_secrets_manager
  arn: arn:aws:secretsmanager:us-east-1:123456789:secret:my-secret
  json_field: password
"#;
        let config: HashMap<String, EnvVarConfig> = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.len(), 3);
        assert_eq!(config["MY_STATIC"].source, EnvVarSource::StaticValue);
        assert_eq!(config["MY_ENV"].source, EnvVarSource::Environment);
        assert_eq!(config["MY_SECRET"].source, EnvVarSource::AwsSecretsManager);
    }

    #[test]
    fn test_from_legacy_env() {
        let mut legacy = HashMap::new();
        legacy.insert("FOO".to_string(), "bar".to_string());
        legacy.insert("BAZ".to_string(), "qux".to_string());

        let converted = from_legacy_env(&legacy);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted["FOO"].source, EnvVarSource::StaticValue);
        assert_eq!(converted["FOO"].value, Some("bar".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_static_value() {
        let resolver = EnvVarResolver::new();
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "MY_VAR".to_string(),
            EnvVarConfig::static_value("test-value"),
        );

        let resolved = resolver.resolve(&env_vars).await.unwrap();

        assert_eq!(resolved["MY_VAR"], "test-value");
    }

    #[tokio::test]
    async fn test_resolve_environment() {
        // SAFETY: Test code running in isolation, setting a unique test-specific env var
        unsafe {
            std::env::set_var("TEST_ENV_VAR_FOR_RESOLVER", "from-env");
        }

        let resolver = EnvVarResolver::new();
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "MY_VAR".to_string(),
            EnvVarConfig::from_env("TEST_ENV_VAR_FOR_RESOLVER"),
        );

        let resolved = resolver.resolve(&env_vars).await.unwrap();

        assert_eq!(resolved["MY_VAR"], "from-env");

        // SAFETY: Cleanup after test
        unsafe {
            std::env::remove_var("TEST_ENV_VAR_FOR_RESOLVER");
        }
    }

    #[test]
    fn test_extract_json_field() {
        let resolver = EnvVarResolver::new();

        let json = r#"{"username": "admin", "password": "secret123"}"#;

        let value = resolver
            .extract_value(json, &Some("password".to_string()))
            .unwrap();
        assert_eq!(value, "secret123");

        let value = resolver.extract_value("plain-text", &None).unwrap();
        assert_eq!(value, "plain-text");
    }
}
