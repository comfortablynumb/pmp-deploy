//! HTTP webhook hook executor.

use async_trait::async_trait;
use std::time::Instant;
use tracing::{debug, info};

use crate::error::Result;

use super::executor::{HookContext, HookExecutor, HookResult};
use super::types::{HookConfig, HookType, HttpHookConfig};

/// HTTP webhook hook executor.
pub struct HttpHookExecutor {
    client: reqwest::Client,
}

impl HttpHookExecutor {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    fn get_http_config(hook: &HookConfig) -> Option<&HttpHookConfig> {
        hook.config.http.as_ref()
    }

    fn is_success_status(&self, status: u16, expected: &[u16]) -> bool {
        if expected.is_empty() {
            // Default: accept any 2xx status
            (200..300).contains(&status)
        } else {
            expected.contains(&status)
        }
    }

    async fn make_request(
        &self,
        config: &HttpHookConfig,
        hook_name: &str,
        _context: &HookContext,
    ) -> Result<HookResult> {
        let start = Instant::now();

        let method = config.method.to_uppercase();
        let method: reqwest::Method = method.parse().map_err(|_| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook_name.to_string(),
                message: format!("Invalid HTTP method: {}", config.method),
            })
        })?;

        debug!("Making {} request to {}", method, config.url);

        let mut request = self.client.request(method, &config.url);

        // Add content-type header
        request = request.header("Content-Type", &config.content_type);

        // Add custom headers
        for (key, value) in &config.headers {
            request = request.header(key, value);
        }

        // Add body if present
        if let Some(ref body) = config.body {
            request = request.body(body.clone());
        }

        let response = request.send().await.map_err(|e| {
            crate::error::Error::Network(crate::error::NetworkError::HttpError {
                status: 0,
                message: format!("Request failed: {}", e),
            })
        })?;

        let duration = start.elapsed();
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        if self.is_success_status(status, &config.expected_status) {
            info!(
                "HTTP hook '{}' completed successfully (status: {})",
                hook_name, status
            );
            Ok(HookResult::success(hook_name, body, duration))
        } else {
            Ok(HookResult::failure(
                hook_name,
                format!("HTTP request returned status {}: {}", status, body),
                duration,
            )
            .with_exit_code(status as i32))
        }
    }
}

impl Default for HttpHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookExecutor for HttpHookExecutor {
    fn hook_type(&self) -> HookType {
        HookType::Http
    }

    async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult> {
        let config = Self::get_http_config(hook).ok_or_else(|| {
            crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: "HTTP hook configuration not found".to_string(),
            })
        })?;

        if config.url.is_empty() {
            return Err(crate::error::Error::Deployment(
                crate::error::DeploymentError::HookFailed {
                    hook_name: hook.name.clone(),
                    message: "HTTP URL is required".to_string(),
                },
            ));
        }

        self.make_request(config, &hook.name, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::types::HookTypeConfig;

    #[test]
    fn test_http_executor_hook_type() {
        let executor = HttpHookExecutor::new();
        assert_eq!(executor.hook_type(), HookType::Http);
    }

    #[test]
    fn test_is_success_status_default() {
        let executor = HttpHookExecutor::new();

        // Empty expected means 2xx is success
        assert!(executor.is_success_status(200, &[]));
        assert!(executor.is_success_status(201, &[]));
        assert!(executor.is_success_status(204, &[]));
        assert!(!executor.is_success_status(400, &[]));
        assert!(!executor.is_success_status(500, &[]));
    }

    #[test]
    fn test_is_success_status_custom() {
        let executor = HttpHookExecutor::new();

        // Custom expected statuses
        assert!(executor.is_success_status(200, &[200, 201]));
        assert!(executor.is_success_status(201, &[200, 201]));
        assert!(!executor.is_success_status(204, &[200, 201]));
    }

    #[tokio::test]
    async fn test_http_hook_missing_config() {
        let executor = HttpHookExecutor::new();
        let hook = HookConfig {
            name: "test".to_string(),
            hook_type: HookType::Http,
            config: HookTypeConfig::default(),
            timeout_secs: 60,
            fail_on_error: true,
            description: None,
        };
        let context = HookContext::default();

        let result = executor.execute(&hook, &context).await;
        assert!(result.is_err());
    }
}
