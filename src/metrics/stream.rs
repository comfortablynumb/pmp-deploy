//! Server-Sent Events (SSE) streaming for metrics.
//!
//! This module provides helpers for streaming real-time metric updates to clients.

use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use super::provider::{MetricGauge, MetricResult, StandardMetric};

/// Event types sent via SSE.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum MetricEvent {
    /// Time series data update for a metric.
    TimeSeries(MetricResult),

    /// Current gauge value update.
    Gauge(MetricGauge),

    /// All gauges update (batch).
    AllGauges(Vec<MetricGauge>),

    /// Error during metric collection.
    Error { metric: String, message: String },

    /// Heartbeat to keep connection alive.
    Heartbeat,
}

impl MetricEvent {
    pub fn to_sse_data(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Configuration for the metric stream.
#[derive(Debug, Clone)]
pub struct MetricStreamConfig {
    /// Interval between gauge updates.
    pub gauge_interval: Duration,

    /// Interval between time series updates.
    pub timeseries_interval: Duration,

    /// Heartbeat interval to keep connection alive.
    pub heartbeat_interval: Duration,

    /// Metrics to stream.
    pub metrics: Vec<StandardMetric>,
}

impl Default for MetricStreamConfig {
    fn default() -> Self {
        Self {
            gauge_interval: Duration::from_secs(5),
            timeseries_interval: Duration::from_secs(30),
            heartbeat_interval: Duration::from_secs(15),
            metrics: StandardMetric::all(),
        }
    }
}

/// Broadcaster for metric events.
pub struct MetricBroadcaster {
    sender: broadcast::Sender<MetricEvent>,
}

impl MetricBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Subscribe to metric events.
    pub fn subscribe(&self) -> broadcast::Receiver<MetricEvent> {
        self.sender.subscribe()
    }

    /// Send a metric event to all subscribers.
    pub fn send(&self, event: MetricEvent) -> Result<usize, broadcast::error::SendError<MetricEvent>> {
        self.sender.send(event)
    }

    /// Send a time series update.
    pub fn send_timeseries(&self, result: MetricResult) {
        let _ = self.send(MetricEvent::TimeSeries(result));
    }

    /// Send a gauge update.
    pub fn send_gauge(&self, gauge: MetricGauge) {
        let _ = self.send(MetricEvent::Gauge(gauge));
    }

    /// Send all gauges as a batch.
    pub fn send_all_gauges(&self, gauges: Vec<MetricGauge>) {
        let _ = self.send(MetricEvent::AllGauges(gauges));
    }

    /// Send an error event.
    pub fn send_error(&self, metric: &str, message: &str) {
        let _ = self.send(MetricEvent::Error {
            metric: metric.to_string(),
            message: message.to_string(),
        });
    }

    /// Send a heartbeat.
    pub fn send_heartbeat(&self) {
        let _ = self.send(MetricEvent::Heartbeat);
    }

    /// Get the number of active subscribers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for MetricBroadcaster {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Stream handle for managing metric streaming.
pub struct MetricStreamHandle {
    broadcaster: Arc<MetricBroadcaster>,
    shutdown: tokio::sync::watch::Sender<bool>,
}

impl MetricStreamHandle {
    pub fn new(broadcaster: Arc<MetricBroadcaster>) -> Self {
        let (shutdown, _) = tokio::sync::watch::channel(false);
        Self {
            broadcaster,
            shutdown,
        }
    }

    pub fn broadcaster(&self) -> Arc<MetricBroadcaster> {
        self.broadcaster.clone()
    }

    pub fn shutdown_receiver(&self) -> tokio::sync::watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_event_serialization() {
        let gauge = MetricGauge::new("cpu", "CPU Utilization", 75.5, "percent");
        let event = MetricEvent::Gauge(gauge);

        let json = event.to_sse_data();
        assert!(json.contains("Gauge"));
        assert!(json.contains("75.5"));
    }

    #[tokio::test]
    async fn test_broadcaster_subscribe() {
        let broadcaster = MetricBroadcaster::new(10);

        let mut rx = broadcaster.subscribe();

        broadcaster.send_heartbeat();

        let event = rx.recv().await.unwrap();
        matches!(event, MetricEvent::Heartbeat);
    }

    #[test]
    fn test_default_stream_config() {
        let config = MetricStreamConfig::default();

        assert_eq!(config.gauge_interval, Duration::from_secs(5));
        assert_eq!(config.metrics.len(), 5);
    }
}
