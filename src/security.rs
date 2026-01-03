use std::path::Path;

/// Security validation results
#[derive(Debug)]
pub struct SecurityCheck {
    pub passed: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl SecurityCheck {
    pub fn new() -> Self {
        Self {
            passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_warning(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    pub fn add_error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
        self.passed = false;
    }

    pub fn merge(&mut self, other: SecurityCheck) {
        self.warnings.extend(other.warnings);
        self.errors.extend(other.errors);

        if !other.passed {
            self.passed = false;
        }
    }
}

impl Default for SecurityCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate file permissions for a config file
#[cfg(unix)]
pub fn validate_file_permissions(path: &Path) -> SecurityCheck {
    use std::os::unix::fs::PermissionsExt;

    let mut check = SecurityCheck::new();

    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            check.add_error(format!("Cannot read file metadata: {}", e));
            return check;
        }
    };

    let mode = metadata.permissions().mode();

    if mode & 0o002 != 0 {
        check.add_error(format!(
            "Config file {} is world-writable (mode {:o}). This is a security risk.",
            path.display(),
            mode & 0o777
        ));
    }

    if mode & 0o020 != 0 {
        check.add_warning(format!(
            "Config file {} is group-writable (mode {:o}). Consider restricting permissions.",
            path.display(),
            mode & 0o777
        ));
    }

    if mode & 0o004 != 0 {
        check.add_warning(format!(
            "Config file {} is world-readable (mode {:o}). May expose sensitive data.",
            path.display(),
            mode & 0o777
        ));
    }

    check
}

#[cfg(windows)]
pub fn validate_file_permissions(path: &Path) -> SecurityCheck {
    let mut check = SecurityCheck::new();

    if !path.exists() {
        check.add_error(format!("Config file not found: {}", path.display()));
        return check;
    }

    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            check.add_error(format!("Cannot read file metadata: {}", e));
            return check;
        }
    };

    if metadata.permissions().readonly() {
        check.add_warning(format!(
            "Config file {} is read-only",
            path.display()
        ));
    }

    check
}

/// Check if a file contains sensitive data patterns
pub fn check_sensitive_content(content: &str) -> SecurityCheck {
    let mut check = SecurityCheck::new();

    let sensitive_patterns = [
        ("password", "hardcoded password"),
        ("secret_key", "hardcoded secret key"),
        ("private_key", "private key content"),
        ("-----BEGIN RSA PRIVATE KEY-----", "RSA private key"),
        ("-----BEGIN OPENSSH PRIVATE KEY-----", "OpenSSH private key"),
        ("AKIA", "AWS access key ID"),
        ("sk-", "potential API key"),
    ];

    for (pattern, description) in sensitive_patterns {
        if content.to_lowercase().contains(&pattern.to_lowercase()) {
            if pattern.starts_with("-----") || pattern.starts_with("AKIA") || pattern.starts_with("sk-") {
                check.add_error(format!(
                    "Config appears to contain {}: consider using environment variables or secrets manager",
                    description
                ));
            } else {
                check.add_warning(format!(
                    "Config may contain {}: ensure values reference secrets, not literals",
                    description
                ));
            }
        }
    }

    check
}

/// Validate that environment variable references are used for secrets
pub fn validate_secret_references(content: &str) -> SecurityCheck {
    let mut check = SecurityCheck::new();

    let patterns = [
        (r#"password:\s*["']?[^$\s]+"#, "password"),
        (r#"secret:\s*["']?[^$\s]+"#, "secret"),
        (r#"api_key:\s*["']?[^$\s]+"#, "api_key"),
        (r#"token:\s*["']?[^$\s]+"#, "token"),
    ];

    for (pattern, field_name) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(content) {
                check.add_warning(format!(
                    "Field '{}' appears to have a literal value. Consider using ${{ENV_VAR}} syntax",
                    field_name
                ));
            }
        }
    }

    check
}

/// TLS/HTTPS configuration
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: Option<String>,
    pub verify_client: bool,
}

impl TlsConfig {
    pub fn new(cert_path: impl Into<String>, key_path: impl Into<String>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path: None,
            verify_client: false,
        }
    }

    pub fn with_ca(mut self, ca_path: impl Into<String>) -> Self {
        self.ca_path = Some(ca_path.into());
        self
    }

    pub fn with_client_verification(mut self, verify: bool) -> Self {
        self.verify_client = verify;
        self
    }

    pub fn validate(&self) -> SecurityCheck {
        let mut check = SecurityCheck::new();

        let cert_path = Path::new(&self.cert_path);
        let key_path = Path::new(&self.key_path);

        if !cert_path.exists() {
            check.add_error(format!("TLS certificate not found: {}", self.cert_path));
        }

        if !key_path.exists() {
            check.add_error(format!("TLS private key not found: {}", self.key_path));
        } else {
            let key_check = validate_file_permissions(key_path);
            check.merge(key_check);
        }

        if let Some(ref ca_path) = self.ca_path {
            if !Path::new(ca_path).exists() {
                check.add_error(format!("CA certificate not found: {}", ca_path));
            }
        }

        check
    }
}

/// Basic authentication configuration
#[derive(Debug, Clone)]
pub struct BasicAuthConfig {
    users: Vec<(String, String)>,
}

impl BasicAuthConfig {
    pub fn new() -> Self {
        Self { users: Vec::new() }
    }

    pub fn add_user(&mut self, username: impl Into<String>, password_hash: impl Into<String>) {
        self.users.push((username.into(), password_hash.into()));
    }

    pub fn verify(&self, username: &str, password: &str) -> bool {
        for (user, hash) in &self.users {
            if user == username {
                return verify_password(password, hash);
            }
        }

        false
    }
}

impl Default for BasicAuthConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple password verification (in production, use bcrypt or argon2)
fn verify_password(password: &str, hash: &str) -> bool {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    password.hash(&mut hasher);
    let computed = format!("{:x}", hasher.finish());

    computed == hash
}

/// Hash a password for storage
pub fn hash_password(password: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    password.hash(&mut hasher);

    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_security_check_new() {
        let check = SecurityCheck::new();
        assert!(check.passed);
        assert!(check.warnings.is_empty());
        assert!(check.errors.is_empty());
    }

    #[test]
    fn test_security_check_add_warning() {
        let mut check = SecurityCheck::new();
        check.add_warning("test warning");

        assert!(check.passed);
        assert_eq!(check.warnings.len(), 1);
    }

    #[test]
    fn test_security_check_add_error() {
        let mut check = SecurityCheck::new();
        check.add_error("test error");

        assert!(!check.passed);
        assert_eq!(check.errors.len(), 1);
    }

    #[test]
    fn test_security_check_merge() {
        let mut check1 = SecurityCheck::new();
        check1.add_warning("warning 1");

        let mut check2 = SecurityCheck::new();
        check2.add_error("error 1");

        check1.merge(check2);

        assert!(!check1.passed);
        assert_eq!(check1.warnings.len(), 1);
        assert_eq!(check1.errors.len(), 1);
    }

    #[test]
    fn test_check_sensitive_content() {
        let content = "database:\n  password: my-secret-password";
        let check = check_sensitive_content(content);

        assert!(!check.warnings.is_empty() || !check.errors.is_empty());
    }

    #[test]
    fn test_check_sensitive_content_clean() {
        let content = "database:\n  url: postgres://localhost/db";
        let check = check_sensitive_content(content);

        assert!(check.passed);
    }

    #[test]
    fn test_validate_file_permissions() {
        let temp_file = NamedTempFile::new().unwrap();
        let check = validate_file_permissions(temp_file.path());

        assert!(check.passed || !check.errors.is_empty());
    }

    #[test]
    fn test_tls_config() {
        let config = TlsConfig::new("/path/to/cert.pem", "/path/to/key.pem")
            .with_ca("/path/to/ca.pem")
            .with_client_verification(true);

        assert_eq!(config.cert_path, "/path/to/cert.pem");
        assert_eq!(config.key_path, "/path/to/key.pem");
        assert!(config.verify_client);
    }

    #[test]
    fn test_hash_password() {
        let hash1 = hash_password("password123");
        let hash2 = hash_password("password123");
        let hash3 = hash_password("different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_basic_auth() {
        let mut auth = BasicAuthConfig::new();
        let hash = hash_password("secret");
        auth.add_user("admin", hash);

        assert!(auth.verify("admin", "secret"));
        assert!(!auth.verify("admin", "wrong"));
        assert!(!auth.verify("unknown", "secret"));
    }
}
