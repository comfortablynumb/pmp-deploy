//! # pmp-deploy Plugin SDK
//!
//! This crate provides the types and macros needed to create infrastructure
//! plugins for pmp-deploy.
//!
//! ## Creating a Plugin
//!
//! Use the `declare_plugin!` macro to create a plugin:
//!
//! ```rust,ignore
//! use pmp_deploy_plugin_sdk::*;
//!
//! declare_plugin!(
//!     name: "my-cloud-provider",
//!     version: "0.1.0",
//!     description: "My Cloud Provider plugin",
//!     infrastructure_type: "my-cloud",
//!     plugin: MyPlugin,
//! );
//!
//! struct MyPlugin;
//!
//! impl InfrastructurePlugin for MyPlugin {
//!     fn validate_config(&self, config: &PluginConfig) -> PluginResult<()> {
//!         Ok(())
//!     }
//!
//!     fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
//!         Ok(DeploymentResult::success("Deployed successfully", "v1.0.0"))
//!     }
//!
//!     // ... implement other methods
//! }
//! ```

use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::ptr;
use std::slice;

/// Plugin API version - must match the host's version.
pub const PLUGIN_API_VERSION: u32 = 1;

/// FFI-safe string for cross-boundary communication.
#[repr(C)]
pub struct FfiString {
    ptr: *mut c_char,
    len: usize,
    capacity: usize,
}

impl FfiString {
    pub fn from_string(s: String) -> Self {
        let bytes = s.into_bytes();
        let len = bytes.len();
        let capacity = bytes.capacity();
        let ptr = bytes.leak().as_mut_ptr() as *mut c_char;
        Self { ptr, len, capacity }
    }

    pub fn as_str(&self) -> &str {
        if self.ptr.is_null() {
            return "";
        }

        unsafe {
            let bytes = slice::from_raw_parts(self.ptr as *const u8, self.len);
            std::str::from_utf8_unchecked(bytes)
        }
    }

    pub fn empty() -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }
}

impl Default for FfiString {
    fn default() -> Self {
        Self::empty()
    }
}

/// FFI-safe result type.
#[repr(C)]
pub struct FfiResult {
    pub success: bool,
    pub data: FfiString,
    pub error: FfiString,
}

impl FfiResult {
    pub fn ok(data: String) -> Self {
        Self {
            success: true,
            data: FfiString::from_string(data),
            error: FfiString::empty(),
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            success: false,
            data: FfiString::empty(),
            error: FfiString::from_string(error),
        }
    }
}

/// FFI-safe plugin metadata.
#[repr(C)]
pub struct FfiPluginMetadata {
    pub name: FfiString,
    pub version: FfiString,
    pub description: FfiString,
    pub infrastructure_type: FfiString,
}

impl FfiPluginMetadata {
    pub fn new(
        name: &str,
        version: &str,
        description: &str,
        infrastructure_type: &str,
    ) -> Self {
        Self {
            name: FfiString::from_string(name.to_string()),
            version: FfiString::from_string(version.to_string()),
            description: FfiString::from_string(description.to_string()),
            infrastructure_type: FfiString::from_string(infrastructure_type.to_string()),
        }
    }
}

/// FFI-safe deployment context.
#[repr(C)]
pub struct FfiDeploymentContext {
    pub environment_name: FfiString,
    pub infrastructure_name: FfiString,
    pub config_json: FfiString,
    pub dry_run: bool,
    pub verbose: bool,
}

/// FFI-safe deployment result.
#[repr(C)]
pub struct FfiDeploymentResult {
    pub success: bool,
    pub message: FfiString,
    pub version: FfiString,
    pub rollback_version: FfiString,
}

impl FfiDeploymentResult {
    pub fn ok(message: String, version: String, rollback_version: Option<String>) -> Self {
        Self {
            success: true,
            message: FfiString::from_string(message),
            version: FfiString::from_string(version),
            rollback_version: FfiString::from_string(rollback_version.unwrap_or_default()),
        }
    }

    pub fn err(message: String) -> Self {
        Self {
            success: false,
            message: FfiString::from_string(message),
            version: FfiString::empty(),
            rollback_version: FfiString::empty(),
        }
    }
}

/// Plugin error type.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Deployment error: {0}")]
    DeploymentError(String),

    #[error("Rollback error: {0}")]
    RollbackError(String),

    #[error("Status error: {0}")]
    StatusError(String),

    #[error("Logs error: {0}")]
    LogsError(String),

    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Plugin result type.
pub type PluginResult<T> = Result<T, PluginError>;

/// Plugin configuration passed from config file.
pub type PluginConfig = HashMap<String, serde_json::Value>;

/// Deployment context available to plugins.
#[derive(Debug, Clone)]
pub struct DeploymentContext {
    pub environment_name: String,
    pub infrastructure_name: String,
    pub config: PluginConfig,
    pub dry_run: bool,
    pub verbose: bool,
}

impl DeploymentContext {
    /// Parse from FFI context.
    ///
    /// # Safety
    /// The pointer must be valid.
    pub unsafe fn from_ffi(ctx: *const FfiDeploymentContext) -> Self {
        let ffi = unsafe { &*ctx };
        let config_json = ffi.config_json.as_str();
        let config: PluginConfig = serde_json::from_str(config_json).unwrap_or_default();

        Self {
            environment_name: ffi.environment_name.as_str().to_string(),
            infrastructure_name: ffi.infrastructure_name.as_str().to_string(),
            config,
            dry_run: ffi.dry_run,
            verbose: ffi.verbose,
        }
    }
}

/// Deployment result returned by plugins.
#[derive(Debug, Clone)]
pub struct DeploymentResult {
    pub success: bool,
    pub message: String,
    pub version: Option<String>,
    pub rollback_version: Option<String>,
}

impl DeploymentResult {
    pub fn success(message: &str, version: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            version: Some(version.to_string()),
            rollback_version: None,
        }
    }

    pub fn success_with_rollback(message: &str, version: &str, rollback: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            version: Some(version.to_string()),
            rollback_version: Some(rollback.to_string()),
        }
    }

    pub fn failure(message: &str) -> Self {
        Self {
            success: false,
            message: message.to_string(),
            version: None,
            rollback_version: None,
        }
    }

    pub fn to_ffi(self) -> FfiDeploymentResult {
        if self.success {
            FfiDeploymentResult::ok(
                self.message,
                self.version.unwrap_or_default(),
                self.rollback_version,
            )
        } else {
            FfiDeploymentResult::err(self.message)
        }
    }
}

/// Trait that plugins must implement.
pub trait InfrastructurePlugin: Send + Sync {
    /// Validate the configuration.
    fn validate_config(&self, config: &PluginConfig) -> PluginResult<()>;

    /// Deploy to the infrastructure.
    fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult>;

    /// Rollback a deployment.
    fn rollback(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult>;

    /// Get deployment status.
    fn status(&self, ctx: &DeploymentContext) -> PluginResult<String>;

    /// Get logs from the deployment.
    fn logs(&self, ctx: &DeploymentContext, follow: bool) -> PluginResult<String>;

    /// Initialize the plugin (optional).
    fn init(&self) -> PluginResult<()> {
        Ok(())
    }

    /// Shutdown the plugin (optional).
    fn shutdown(&self) {}
}

/// Helper to parse config JSON.
pub fn parse_config(config_json: *const c_char) -> PluginConfig {
    if config_json.is_null() {
        return HashMap::new();
    }

    let c_str = unsafe { CStr::from_ptr(config_json) };
    let json_str = c_str.to_string_lossy();

    serde_json::from_str(&json_str).unwrap_or_default()
}

/// Macro to declare a plugin with all required FFI exports.
///
/// # Example
///
/// ```rust,ignore
/// declare_plugin!(
///     name: "my-plugin",
///     version: "0.1.0",
///     description: "My infrastructure plugin",
///     infrastructure_type: "my-infra",
///     plugin: MyPlugin,
/// );
/// ```
#[macro_export]
macro_rules! declare_plugin {
    (
        name: $name:expr,
        version: $version:expr,
        description: $description:expr,
        infrastructure_type: $infra_type:expr,
        plugin: $plugin_type:ty,
    ) => {
        static PLUGIN_INSTANCE: std::sync::OnceLock<$plugin_type> = std::sync::OnceLock::new();

        fn get_plugin() -> &'static $plugin_type {
            PLUGIN_INSTANCE.get_or_init(|| <$plugin_type>::default())
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_api_version() -> u32 {
            $crate::PLUGIN_API_VERSION
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_metadata() -> $crate::FfiPluginMetadata {
            $crate::FfiPluginMetadata::new($name, $version, $description, $infra_type)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_validate_config(
            config_json: *const std::ffi::c_char,
        ) -> $crate::FfiResult {
            let config = $crate::parse_config(config_json);

            match get_plugin().validate_config(&config) {
                Ok(()) => $crate::FfiResult::ok(String::new()),
                Err(e) => $crate::FfiResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_deploy(
            ctx: *const $crate::FfiDeploymentContext,
        ) -> $crate::FfiDeploymentResult {
            let context = unsafe { $crate::DeploymentContext::from_ffi(ctx) };

            match get_plugin().deploy(&context) {
                Ok(result) => result.to_ffi(),
                Err(e) => $crate::FfiDeploymentResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_rollback(
            ctx: *const $crate::FfiDeploymentContext,
        ) -> $crate::FfiDeploymentResult {
            let context = unsafe { $crate::DeploymentContext::from_ffi(ctx) };

            match get_plugin().rollback(&context) {
                Ok(result) => result.to_ffi(),
                Err(e) => $crate::FfiDeploymentResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_status(
            ctx: *const $crate::FfiDeploymentContext,
        ) -> $crate::FfiResult {
            let context = unsafe { $crate::DeploymentContext::from_ffi(ctx) };

            match get_plugin().status(&context) {
                Ok(status) => $crate::FfiResult::ok(status),
                Err(e) => $crate::FfiResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_logs(
            ctx: *const $crate::FfiDeploymentContext,
            follow: bool,
        ) -> $crate::FfiResult {
            let context = unsafe { $crate::DeploymentContext::from_ffi(ctx) };

            match get_plugin().logs(&context, follow) {
                Ok(logs) => $crate::FfiResult::ok(logs),
                Err(e) => $crate::FfiResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_init() -> $crate::FfiResult {
            match get_plugin().init() {
                Ok(()) => $crate::FfiResult::ok(String::new()),
                Err(e) => $crate::FfiResult::err(e.to_string()),
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn pmp_plugin_shutdown() {
            get_plugin().shutdown();
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_string() {
        let s = FfiString::from_string("hello".to_string());
        assert_eq!(s.as_str(), "hello");
    }

    #[test]
    fn test_ffi_string_empty() {
        let s = FfiString::empty();
        assert_eq!(s.as_str(), "");
    }

    #[test]
    fn test_ffi_result_ok() {
        let r = FfiResult::ok("success".to_string());
        assert!(r.success);
        assert_eq!(r.data.as_str(), "success");
    }

    #[test]
    fn test_ffi_result_err() {
        let r = FfiResult::err("failure".to_string());
        assert!(!r.success);
        assert_eq!(r.error.as_str(), "failure");
    }

    #[test]
    fn test_deployment_result_success() {
        let r = DeploymentResult::success("deployed", "v1.0.0");
        assert!(r.success);
        assert_eq!(r.version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_deployment_result_failure() {
        let r = DeploymentResult::failure("failed");
        assert!(!r.success);
        assert!(r.version.is_none());
    }
}
