use axum::{
    http::{header, Method},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::path::Path;
use tower_http::cors::{Any, CorsLayer};

use super::api::api_routes;
use super::state::AppState;
use crate::security::TlsConfig;

/// Server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tls: Option<TlsConfig>,
}

impl ServerConfig {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            tls: None,
        }
    }

    pub fn with_tls(mut self, tls_config: TlsConfig) -> Self {
        self.tls = Some(tls_config);
        self
    }

    fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    fn scheme(&self) -> &'static str {
        if self.tls.is_some() { "https" } else { "http" }
    }
}

/// Start the UI server with optional HTTPS support
pub async fn start_server(host: &str, port: u16) -> anyhow::Result<()> {
    let config = ServerConfig::new(host, port);
    start_server_with_config(config).await
}

/// Start the UI server with HTTPS
pub async fn start_server_https(
    host: &str,
    port: u16,
    cert_path: &str,
    key_path: &str,
) -> anyhow::Result<()> {
    let tls_config = TlsConfig::new(cert_path, key_path);
    let config = ServerConfig::new(host, port).with_tls(tls_config);
    start_server_with_config(config).await
}

/// Start the UI server with full configuration
pub async fn start_server_with_config(config: ServerConfig) -> anyhow::Result<()> {
    let state = AppState::new();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT]);

    let app = Router::new()
        .route("/", get(index_handler))
        .nest("/api", api_routes())
        .with_state(state)
        .layer(cors);

    let addr: SocketAddr = config.address().parse()?;

    if let Some(tls_config) = &config.tls {
        start_https_server(app, addr, tls_config).await
    } else {
        start_http_server(app, addr).await
    }
}

async fn start_http_server(app: Router, addr: SocketAddr) -> anyhow::Result<()> {
    tracing::info!("Starting UI server at http://{}", addr);
    tracing::info!("API available at http://{}/api", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn start_https_server(
    app: Router,
    addr: SocketAddr,
    tls_config: &TlsConfig,
) -> anyhow::Result<()> {
    use axum_server::tls_rustls::RustlsConfig;

    let security_check = tls_config.validate();

    if !security_check.passed {
        for error in &security_check.errors {
            tracing::error!("TLS configuration error: {}", error);
        }
        anyhow::bail!("TLS configuration validation failed");
    }

    for warning in &security_check.warnings {
        tracing::warn!("TLS configuration warning: {}", warning);
    }

    let rustls_config = RustlsConfig::from_pem_file(&tls_config.cert_path, &tls_config.key_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load TLS certificates: {}", e))?;

    tracing::info!("Starting UI server at https://{}", addr);
    tracing::info!("API available at https://{}/api", addr);
    tracing::info!("TLS certificate: {}", tls_config.cert_path);

    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../../static/index.html"))
}

/// Generate self-signed certificate for development
pub fn generate_dev_certificate(
    output_dir: &Path,
) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let cert_path = output_dir.join("dev-cert.pem");
    let key_path = output_dir.join("dev-key.pem");

    if cert_path.exists() && key_path.exists() {
        tracing::info!("Using existing development certificates");
        return Ok((cert_path, key_path));
    }

    tracing::info!(
        "Generating self-signed development certificate in {:?}",
        output_dir
    );
    tracing::warn!("Self-signed certificates should only be used for development!");

    use std::process::Command;

    let output = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:4096",
            "-keyout",
            key_path.to_str().unwrap(),
            "-out",
            cert_path.to_str().unwrap(),
            "-days",
            "365",
            "-nodes",
            "-subj",
            "/CN=localhost",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            tracing::info!("Development certificates generated successfully");
            Ok((cert_path, key_path))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("OpenSSL failed: {}", stderr)
        }
        Err(e) => {
            anyhow::bail!(
                "Failed to run openssl. Ensure openssl is installed: {}",
                e
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_creation() {
        let state = AppState::new();
        let projects = state.get_projects().await;
        assert!(projects.is_ok());
    }

    #[test]
    fn test_server_config_new() {
        let config = ServerConfig::new("0.0.0.0", 8080);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.tls.is_none());
    }

    #[test]
    fn test_server_config_with_tls() {
        let tls = TlsConfig::new("/path/to/cert.pem", "/path/to/key.pem");
        let config = ServerConfig::new("0.0.0.0", 443).with_tls(tls);

        assert!(config.tls.is_some());
        assert_eq!(config.scheme(), "https");
    }

    #[test]
    fn test_server_config_address() {
        let config = ServerConfig::new("127.0.0.1", 3000);
        assert_eq!(config.address(), "127.0.0.1:3000");
    }

    #[test]
    fn test_server_config_scheme() {
        let http_config = ServerConfig::new("localhost", 8080);
        assert_eq!(http_config.scheme(), "http");

        let tls = TlsConfig::new("/cert.pem", "/key.pem");
        let https_config = ServerConfig::new("localhost", 443).with_tls(tls);
        assert_eq!(https_config.scheme(), "https");
    }
}
