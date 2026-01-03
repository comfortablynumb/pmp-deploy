//! CloudWatch metrics provider implementation.
//!
//! This provider queries AWS CloudWatch for metrics from ECS, EKS, and Lambda.

use async_trait::async_trait;
use aws_sdk_cloudwatch::types::{Dimension, Metric, MetricDataQuery, MetricStat};
use aws_sdk_cloudwatch::Client;
use std::collections::HashMap;

use crate::error::{InfrastructureError, Result};

use super::provider::{
    CustomMetricQueryParams, DataPoint, MetricGauge, MetricQueryParams, MetricResult,
    MetricsProvider, StandardMetric,
};

/// Configuration for the CloudWatch metrics provider.
#[derive(Debug, Clone)]
pub struct CloudWatchProviderConfig {
    pub region: String,
    pub namespace: Option<String>,
    pub ecs_cluster: Option<String>,
    pub ecs_service: Option<String>,
    pub eks_cluster: Option<String>,
    pub lambda_function: Option<String>,
    pub dimensions: HashMap<String, String>,
}

impl CloudWatchProviderConfig {
    pub fn new(region: &str) -> Self {
        Self {
            region: region.to_string(),
            namespace: None,
            ecs_cluster: None,
            ecs_service: None,
            eks_cluster: None,
            lambda_function: None,
            dimensions: HashMap::new(),
        }
    }

    pub fn with_ecs(mut self, cluster: &str, service: &str) -> Self {
        self.ecs_cluster = Some(cluster.to_string());
        self.ecs_service = Some(service.to_string());
        self
    }

    pub fn with_eks(mut self, cluster: &str) -> Self {
        self.eks_cluster = Some(cluster.to_string());
        self
    }

    pub fn with_lambda(mut self, function_name: &str) -> Self {
        self.lambda_function = Some(function_name.to_string());
        self
    }
}

/// CloudWatch metrics provider.
pub struct CloudWatchProvider {
    client: Client,
    config: CloudWatchProviderConfig,
}

impl CloudWatchProvider {
    pub async fn new(config: CloudWatchProviderConfig) -> Result<Self> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_cloudwatch::config::Region::new(config.region.clone()))
            .load()
            .await;

        let client = Client::new(&aws_config);

        Ok(Self { client, config })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn config(&self) -> &CloudWatchProviderConfig {
        &self.config
    }

    /// Build metric queries for ECS infrastructure.
    fn build_ecs_query(&self, metric: StandardMetric, period: i32) -> Option<MetricDataQuery> {
        let (cluster, service) = match (&self.config.ecs_cluster, &self.config.ecs_service) {
            (Some(c), Some(s)) => (c, s),
            _ => return None,
        };

        let (namespace, metric_name, stat) = match metric {
            StandardMetric::CpuUtilization => ("AWS/ECS", "CPUUtilization", "Average"),
            StandardMetric::MemoryUtilization => ("AWS/ECS", "MemoryUtilization", "Average"),
            StandardMetric::ErrorRate => return None, // ECS doesn't have built-in error rate
            StandardMetric::RequestLatency => return None,
            StandardMetric::RequestCount => return None,
        };

        let dimensions = vec![
            Dimension::builder()
                .name("ClusterName")
                .value(cluster)
                .build(),
            Dimension::builder()
                .name("ServiceName")
                .value(service)
                .build(),
        ];

        Some(self.build_metric_data_query(
            "ecs_metric",
            namespace,
            metric_name,
            dimensions,
            stat,
            period,
        ))
    }

    /// Build metric queries for Lambda infrastructure.
    fn build_lambda_query(&self, metric: StandardMetric, period: i32) -> Option<MetricDataQuery> {
        let function_name = self.config.lambda_function.as_ref()?;

        let (metric_name, stat) = match metric {
            StandardMetric::CpuUtilization => return None, // Lambda doesn't expose CPU directly
            StandardMetric::MemoryUtilization => return None, // Use custom metric
            StandardMetric::ErrorRate => ("Errors", "Sum"),
            StandardMetric::RequestLatency => ("Duration", "Average"),
            StandardMetric::RequestCount => ("Invocations", "Sum"),
        };

        let dimensions = vec![Dimension::builder()
            .name("FunctionName")
            .value(function_name)
            .build()];

        Some(self.build_metric_data_query(
            "lambda_metric",
            "AWS/Lambda",
            metric_name,
            dimensions,
            stat,
            period,
        ))
    }

    /// Build metric queries for EKS infrastructure.
    fn build_eks_query(&self, metric: StandardMetric, period: i32) -> Option<MetricDataQuery> {
        let cluster = self.config.eks_cluster.as_ref()?;

        let (namespace, metric_name, stat) = match metric {
            StandardMetric::CpuUtilization => {
                ("ContainerInsights", "pod_cpu_utilization", "Average")
            }
            StandardMetric::MemoryUtilization => {
                ("ContainerInsights", "pod_memory_utilization", "Average")
            }
            StandardMetric::ErrorRate => return None,
            StandardMetric::RequestLatency => return None,
            StandardMetric::RequestCount => return None,
        };

        let dimensions = vec![Dimension::builder()
            .name("ClusterName")
            .value(cluster)
            .build()];

        Some(self.build_metric_data_query(
            "eks_metric",
            namespace,
            metric_name,
            dimensions,
            stat,
            period,
        ))
    }

    /// Helper to build a MetricDataQuery.
    fn build_metric_data_query(
        &self,
        id: &str,
        namespace: &str,
        metric_name: &str,
        dimensions: Vec<Dimension>,
        stat: &str,
        period: i32,
    ) -> MetricDataQuery {
        let metric = Metric::builder()
            .namespace(namespace)
            .metric_name(metric_name)
            .set_dimensions(Some(dimensions))
            .build();

        let metric_stat = MetricStat::builder()
            .metric(metric)
            .period(period)
            .stat(stat)
            .build();

        MetricDataQuery::builder()
            .id(id)
            .metric_stat(metric_stat)
            .return_data(true)
            .build()
    }

    /// Execute a metric query and return data points.
    async fn execute_metric_query(
        &self,
        query: MetricDataQuery,
        params: &MetricQueryParams,
    ) -> Result<Vec<DataPoint>> {
        let start_time = aws_sdk_cloudwatch::primitives::DateTime::from(params.time_range.start);
        let end_time = aws_sdk_cloudwatch::primitives::DateTime::from(params.time_range.end);

        let response = self
            .client
            .get_metric_data()
            .start_time(start_time)
            .end_time(end_time)
            .metric_data_queries(query)
            .send()
            .await
            .map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                    provider: "CloudWatch".to_string(),
                    message: format!("Failed to get metric data: {}", e),
                })
            })?;

        let mut data_points = Vec::new();

        if let Some(results) = response.metric_data_results {
            for result in results {
                if let (Some(timestamps), Some(values)) = (result.timestamps, result.values) {
                    for (ts, value) in timestamps.into_iter().zip(values.into_iter()) {
                        data_points.push(DataPoint {
                            timestamp: (ts.as_secs_f64() * 1000.0) as u64,
                            value,
                        });
                    }
                }
            }
        }

        // Sort by timestamp
        data_points.sort_by_key(|dp| dp.timestamp);

        Ok(data_points)
    }

    /// Parse a custom CloudWatch query from JSON.
    fn parse_custom_query(&self, query_json: &str, period: i32) -> Result<MetricDataQuery> {
        let query_config: serde_json::Value =
            serde_json::from_str(query_json).map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                    provider: "CloudWatch".to_string(),
                    message: format!("Failed to parse custom query JSON: {}", e),
                })
            })?;

        let namespace = query_config["Namespace"]
            .as_str()
            .unwrap_or("AWS/ECS");

        let metric_name = query_config["MetricName"]
            .as_str()
            .ok_or_else(|| {
                crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                    provider: "CloudWatch".to_string(),
                    message: "Custom query missing MetricName".to_string(),
                })
            })?;

        let stat = query_config["Statistic"]
            .as_str()
            .unwrap_or("Average");

        let mut dimensions = Vec::new();

        if let Some(dims) = query_config["Dimensions"].as_array() {
            for dim in dims {
                if let (Some(name), Some(value)) = (dim["Name"].as_str(), dim["Value"].as_str()) {
                    dimensions.push(Dimension::builder().name(name).value(value).build());
                }
            }
        }

        Ok(self.build_metric_data_query(
            "custom_metric",
            namespace,
            metric_name,
            dimensions,
            stat,
            period,
        ))
    }
}

#[async_trait]
impl MetricsProvider for CloudWatchProvider {
    fn name(&self) -> &str {
        "cloudwatch"
    }

    fn supported_infrastructure_types(&self) -> Vec<&str> {
        vec!["aws-ecs", "aws-eks", "aws-lambda"]
    }

    async fn query_metric(&self, params: &MetricQueryParams) -> Result<MetricResult> {
        let period = params.period_seconds as i32;

        // Try to build query based on infrastructure type
        let query = match params.infrastructure_type.as_str() {
            "aws-ecs" => self.build_ecs_query(params.metric, period),
            "aws-lambda" => self.build_lambda_query(params.metric, period),
            "aws-eks" => self.build_eks_query(params.metric, period),
            _ => None,
        };

        let query = match query {
            Some(q) => q,
            None => {
                return Ok(MetricResult::empty(
                    params.metric.display_name(),
                    params.metric.default_unit(),
                ))
            }
        };

        let data_points = self.execute_metric_query(query, params).await?;

        Ok(MetricResult {
            metric_name: params.metric.display_name().to_string(),
            unit: params.metric.default_unit().to_string(),
            data_points,
        })
    }

    async fn get_current_value(&self, params: &MetricQueryParams) -> Result<MetricGauge> {
        // Query with a short time range to get current value
        let mut current_params = params.clone();
        current_params.time_range = super::provider::TimeRange::last_minutes(5);
        current_params.period_seconds = 60;

        let result = self.query_metric(&current_params).await?;

        let value = result.latest_value().unwrap_or(0.0);

        Ok(MetricGauge::new(
            params.metric.display_name(),
            params.metric.display_name(),
            value,
            params.metric.default_unit(),
        ))
    }

    async fn query_custom(&self, params: &CustomMetricQueryParams) -> Result<MetricResult> {
        let query = self.parse_custom_query(&params.query, params.period_seconds as i32)?;

        // Create a temporary MetricQueryParams for execution
        let metric_params = MetricQueryParams {
            environment: String::new(),
            infrastructure_type: "custom".to_string(),
            metric: StandardMetric::CpuUtilization, // Placeholder
            time_range: params.time_range.clone(),
            period_seconds: params.period_seconds,
        };

        let data_points = self.execute_metric_query(query, &metric_params).await?;

        Ok(MetricResult {
            metric_name: params.name.clone(),
            unit: "value".to_string(),
            data_points,
        })
    }

    async fn health_check(&self) -> Result<()> {
        // Simple health check - list metrics to verify connectivity
        self.client
            .list_metrics()
            .send()
            .await
            .map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                    provider: "CloudWatch".to_string(),
                    message: e.to_string(),
                })
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = CloudWatchProviderConfig::new("us-east-1")
            .with_ecs("my-cluster", "my-service")
            .with_lambda("my-function");

        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.ecs_cluster, Some("my-cluster".to_string()));
        assert_eq!(config.ecs_service, Some("my-service".to_string()));
        assert_eq!(config.lambda_function, Some("my-function".to_string()));
    }
}
