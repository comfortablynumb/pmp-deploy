//! Metrics resolver for managing multiple metrics providers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};

use super::provider::{
    CustomMetricQueryParams, MetricGauge, MetricQueryParams, MetricResult, MetricsProvider,
};

/// Registry and resolver for metrics providers.
pub struct MetricsResolver {
    providers: HashMap<String, Arc<dyn MetricsProvider>>,
    infrastructure_mapping: HashMap<String, String>,
}

impl MetricsResolver {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            infrastructure_mapping: HashMap::new(),
        }
    }

    /// Register a metrics provider.
    pub fn register_provider(&mut self, provider: Arc<dyn MetricsProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// Map an infrastructure type to a provider name.
    pub fn map_infrastructure(&mut self, infrastructure_type: &str, provider_name: &str) {
        self.infrastructure_mapping
            .insert(infrastructure_type.to_string(), provider_name.to_string());
    }

    /// Set the infrastructure mapping from a HashMap.
    pub fn set_infrastructure_mapping(&mut self, mapping: HashMap<String, String>) {
        self.infrastructure_mapping = mapping;
    }

    /// Get the provider for a given infrastructure type.
    pub fn get_provider_for_infrastructure(
        &self,
        infrastructure_type: &str,
    ) -> Option<Arc<dyn MetricsProvider>> {
        let provider_name = self.infrastructure_mapping.get(infrastructure_type)?;
        self.providers.get(provider_name).cloned()
    }

    /// Get a provider by name.
    pub fn get_provider(&self, name: &str) -> Option<Arc<dyn MetricsProvider>> {
        self.providers.get(name).cloned()
    }

    /// List all registered provider names.
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Query a metric, automatically selecting the appropriate provider.
    pub async fn query_metric(&self, params: &MetricQueryParams) -> Result<MetricResult> {
        use crate::error::ConfigError;

        let provider = self
            .get_provider_for_infrastructure(&params.infrastructure_type)
            .ok_or_else(|| {
                Error::Config(ConfigError::InvalidValue {
                    field: "infrastructure_type".to_string(),
                    value: params.infrastructure_type.clone(),
                    expected: "a configured metrics provider".to_string(),
                })
            })?;

        provider.query_metric(params).await
    }

    /// Get current gauge value, automatically selecting the appropriate provider.
    pub async fn get_current_value(&self, params: &MetricQueryParams) -> Result<MetricGauge> {
        use crate::error::ConfigError;

        let provider = self
            .get_provider_for_infrastructure(&params.infrastructure_type)
            .ok_or_else(|| {
                Error::Config(ConfigError::InvalidValue {
                    field: "infrastructure_type".to_string(),
                    value: params.infrastructure_type.clone(),
                    expected: "a configured metrics provider".to_string(),
                })
            })?;

        provider.get_current_value(params).await
    }

    /// Query a custom metric using a specific provider.
    pub async fn query_custom(
        &self,
        provider_name: &str,
        params: &CustomMetricQueryParams,
    ) -> Result<MetricResult> {
        use crate::error::ConfigError;

        let provider = self.get_provider(provider_name).ok_or_else(|| {
            Error::Config(ConfigError::InvalidValue {
                field: "provider".to_string(),
                value: provider_name.to_string(),
                expected: "a registered metrics provider".to_string(),
            })
        })?;

        provider.query_custom(params).await
    }

    /// Check health of all registered providers.
    pub async fn health_check_all(&self) -> HashMap<String, Result<()>> {
        let mut results = HashMap::new();

        for (name, provider) in &self.providers {
            results.insert(name.clone(), provider.health_check().await);
        }

        results
    }
}

impl Default for MetricsResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockProvider {
        name: String,
    }

    #[async_trait]
    impl MetricsProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn supported_infrastructure_types(&self) -> Vec<&str> {
            vec!["aws-ecs", "aws-eks"]
        }

        async fn query_metric(&self, params: &MetricQueryParams) -> Result<MetricResult> {
            Ok(MetricResult {
                metric_name: params.metric.display_name().to_string(),
                unit: params.metric.default_unit().to_string(),
                data_points: vec![],
            })
        }

        async fn get_current_value(&self, params: &MetricQueryParams) -> Result<MetricGauge> {
            Ok(MetricGauge::new(
                params.metric.display_name(),
                params.metric.display_name(),
                50.0,
                params.metric.default_unit(),
            ))
        }

        async fn query_custom(&self, params: &CustomMetricQueryParams) -> Result<MetricResult> {
            Ok(MetricResult {
                metric_name: params.name.clone(),
                unit: "count".to_string(),
                data_points: vec![],
            })
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_resolver_registration() {
        let mut resolver = MetricsResolver::new();

        let provider = Arc::new(MockProvider {
            name: "cloudwatch".to_string(),
        });
        resolver.register_provider(provider);

        assert!(resolver.get_provider("cloudwatch").is_some());
        assert!(resolver.get_provider("unknown").is_none());
    }

    #[tokio::test]
    async fn test_infrastructure_mapping() {
        let mut resolver = MetricsResolver::new();

        let provider = Arc::new(MockProvider {
            name: "cloudwatch".to_string(),
        });
        resolver.register_provider(provider);
        resolver.map_infrastructure("aws-ecs", "cloudwatch");

        let mapped = resolver.get_provider_for_infrastructure("aws-ecs");
        assert!(mapped.is_some());
        assert_eq!(mapped.unwrap().name(), "cloudwatch");

        assert!(resolver
            .get_provider_for_infrastructure("kubernetes")
            .is_none());
    }

    #[tokio::test]
    async fn test_query_through_resolver() {
        use super::super::provider::StandardMetric;

        let mut resolver = MetricsResolver::new();

        let provider = Arc::new(MockProvider {
            name: "cloudwatch".to_string(),
        });
        resolver.register_provider(provider);
        resolver.map_infrastructure("aws-ecs", "cloudwatch");

        let params = MetricQueryParams::new("production", "aws-ecs", StandardMetric::CpuUtilization);
        let result = resolver.query_metric(&params).await;

        assert!(result.is_ok());
    }
}
