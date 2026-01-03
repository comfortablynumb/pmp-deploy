use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Whether to add jitter to delays
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    pub fn with_jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.initial_delay.as_millis() as f64
            * self.backoff_multiplier.powi(attempt as i32 - 1);

        let delay_ms = base_delay.min(self.max_delay.as_millis() as f64);

        let final_delay = if self.jitter {
            let jitter_factor = 0.5 + (rand_simple() * 0.5);
            delay_ms * jitter_factor
        } else {
            delay_ms
        };

        Duration::from_millis(final_delay as u64)
    }
}

/// Simple pseudo-random number generator (0.0 to 1.0)
fn rand_simple() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    (nanos as f64 % 1000.0) / 1000.0
}

/// Result of a retry operation
#[derive(Debug)]
pub struct RetryResult<T, E> {
    pub result: Result<T, E>,
    pub attempts: u32,
    pub total_delay: Duration,
}

/// Retry an async operation with exponential backoff
pub async fn retry<F, Fut, T, E>(
    config: &RetryConfig,
    operation: F,
) -> RetryResult<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempts = 0;
    let mut total_delay = Duration::ZERO;
    let mut last_error: Option<E> = None;

    while attempts < config.max_attempts {
        attempts += 1;

        match operation().await {
            Ok(result) => {
                return RetryResult {
                    result: Ok(result),
                    attempts,
                    total_delay,
                };
            }
            Err(e) => {
                tracing::warn!(
                    "Attempt {}/{} failed: {}",
                    attempts,
                    config.max_attempts,
                    e
                );
                last_error = Some(e);

                if attempts < config.max_attempts {
                    let delay = config.calculate_delay(attempts);
                    tracing::debug!("Retrying in {:?}", delay);
                    total_delay += delay;
                    sleep(delay).await;
                }
            }
        }
    }

    RetryResult {
        result: Err(last_error.expect("At least one attempt should have been made")),
        attempts,
        total_delay,
    }
}

/// Retry an async operation with a custom retry condition
pub async fn retry_if<F, Fut, T, E, C>(
    config: &RetryConfig,
    operation: F,
    should_retry: C,
) -> RetryResult<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    C: Fn(&E) -> bool,
{
    let mut attempts = 0;
    let mut total_delay = Duration::ZERO;
    let mut last_error: Option<E> = None;

    while attempts < config.max_attempts {
        attempts += 1;

        match operation().await {
            Ok(result) => {
                return RetryResult {
                    result: Ok(result),
                    attempts,
                    total_delay,
                };
            }
            Err(e) => {
                let should_continue = should_retry(&e);

                tracing::warn!(
                    "Attempt {}/{} failed: {}{}",
                    attempts,
                    config.max_attempts,
                    e,
                    if !should_continue { " (not retryable)" } else { "" }
                );

                if !should_continue {
                    return RetryResult {
                        result: Err(e),
                        attempts,
                        total_delay,
                    };
                }

                last_error = Some(e);

                if attempts < config.max_attempts {
                    let delay = config.calculate_delay(attempts);
                    tracing::debug!("Retrying in {:?}", delay);
                    total_delay += delay;
                    sleep(delay).await;
                }
            }
        }
    }

    RetryResult {
        result: Err(last_error.expect("At least one attempt should have been made")),
        attempts,
        total_delay,
    }
}

/// Presets for common retry scenarios
pub mod presets {
    use super::*;

    /// Quick retries for local operations
    pub fn local() -> RetryConfig {
        RetryConfig::new()
            .with_max_attempts(3)
            .with_initial_delay(Duration::from_millis(50))
            .with_max_delay(Duration::from_millis(500))
    }

    /// Standard retries for API calls
    pub fn api() -> RetryConfig {
        RetryConfig::new()
            .with_max_attempts(5)
            .with_initial_delay(Duration::from_millis(200))
            .with_max_delay(Duration::from_secs(10))
    }

    /// Aggressive retries for critical operations
    pub fn critical() -> RetryConfig {
        RetryConfig::new()
            .with_max_attempts(10)
            .with_initial_delay(Duration::from_millis(500))
            .with_max_delay(Duration::from_secs(60))
    }

    /// Patient retries for long-running operations
    pub fn patient() -> RetryConfig {
        RetryConfig::new()
            .with_max_attempts(20)
            .with_initial_delay(Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(120))
            .with_backoff_multiplier(1.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_attempts(5)
            .with_initial_delay(Duration::from_secs(1))
            .with_jitter(false);

        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert!(!config.jitter);
    }

    #[test]
    fn test_calculate_delay_no_jitter() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_millis(100))
            .with_backoff_multiplier(2.0)
            .with_jitter(false);

        let delay1 = config.calculate_delay(1);
        let delay2 = config.calculate_delay(2);
        let delay3 = config.calculate_delay(3);

        assert_eq!(delay1, Duration::from_millis(100));
        assert_eq!(delay2, Duration::from_millis(200));
        assert_eq!(delay3, Duration::from_millis(400));
    }

    #[test]
    fn test_calculate_delay_respects_max() {
        let config = RetryConfig::new()
            .with_initial_delay(Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(5))
            .with_backoff_multiplier(10.0)
            .with_jitter(false);

        let delay = config.calculate_delay(5);
        assert_eq!(delay, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig::new().with_max_attempts(3);

        let result = retry(&config, || async { Ok::<_, &str>(42) }).await;

        assert!(result.result.is_ok());
        assert_eq!(result.attempts, 1);
        assert_eq!(result.total_delay, Duration::ZERO);
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig::new()
            .with_max_attempts(5)
            .with_initial_delay(Duration::from_millis(1))
            .with_jitter(false);

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry(&config, || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);

                if count < 2 {
                    Err("temporary failure")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.result.is_ok());
        assert_eq!(result.attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_all_failures() {
        let config = RetryConfig::new()
            .with_max_attempts(3)
            .with_initial_delay(Duration::from_millis(1))
            .with_jitter(false);

        let result = retry(&config, || async { Err::<i32, _>("always fails") }).await;

        assert!(result.result.is_err());
        assert_eq!(result.attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_if_stops_on_non_retryable() {
        let config = RetryConfig::new()
            .with_max_attempts(5)
            .with_initial_delay(Duration::from_millis(1));

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_if(
            &config,
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>("non-retryable error")
                }
            },
            |_err| false,
        )
        .await;

        assert!(result.result.is_err());
        assert_eq!(result.attempts, 1);
    }

    #[test]
    fn test_presets() {
        let local = presets::local();
        assert_eq!(local.max_attempts, 3);

        let api = presets::api();
        assert_eq!(api.max_attempts, 5);

        let critical = presets::critical();
        assert_eq!(critical.max_attempts, 10);

        let patient = presets::patient();
        assert_eq!(patient.max_attempts, 20);
    }
}
