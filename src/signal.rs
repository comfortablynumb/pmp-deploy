use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

/// Global flag to track if shutdown has been requested
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Check if shutdown has been requested
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

/// Request a shutdown
pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

/// Reset shutdown flag (useful for testing)
pub fn reset_shutdown() {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
}

/// Signal handler for graceful shutdown
#[derive(Clone)]
pub struct SignalHandler {
    shutdown_tx: broadcast::Sender<()>,
}

impl SignalHandler {
    /// Create a new signal handler
    pub fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self { shutdown_tx }
    }

    /// Get a receiver for shutdown signals
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Trigger a shutdown
    pub fn shutdown(&self) {
        request_shutdown();
        let _ = self.shutdown_tx.send(());
    }

    /// Install signal handlers for Ctrl+C and SIGTERM
    pub async fn install(&self) -> anyhow::Result<()> {
        let handler = self.clone();

        tokio::spawn(async move {
            if let Err(e) = wait_for_signal().await {
                tracing::error!("Signal handler error: {}", e);
            }

            tracing::info!("Shutdown signal received");
            handler.shutdown();
        });

        Ok(())
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait for a shutdown signal (Ctrl+C or SIGTERM)
async fn wait_for_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;

        tokio::select! {
            _ = sigint.recv() => {
                tracing::debug!("Received SIGINT");
            }
            _ = sigterm.recv() => {
                tracing::debug!("Received SIGTERM");
            }
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await?;
        tracing::debug!("Received Ctrl+C");
    }

    Ok(())
}

/// Context for tracking interruptible operations
#[derive(Clone)]
pub struct InterruptibleContext {
    handler: SignalHandler,
    operation_name: String,
}

impl InterruptibleContext {
    pub fn new(handler: SignalHandler, operation_name: impl Into<String>) -> Self {
        Self {
            handler,
            operation_name: operation_name.into(),
        }
    }

    /// Check if the operation should be interrupted
    pub fn is_interrupted(&self) -> bool {
        is_shutdown_requested()
    }

    /// Get a receiver for interrupt signals
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.handler.subscribe()
    }

    /// Execute an async operation with interrupt support
    pub async fn run<F, Fut, T>(&self, operation: F) -> Result<T, InterruptError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let mut shutdown_rx = self.subscribe();

        tokio::select! {
            result = operation() => {
                Ok(result)
            }
            _ = shutdown_rx.recv() => {
                Err(InterruptError {
                    operation: self.operation_name.clone(),
                })
            }
        }
    }

    /// Check for interruption and return error if interrupted
    pub fn check(&self) -> Result<(), InterruptError> {
        if self.is_interrupted() {
            Err(InterruptError {
                operation: self.operation_name.clone(),
            })
        } else {
            Ok(())
        }
    }
}

/// Error returned when an operation is interrupted
#[derive(Debug)]
pub struct InterruptError {
    pub operation: String,
}

impl std::fmt::Display for InterruptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation '{}' was interrupted", self.operation)
    }
}

impl std::error::Error for InterruptError {}

/// Helper macro for checking interruption in loops
#[macro_export]
macro_rules! check_interrupt {
    ($ctx:expr) => {
        if $ctx.is_interrupted() {
            tracing::warn!("Operation interrupted by user");
            break;
        }
    };
    ($ctx:expr, $cleanup:expr) => {
        if $ctx.is_interrupted() {
            tracing::warn!("Operation interrupted by user, running cleanup");
            $cleanup;
            break;
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_flag() {
        reset_shutdown();
        assert!(!is_shutdown_requested());

        request_shutdown();
        assert!(is_shutdown_requested());

        reset_shutdown();
        assert!(!is_shutdown_requested());
    }

    #[test]
    fn test_signal_handler_subscribe() {
        let handler = SignalHandler::new();
        let _rx1 = handler.subscribe();
        let _rx2 = handler.subscribe();
    }

    #[tokio::test]
    async fn test_interruptible_context_success() {
        reset_shutdown();

        let handler = SignalHandler::new();
        let ctx = InterruptibleContext::new(handler, "test_operation");

        let result = ctx.run(|| async { 42 }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_interruptible_context_check() {
        reset_shutdown();

        let handler = SignalHandler::new();
        let ctx = InterruptibleContext::new(handler.clone(), "test_operation");

        assert!(ctx.check().is_ok());

        handler.shutdown();

        assert!(ctx.check().is_err());

        reset_shutdown();
    }

    #[test]
    fn test_interrupt_error_display() {
        let err = InterruptError {
            operation: "deployment".to_string(),
        };

        assert!(err.to_string().contains("deployment"));
        assert!(err.to_string().contains("interrupted"));
    }
}
