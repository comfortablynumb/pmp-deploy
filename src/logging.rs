use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

/// Logging format options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Human-readable format (default)
    Text,
    /// JSON format for machine parsing
    Json,
    /// Compact format for CI/CD
    Compact,
}

impl Default for LogFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" | "pretty" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            "compact" => Ok(LogFormat::Compact),
            _ => Err(format!("Unknown log format: {}. Use 'text', 'json', or 'compact'", s)),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log format
    pub format: LogFormat,
    /// Log level filter (e.g., "info", "debug", "pmp_deploy=debug")
    pub level: String,
    /// Whether to include timestamps
    pub timestamps: bool,
    /// Whether to include file/line information
    pub file_info: bool,
    /// Whether to include span events
    pub span_events: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Text,
            level: "info".to_string(),
            timestamps: true,
            file_info: false,
            span_events: false,
        }
    }
}

impl LogConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_level(mut self, level: impl Into<String>) -> Self {
        self.level = level.into();
        self
    }

    pub fn with_timestamps(mut self, enabled: bool) -> Self {
        self.timestamps = enabled;
        self
    }

    pub fn with_file_info(mut self, enabled: bool) -> Self {
        self.file_info = enabled;
        self
    }

    pub fn with_span_events(mut self, enabled: bool) -> Self {
        self.span_events = enabled;
        self
    }

    pub fn verbose() -> Self {
        Self {
            format: LogFormat::Text,
            level: "debug".to_string(),
            timestamps: true,
            file_info: true,
            span_events: true,
        }
    }

    pub fn quiet() -> Self {
        Self {
            format: LogFormat::Compact,
            level: "warn".to_string(),
            timestamps: false,
            file_info: false,
            span_events: false,
        }
    }

    pub fn json() -> Self {
        Self {
            format: LogFormat::Json,
            level: "info".to_string(),
            timestamps: true,
            file_info: true,
            span_events: true,
        }
    }
}

/// Initialize the logging system
pub fn init(config: &LogConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let span_events = if config.span_events {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    };

    match config.format {
        LogFormat::Text => {
            let layer = fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_file(config.file_info)
                .with_line_number(config.file_info)
                .with_span_events(span_events);

            let layer = if config.timestamps {
                layer.boxed()
            } else {
                layer.without_time().boxed()
            };

            tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;
        }
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_target(true)
                .with_file(config.file_info)
                .with_line_number(config.file_info)
                .with_span_events(span_events);

            tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;
        }
        LogFormat::Compact => {
            let layer = fmt::layer()
                .compact()
                .with_target(false)
                .with_level(true)
                .without_time()
                .with_span_events(span_events);

            tracing_subscriber::registry()
                .with(filter)
                .with(layer)
                .try_init()
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;
        }
    }

    Ok(())
}

/// Patterns that should be masked in logs
const SENSITIVE_PATTERNS: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "auth",
    "credential",
    "private_key",
    "privatekey",
    "private-key",
    "access_key",
    "accesskey",
    "access-key",
    "bearer",
    "jwt",
    "session",
    "cookie",
];

/// Mask sensitive values in a string
pub fn mask_sensitive(input: &str) -> String {
    let mut result = input.to_string();

    for pattern in SENSITIVE_PATTERNS {
        let re_patterns = vec![
            format!(r#"{}["']?\s*[:=]\s*["']?([^"'\s,}}]+)"#, pattern),
            format!(r#"{}["']?\s*[:=]\s*["']([^"']+)["']"#, pattern),
        ];

        for re_pattern in re_patterns {
            if let Ok(re) = regex::Regex::new(&re_pattern) {
                result = re
                    .replace_all(&result, |caps: &regex::Captures| {
                        let full_match = caps.get(0).unwrap().as_str();
                        let value = caps.get(1).unwrap().as_str();

                        if value.len() > 4 {
                            full_match.replace(value, &format!("{}***", &value[..2]))
                        } else {
                            full_match.replace(value, "***")
                        }
                    })
                    .to_string();
            }
        }
    }

    result
}

/// Mask a value, showing only the first few characters
pub fn mask_value(value: &str) -> String {
    if value.len() <= 4 {
        "****".to_string()
    } else {
        format!("{}****", &value[..4])
    }
}

/// Check if a key name indicates a sensitive value
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();

    SENSITIVE_PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// Wrapper for logging values that may be sensitive
pub struct SensitiveValue<'a> {
    value: &'a str,
    masked: bool,
}

impl<'a> SensitiveValue<'a> {
    pub fn new(value: &'a str, mask: bool) -> Self {
        Self { value, masked: mask }
    }

    pub fn masked(value: &'a str) -> Self {
        Self { value, masked: true }
    }

    pub fn visible(value: &'a str) -> Self {
        Self { value, masked: false }
    }
}

impl<'a> std::fmt::Display for SensitiveValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.masked {
            write!(f, "{}", mask_value(self.value))
        } else {
            write!(f, "{}", self.value)
        }
    }
}

impl<'a> std::fmt::Debug for SensitiveValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.masked {
            write!(f, "\"{}\"", mask_value(self.value))
        } else {
            write!(f, "{:?}", self.value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_format_from_str() {
        assert_eq!("text".parse::<LogFormat>().unwrap(), LogFormat::Text);
        assert_eq!("json".parse::<LogFormat>().unwrap(), LogFormat::Json);
        assert_eq!("compact".parse::<LogFormat>().unwrap(), LogFormat::Compact);
        assert!("invalid".parse::<LogFormat>().is_err());
    }

    #[test]
    fn test_log_config_builder() {
        let config = LogConfig::new()
            .with_format(LogFormat::Json)
            .with_level("debug")
            .with_timestamps(false);

        assert_eq!(config.format, LogFormat::Json);
        assert_eq!(config.level, "debug");
        assert!(!config.timestamps);
    }

    #[test]
    fn test_mask_value() {
        assert_eq!(mask_value("abc"), "****");
        assert_eq!(mask_value("abcd"), "****");
        assert_eq!(mask_value("abcde"), "abcd****");
        assert_eq!(mask_value("secret-token-12345"), "secr****");
    }

    #[test]
    fn test_is_sensitive_key() {
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("API_KEY"));
        assert!(is_sensitive_key("aws_secret_access_key"));
        assert!(is_sensitive_key("DATABASE_PASSWORD"));
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("host"));
    }

    #[test]
    fn test_mask_sensitive() {
        let input = r#"{"password": "secret123", "username": "admin"}"#;
        let masked = mask_sensitive(input);

        assert!(!masked.contains("secret123"));
        assert!(masked.contains("admin"));
    }

    #[test]
    fn test_sensitive_value_display() {
        let masked = SensitiveValue::masked("my-secret-token");
        assert_eq!(masked.to_string(), "my-s****");

        let visible = SensitiveValue::visible("my-value");
        assert_eq!(visible.to_string(), "my-value");
    }
}
