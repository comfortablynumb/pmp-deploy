//! Prometheus metrics provider implementation.
//!
//! This provider queries a Prometheus server using PromQL.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;

use crate::error::Result;

use super::provider::{
    CustomMetricQueryParams, DataPoint, MetricGauge, MetricQueryParams, MetricResult,
    MetricsProvider, StandardMetric,
};

/// Configuration for the Prometheus metrics provider.
#[derive(Debug, Clone)]
pub struct PrometheusProviderConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub timeout_seconds: u32,
    pub namespace: Option<String>,
    pub app_label: Option<String>,
}

impl PrometheusProviderConfig {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            username: None,
            password: None,
            timeout_seconds: 30,
            namespace: None,
            app_label: None,
        }
    }

    pub fn with_auth(mut self, username: &str, password: &str) -> Self {
        self.username = Some(username.to_string());
        self.password = Some(password.to_string());
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = Some(namespace.to_string());
        self
    }

    pub fn with_app_label(mut self, app: &str) -> Self {
        self.app_label = Some(app.to_string());
        self
    }
}

/// Prometheus API response structures.
#[derive(Debug, Deserialize)]
struct PrometheusResponse {
    status: String,
    data: PrometheusData,
}

#[derive(Debug, Deserialize)]
struct PrometheusData {
    #[serde(rename = "resultType")]
    result_type: String,
    result: Vec<PrometheusResult>,
}

#[derive(Debug, Deserialize)]
struct PrometheusResult {
    metric: serde_json::Value,
    values: Option<Vec<(f64, String)>>,
    value: Option<(f64, String)>,
}

/// Prometheus metrics provider.
pub struct PrometheusProvider {
    client: Client,
    config: PrometheusProviderConfig,
}

impl PrometheusProvider {
    pub fn new(config: PrometheusProviderConfig) -> Result<Self> {
        use crate::error::InfrastructureError;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.timeout_seconds as u64,
            ))
            .build()
            .map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                    provider: "Prometheus".to_string(),
                    message: format!("Failed to create HTTP client: {}", e),
                })
            })?;

        Ok(Self { client, config })
    }

    /// Build a PromQL query for a standard metric.
    fn build_standard_query(&self, metric: StandardMetric) -> String {
        let namespace_filter = self
            .config
            .namespace
            .as_ref()
            .map(|ns| format!("namespace=\"{}\"", ns))
            .unwrap_or_default();

        let app_filter = self
            .config
            .app_label
            .as_ref()
            .map(|app| format!("app=\"{}\"", app))
            .unwrap_or_default();

        let filters = [namespace_filter, app_filter]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        let filter_clause = if filters.is_empty() {
            String::new()
        } else {
            format!("{{{}}}", filters)
        };

        match metric {
            StandardMetric::CpuUtilization => {
                format!(
                    "sum(rate(container_cpu_usage_seconds_total{}[5m])) * 100",
                    filter_clause
                )
            }
            StandardMetric::MemoryUtilization => {
                format!(
                    "sum(container_memory_working_set_bytes{}) / sum(kube_pod_container_resource_limits{{resource=\"memory\"{}}}) * 100",
                    filter_clause,
                    if filters.is_empty() { "" } else { ", " }
                )
            }
            StandardMetric::ErrorRate => {
                format!(
                    "sum(rate(http_requests_total{{status=~\"5..\"{}}}))[5m]) / sum(rate(http_requests_total{}[5m])) * 100",
                    if filters.is_empty() { "" } else { ", " },
                    filter_clause
                )
            }
            StandardMetric::RequestLatency => {
                format!(
                    "histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket{}[5m])) by (le)) * 1000",
                    filter_clause
                )
            }
            StandardMetric::RequestCount => {
                format!("sum(rate(http_requests_total{}[5m]))", filter_clause)
            }
        }
    }

    /// Execute a PromQL query and return results.
    async fn execute_query(&self, query: &str, start: f64, end: f64, step: u32) -> Result<Vec<DataPoint>> {
        use crate::error::InfrastructureError;

        let url = format!("{}/api/v1/query_range", self.config.url);

        let mut request = self.client.get(&url).query(&[
            ("query", query),
            ("start", &start.to_string()),
            ("end", &end.to_string()),
            ("step", &step.to_string()),
        ]);

        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                provider: "Prometheus".to_string(),
                message: format!("Query failed: {}", e),
            })
        })?;

        if !response.status().is_success() {
            return Err(crate::error::Error::Infrastructure(
                InfrastructureError::ProviderError {
                    provider: "Prometheus".to_string(),
                    message: format!("Returned status: {}", response.status()),
                },
            ));
        }

        let prom_response: PrometheusResponse = response.json().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                provider: "Prometheus".to_string(),
                message: format!("Failed to parse response: {}", e),
            })
        })?;

        if prom_response.status != "success" {
            return Err(crate::error::Error::Infrastructure(
                InfrastructureError::ProviderError {
                    provider: "Prometheus".to_string(),
                    message: "Query returned non-success status".to_string(),
                },
            ));
        }

        let mut data_points = Vec::new();

        for result in prom_response.data.result {
            if let Some(values) = result.values {
                for (timestamp, value_str) in values {
                    if let Ok(value) = value_str.parse::<f64>() {
                        data_points.push(DataPoint {
                            timestamp: (timestamp * 1000.0) as u64,
                            value,
                        });
                    }
                }
            }
        }

        Ok(data_points)
    }

    /// Execute an instant query and return the current value.
    async fn execute_instant_query(&self, query: &str) -> Result<f64> {
        use crate::error::InfrastructureError;

        let url = format!("{}/api/v1/query", self.config.url);

        let mut request = self.client.get(&url).query(&[("query", query)]);

        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                provider: "Prometheus".to_string(),
                message: format!("Query failed: {}", e),
            })
        })?;

        let prom_response: PrometheusResponse = response.json().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                provider: "Prometheus".to_string(),
                message: format!("Failed to parse response: {}", e),
            })
        })?;

        if let Some(result) = prom_response.data.result.first() {
            if let Some((_, value_str)) = &result.value {
                return value_str.parse::<f64>().map_err(|e| {
                    crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                        provider: "Prometheus".to_string(),
                        message: format!("Failed to parse value: {}", e),
                    })
                });
            }
        }

        Err(crate::error::Error::Infrastructure(
            InfrastructureError::ResourceNotFound {
                resource_type: "Metric".to_string(),
                name: "No data returned from Prometheus".to_string(),
            },
        ))
    }

    pub fn config(&self) -> &PrometheusProviderConfig {
        &self.config
    }
}

#[async_trait]
impl MetricsProvider for PrometheusProvider {
    fn name(&self) -> &str {
        "prometheus"
    }

    fn supported_infrastructure_types(&self) -> Vec<&str> {
        vec!["kubernetes", "docker-compose"]
    }

    async fn query_metric(&self, params: &MetricQueryParams) -> Result<MetricResult> {
        let query = self.build_standard_query(params.metric);

        let start = params
            .time_range
            .start
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let end = params
            .time_range
            .end
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let data_points = self
            .execute_query(&query, start, end, params.period_seconds)
            .await?;

        Ok(MetricResult {
            metric_name: params.metric.display_name().to_string(),
            unit: params.metric.default_unit().to_string(),
            data_points,
        })
    }

    async fn get_current_value(&self, params: &MetricQueryParams) -> Result<MetricGauge> {
        let query = self.build_standard_query(params.metric);
        let value = self.execute_instant_query(&query).await.unwrap_or(0.0);

        Ok(MetricGauge::new(
            params.metric.display_name(),
            params.metric.display_name(),
            value,
            params.metric.default_unit(),
        ))
    }

    async fn query_custom(&self, params: &CustomMetricQueryParams) -> Result<MetricResult> {
        let start = params
            .time_range
            .start
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let end = params
            .time_range
            .end
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let data_points = self
            .execute_query(&params.query, start, end, params.period_seconds)
            .await?;

        Ok(MetricResult {
            metric_name: params.name.clone(),
            unit: "value".to_string(),
            data_points,
        })
    }

    async fn health_check(&self) -> Result<()> {
        use crate::error::InfrastructureError;

        let url = format!("{}/-/healthy", self.config.url);

        let response = self.client.get(&url).send().await.map_err(|e| {
            crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                provider: "Prometheus".to_string(),
                message: format!("Health check failed: {}", e),
            })
        })?;

        if !response.status().is_success() {
            return Err(crate::error::Error::Infrastructure(
                InfrastructureError::ConnectionFailed {
                    provider: "Prometheus".to_string(),
                    message: format!("Health check returned status: {}", response.status()),
                },
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = PrometheusProviderConfig::new("http://prometheus:9090")
            .with_auth("user", "pass")
            .with_namespace("production")
            .with_app_label("myapp");

        assert_eq!(config.url, "http://prometheus:9090");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.namespace, Some("production".to_string()));
        assert_eq!(config.app_label, Some("myapp".to_string()));
    }
}
