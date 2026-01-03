//! Metrics observation module for deployment monitoring.
//!
//! This module provides infrastructure for querying and streaming metrics
//! from various providers (CloudWatch, Prometheus) during deployments.

mod provider;
mod resolver;

pub mod cloudwatch;
pub mod prometheus;
pub mod stream;

pub use provider::{
    CustomMetricQueryParams, DataPoint, GaugeStatus, MetricGauge, MetricQueryParams,
    MetricResult, MetricThresholds, MetricsProvider, StandardMetric, TimeRange,
};
pub use resolver::MetricsResolver;
