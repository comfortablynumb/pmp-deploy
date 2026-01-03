use async_trait::async_trait;
use aws_sdk_secretsmanager::Client;

use super::provider::{SecretRequest, SecretValue, SecretsProvider};

pub struct AwsSecretsManagerProvider {
    client: Client,
    region: String,
}

impl AwsSecretsManagerProvider {
    pub async fn new(region: Option<&str>) -> anyhow::Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

        let region_str = region
            .map(String::from)
            .or_else(|| config.region().map(|r| r.to_string()))
            .unwrap_or_else(|| "us-east-1".to_string());

        let client = if let Some(r) = region {
            let region_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_sdk_secretsmanager::config::Region::new(r.to_string()))
                .load()
                .await;
            Client::new(&region_config)
        } else {
            Client::new(&config)
        };

        Ok(Self {
            client,
            region: region_str,
        })
    }

    pub fn region(&self) -> &str {
        &self.region
    }
}

#[async_trait]
impl SecretsProvider for AwsSecretsManagerProvider {
    fn name(&self) -> &str {
        "aws-secrets-manager"
    }

    async fn get_secret(&self, request: &SecretRequest) -> anyhow::Result<SecretValue> {
        let mut req = self.client.get_secret_value().secret_id(&request.key);

        if let Some(version) = &request.version {
            req = req.version_id(version);
        }

        let response = req.send().await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to get secret '{}' from AWS Secrets Manager: {}",
                request.key,
                e
            )
        })?;

        let secret_string = response
            .secret_string()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Secret '{}' exists but has no string value (might be binary)",
                    request.key
                )
            })?
            .to_string();

        Ok(SecretValue::new(secret_string))
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        self.client
            .list_secrets()
            .max_results(1)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("AWS Secrets Manager health check failed: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires AWS credentials
    async fn test_aws_provider_health_check() {
        let provider = AwsSecretsManagerProvider::new(Some("us-east-1"))
            .await
            .unwrap();

        let result = provider.health_check().await;
        assert!(result.is_ok());
    }
}
