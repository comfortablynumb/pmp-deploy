use std::fmt;
use std::path::PathBuf;

/// Comprehensive error types for pmp-deploy
#[derive(Debug)]
pub enum Error {
    /// Configuration-related errors
    Config(ConfigError),

    /// Infrastructure provider errors
    Infrastructure(InfrastructureError),

    /// Deployment strategy errors
    Deployment(DeploymentError),

    /// Plugin system errors
    Plugin(PluginError),

    /// Secrets management errors
    Secrets(SecretsError),

    /// IO errors
    Io(IoError),

    /// Network errors
    Network(NetworkError),

    /// Authentication/Authorization errors
    Auth(AuthError),
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound { path: PathBuf },
    ParseError { path: PathBuf, message: String },
    ValidationError { errors: Vec<String> },
    MissingField { field: String, context: String },
    InvalidValue { field: String, value: String, expected: String },
    EnvironmentNotFound { name: String },
    InfrastructureNotFound { name: String },
    PermissionDenied { path: PathBuf },
}

#[derive(Debug)]
pub enum InfrastructureError {
    ConnectionFailed { provider: String, message: String },
    AuthenticationFailed { provider: String, message: String },
    ResourceNotFound { resource_type: String, name: String },
    QuotaExceeded { resource: String, limit: String },
    Timeout { operation: String, duration_secs: u64 },
    ProviderError { provider: String, message: String },
    UnsupportedOperation { provider: String, operation: String },
}

#[derive(Debug)]
pub enum DeploymentError {
    StrategyFailed { strategy: String, message: String },
    HealthCheckFailed { environment: String, message: String },
    RollbackFailed { environment: String, message: String },
    ImageNotFound { image: String },
    ReplicasFailed { expected: u32, actual: u32 },
    Interrupted { environment: String, state: String },
    AlreadyInProgress { environment: String },
    HookFailed { hook_name: String, message: String },
}

#[derive(Debug)]
pub enum PluginError {
    LoadFailed { path: PathBuf, message: String },
    VersionMismatch { expected: u32, actual: u32 },
    InitFailed { plugin: String, message: String },
    NotFound { infrastructure_type: String },
    AlreadyRegistered { infrastructure_type: String },
}

#[derive(Debug)]
pub enum SecretsError {
    ProviderNotFound { provider: String },
    SecretNotFound { key: String, provider: String },
    AccessDenied { key: String, message: String },
    DecryptionFailed { key: String },
    ConnectionFailed { provider: String, message: String },
}

#[derive(Debug)]
pub enum IoError {
    FileNotFound { path: PathBuf },
    ReadFailed { path: PathBuf, message: String },
    WriteFailed { path: PathBuf, message: String },
    PermissionDenied { path: PathBuf },
    DirectoryNotFound { path: PathBuf },
}

#[derive(Debug)]
pub enum NetworkError {
    ConnectionRefused { host: String, port: u16 },
    Timeout { host: String, duration_secs: u64 },
    DnsResolutionFailed { host: String },
    TlsError { host: String, message: String },
    HttpError { status: u16, message: String },
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials { provider: String },
    TokenExpired { provider: String },
    MissingCredentials { provider: String, required: Vec<String> },
    InsufficientPermissions { operation: String, required: String },
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config(e) => write!(f, "Configuration error: {}", e),
            Error::Infrastructure(e) => write!(f, "Infrastructure error: {}", e),
            Error::Deployment(e) => write!(f, "Deployment error: {}", e),
            Error::Plugin(e) => write!(f, "Plugin error: {}", e),
            Error::Secrets(e) => write!(f, "Secrets error: {}", e),
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Network(e) => write!(f, "Network error: {}", e),
            Error::Auth(e) => write!(f, "Authentication error: {}", e),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NotFound { path } => {
                write!(f, "Configuration file not found: {}", path.display())
            }
            ConfigError::ParseError { path, message } => {
                write!(f, "Failed to parse {}: {}", path.display(), message)
            }
            ConfigError::ValidationError { errors } => {
                write!(f, "Validation failed:\n  - {}", errors.join("\n  - "))
            }
            ConfigError::MissingField { field, context } => {
                write!(f, "Missing required field '{}' in {}", field, context)
            }
            ConfigError::InvalidValue { field, value, expected } => {
                write!(f, "Invalid value '{}' for '{}'. Expected: {}", value, field, expected)
            }
            ConfigError::EnvironmentNotFound { name } => {
                write!(f, "Environment '{}' not found. Use 'pmp-deploy list' to see available environments", name)
            }
            ConfigError::InfrastructureNotFound { name } => {
                write!(f, "Infrastructure '{}' not found in configuration", name)
            }
            ConfigError::PermissionDenied { path } => {
                write!(f, "Permission denied reading config file: {}", path.display())
            }
        }
    }
}

impl fmt::Display for InfrastructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InfrastructureError::ConnectionFailed { provider, message } => {
                write!(f, "Failed to connect to {}: {}", provider, message)
            }
            InfrastructureError::AuthenticationFailed { provider, message } => {
                write!(f, "Authentication failed for {}: {}", provider, message)
            }
            InfrastructureError::ResourceNotFound { resource_type, name } => {
                write!(f, "{} '{}' not found", resource_type, name)
            }
            InfrastructureError::QuotaExceeded { resource, limit } => {
                write!(f, "Quota exceeded for {}: limit is {}", resource, limit)
            }
            InfrastructureError::Timeout { operation, duration_secs } => {
                write!(f, "Operation '{}' timed out after {}s", operation, duration_secs)
            }
            InfrastructureError::ProviderError { provider, message } => {
                write!(f, "{} error: {}", provider, message)
            }
            InfrastructureError::UnsupportedOperation { provider, operation } => {
                write!(f, "Operation '{}' is not supported by {}", operation, provider)
            }
        }
    }
}

impl fmt::Display for DeploymentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeploymentError::StrategyFailed { strategy, message } => {
                write!(f, "{} deployment failed: {}", strategy, message)
            }
            DeploymentError::HealthCheckFailed { environment, message } => {
                write!(f, "Health check failed for '{}': {}", environment, message)
            }
            DeploymentError::RollbackFailed { environment, message } => {
                write!(f, "Rollback failed for '{}': {}", environment, message)
            }
            DeploymentError::ImageNotFound { image } => {
                write!(f, "Image '{}' not found. Verify the image exists and you have access", image)
            }
            DeploymentError::ReplicasFailed { expected, actual } => {
                write!(f, "Expected {} replicas but only {} are healthy", expected, actual)
            }
            DeploymentError::Interrupted { environment, state } => {
                write!(f, "Deployment to '{}' was interrupted. State: {}", environment, state)
            }
            DeploymentError::AlreadyInProgress { environment } => {
                write!(f, "A deployment to '{}' is already in progress", environment)
            }
            DeploymentError::HookFailed { hook_name, message } => {
                write!(f, "Hook '{}' failed: {}", hook_name, message)
            }
        }
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::LoadFailed { path, message } => {
                write!(f, "Failed to load plugin {}: {}", path.display(), message)
            }
            PluginError::VersionMismatch { expected, actual } => {
                write!(f, "Plugin API version mismatch: expected {}, got {}", expected, actual)
            }
            PluginError::InitFailed { plugin, message } => {
                write!(f, "Plugin '{}' initialization failed: {}", plugin, message)
            }
            PluginError::NotFound { infrastructure_type } => {
                write!(f, "No plugin found for infrastructure type '{}'", infrastructure_type)
            }
            PluginError::AlreadyRegistered { infrastructure_type } => {
                write!(f, "Plugin for '{}' is already registered", infrastructure_type)
            }
        }
    }
}

impl fmt::Display for SecretsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretsError::ProviderNotFound { provider } => {
                write!(f, "Secrets provider '{}' not found", provider)
            }
            SecretsError::SecretNotFound { key, provider } => {
                write!(f, "Secret '{}' not found in {}", key, provider)
            }
            SecretsError::AccessDenied { key, message } => {
                write!(f, "Access denied to secret '{}': {}", key, message)
            }
            SecretsError::DecryptionFailed { key } => {
                write!(f, "Failed to decrypt secret '{}'", key)
            }
            SecretsError::ConnectionFailed { provider, message } => {
                write!(f, "Failed to connect to secrets provider '{}': {}", provider, message)
            }
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::FileNotFound { path } => {
                write!(f, "File not found: {}", path.display())
            }
            IoError::ReadFailed { path, message } => {
                write!(f, "Failed to read {}: {}", path.display(), message)
            }
            IoError::WriteFailed { path, message } => {
                write!(f, "Failed to write {}: {}", path.display(), message)
            }
            IoError::PermissionDenied { path } => {
                write!(f, "Permission denied: {}", path.display())
            }
            IoError::DirectoryNotFound { path } => {
                write!(f, "Directory not found: {}", path.display())
            }
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::ConnectionRefused { host, port } => {
                write!(f, "Connection refused to {}:{}", host, port)
            }
            NetworkError::Timeout { host, duration_secs } => {
                write!(f, "Connection to {} timed out after {}s", host, duration_secs)
            }
            NetworkError::DnsResolutionFailed { host } => {
                write!(f, "Failed to resolve hostname: {}", host)
            }
            NetworkError::TlsError { host, message } => {
                write!(f, "TLS error connecting to {}: {}", host, message)
            }
            NetworkError::HttpError { status, message } => {
                write!(f, "HTTP error {}: {}", status, message)
            }
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::InvalidCredentials { provider } => {
                write!(f, "Invalid credentials for {}", provider)
            }
            AuthError::TokenExpired { provider } => {
                write!(f, "Authentication token expired for {}. Please re-authenticate", provider)
            }
            AuthError::MissingCredentials { provider, required } => {
                write!(f, "Missing credentials for {}. Required: {}", provider, required.join(", "))
            }
            AuthError::InsufficientPermissions { operation, required } => {
                write!(f, "Insufficient permissions for '{}'. Required: {}", operation, required)
            }
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(IoError::ReadFailed {
            path: PathBuf::from("<unknown>"),
            message: err.to_string(),
        })
    }
}

impl From<ConfigError> for Error {
    fn from(err: ConfigError) -> Self {
        Error::Config(err)
    }
}

impl From<InfrastructureError> for Error {
    fn from(err: InfrastructureError) -> Self {
        Error::Infrastructure(err)
    }
}

impl From<DeploymentError> for Error {
    fn from(err: DeploymentError) -> Self {
        Error::Deployment(err)
    }
}

impl From<PluginError> for Error {
    fn from(err: PluginError) -> Self {
        Error::Plugin(err)
    }
}

impl From<SecretsError> for Error {
    fn from(err: SecretsError) -> Self {
        Error::Secrets(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::NotFound {
            path: PathBuf::from("/path/to/config.yaml"),
        };
        assert!(err.to_string().contains("not found"));

        let err = ConfigError::EnvironmentNotFound {
            name: "production".to_string(),
        };
        assert!(err.to_string().contains("production"));
    }

    #[test]
    fn test_deployment_error_display() {
        let err = DeploymentError::HealthCheckFailed {
            environment: "staging".to_string(),
            message: "timeout".to_string(),
        };
        assert!(err.to_string().contains("staging"));
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn test_error_conversion() {
        let config_err = ConfigError::NotFound {
            path: PathBuf::from("/test"),
        };
        let err: Error = config_err.into();

        assert!(matches!(err, Error::Config(_)));
    }
}
