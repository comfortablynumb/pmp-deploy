//! Core metrics provider trait and types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

use crate::error::Result;

/// Standard metrics that all providers should support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardMetric {
    CpuUtilization,
    MemoryUtilization,
    ErrorRate,
    RequestLatency,
    RequestCount,
}

impl StandardMetric {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::CpuUtilization => "CPU Utilization",
            Self::MemoryUtilization => "Memory Utilization",
            Self::ErrorRate => "Error Rate",
            Self::RequestLatency => "Request Latency",
            Self::RequestCount => "Request Count",
        }
    }

    pub fn default_unit(&self) -> &'static str {
        match self {
            Self::CpuUtilization => "percent",
            Self::MemoryUtilization => "percent",
            Self::ErrorRate => "percent",
            Self::RequestLatency => "ms",
            Self::RequestCount => "count",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::CpuUtilization,
            Self::MemoryUtilization,
            Self::ErrorRate,
            Self::RequestLatency,
            Self::RequestCount,
        ]
    }
}

/// Time range for metric queries.
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: SystemTime,
    pub end: SystemTime,
}

impl TimeRange {
    pub fn last(duration: Duration) -> Self {
        let end = SystemTime::now();
        let start = end - duration;
        Self { start, end }
    }

    pub fn last_minutes(minutes: u64) -> Self {
        Self::last(Duration::from_secs(minutes * 60))
    }

    pub fn last_hours(hours: u64) -> Self {
        Self::last(Duration::from_secs(hours * 3600))
    }
}

/// Parameters for querying a standard metric.
#[derive(Debug, Clone)]
pub struct MetricQueryParams {
    pub environment: String,
    pub infrastructure_type: String,
    pub metric: StandardMetric,
    pub time_range: TimeRange,
    pub period_seconds: u32,
}

impl MetricQueryParams {
    pub fn new(environment: &str, infrastructure_type: &str, metric: StandardMetric) -> Self {
        Self {
            environment: environment.to_string(),
            infrastructure_type: infrastructure_type.to_string(),
            metric,
            time_range: TimeRange::last_minutes(30),
            period_seconds: 60,
        }
    }

    pub fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = time_range;
        self
    }

    pub fn with_period(mut self, period_seconds: u32) -> Self {
        self.period_seconds = period_seconds;
        self
    }
}

/// Parameters for querying a custom metric.
#[derive(Debug, Clone)]
pub struct CustomMetricQueryParams {
    pub name: String,
    pub query: String,
    pub time_range: TimeRange,
    pub period_seconds: u32,
}

/// A single data point in a metric time series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub timestamp: u64,
    pub value: f64,
}

/// Result of a metric query containing time series data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub metric_name: String,
    pub unit: String,
    pub data_points: Vec<DataPoint>,
}

impl MetricResult {
    pub fn empty(metric_name: &str, unit: &str) -> Self {
        Self {
            metric_name: metric_name.to_string(),
            unit: unit.to_string(),
            data_points: Vec::new(),
        }
    }

    pub fn latest_value(&self) -> Option<f64> {
        self.data_points.last().map(|dp| dp.value)
    }

    pub fn average(&self) -> Option<f64> {
        if self.data_points.is_empty() {
            return None;
        }

        let sum: f64 = self.data_points.iter().map(|dp| dp.value).sum();
        Some(sum / self.data_points.len() as f64)
    }
}

/// Threshold configuration for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThresholds {
    pub warning: f64,
    pub critical: f64,
}

/// Status of a gauge based on thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GaugeStatus {
    Normal,
    Warning,
    Critical,
    Unknown,
}

impl GaugeStatus {
    pub fn from_value(value: f64, thresholds: Option<&MetricThresholds>) -> Self {
        match thresholds {
            Some(t) => {
                if value >= t.critical {
                    Self::Critical
                } else if value >= t.warning {
                    Self::Warning
                } else {
                    Self::Normal
                }
            }
            None => Self::Unknown,
        }
    }
}

/// Current value gauge for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricGauge {
    pub metric_name: String,
    pub display_name: String,
    pub current_value: f64,
    pub unit: String,
    pub status: GaugeStatus,
    pub thresholds: Option<MetricThresholds>,
}

impl MetricGauge {
    pub fn new(name: &str, display_name: &str, value: f64, unit: &str) -> Self {
        Self {
            metric_name: name.to_string(),
            display_name: display_name.to_string(),
            current_value: value,
            unit: unit.to_string(),
            status: GaugeStatus::Unknown,
            thresholds: None,
        }
    }

    pub fn with_thresholds(mut self, thresholds: MetricThresholds) -> Self {
        self.status = GaugeStatus::from_value(self.current_value, Some(&thresholds));
        self.thresholds = Some(thresholds);
        self
    }
}

/// Trait for metrics providers (CloudWatch, Prometheus, etc.).
#[async_trait]
pub trait MetricsProvider: Send + Sync {
    /// Returns the provider name.
    fn name(&self) -> &str;

    /// Returns the infrastructure types this provider supports.
    fn supported_infrastructure_types(&self) -> Vec<&str>;

    /// Query a standard metric and return time series data.
    async fn query_metric(&self, params: &MetricQueryParams) -> Result<MetricResult>;

    /// Get the current value of a metric as a gauge.
    async fn get_current_value(&self, params: &MetricQueryParams) -> Result<MetricGauge>;

    /// Query a custom metric using provider-specific query syntax.
    async fn query_custom(&self, params: &CustomMetricQueryParams) -> Result<MetricResult>;

    /// Check if the provider is healthy and can connect.
    async fn health_check(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_metric_display() {
        assert_eq!(StandardMetric::CpuUtilization.display_name(), "CPU Utilization");
        assert_eq!(StandardMetric::MemoryUtilization.default_unit(), "percent");
        assert_eq!(StandardMetric::RequestLatency.default_unit(), "ms");
    }

    #[test]
    fn test_time_range_last_minutes() {
        let range = TimeRange::last_minutes(30);
        let duration = range
            .end
            .duration_since(range.start)
            .expect("End should be after start");
        assert_eq!(duration.as_secs(), 30 * 60);
    }

    #[test]
    fn test_gauge_status_from_value() {
        let thresholds = MetricThresholds {
            warning: 70.0,
            critical: 90.0,
        };

        assert_eq!(
            GaugeStatus::from_value(50.0, Some(&thresholds)),
            GaugeStatus::Normal
        );
        assert_eq!(
            GaugeStatus::from_value(75.0, Some(&thresholds)),
            GaugeStatus::Warning
        );
        assert_eq!(
            GaugeStatus::from_value(95.0, Some(&thresholds)),
            GaugeStatus::Critical
        );
        assert_eq!(GaugeStatus::from_value(50.0, None), GaugeStatus::Unknown);
    }

    #[test]
    fn test_metric_result_average() {
        let result = MetricResult {
            metric_name: "test".to_string(),
            unit: "percent".to_string(),
            data_points: vec![
                DataPoint {
                    timestamp: 1000,
                    value: 10.0,
                },
                DataPoint {
                    timestamp: 2000,
                    value: 20.0,
                },
                DataPoint {
                    timestamp: 3000,
                    value: 30.0,
                },
            ],
        };

        assert_eq!(result.average(), Some(20.0));
        assert_eq!(result.latest_value(), Some(30.0));

        let empty_result = MetricResult::empty("test", "percent");
        assert_eq!(empty_result.average(), None);
    }
}
