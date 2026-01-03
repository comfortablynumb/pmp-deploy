use std::ffi::{c_char, CStr, CString};
use std::collections::HashMap;
use std::ptr;
use std::slice;

/// FFI-safe string that can be passed across plugin boundaries.
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

    pub fn into_string(self) -> String {
        if self.ptr.is_null() {
            return String::new();
        }

        unsafe {
            let bytes = Vec::from_raw_parts(self.ptr as *mut u8, self.len, self.capacity);
            String::from_utf8_unchecked(bytes)
        }
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

/// FFI-safe result type for plugin operations.
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

    pub fn into_result(self) -> anyhow::Result<String> {
        if self.success {
            Ok(self.data.into_string())
        } else {
            Err(anyhow::anyhow!(self.error.into_string()))
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

/// FFI-safe deployment context passed to plugins.
#[repr(C)]
pub struct FfiDeploymentContext {
    pub environment_name: FfiString,
    pub infrastructure_name: FfiString,
    pub config_json: FfiString,
    pub dry_run: bool,
    pub verbose: bool,
}

impl FfiDeploymentContext {
    pub fn from_context(
        env_name: &str,
        infra_name: &str,
        config: &HashMap<String, serde_yaml::Value>,
        dry_run: bool,
        verbose: bool,
    ) -> Self {
        let config_json = serde_json::to_string(config).unwrap_or_default();
        Self {
            environment_name: FfiString::from_string(env_name.to_string()),
            infrastructure_name: FfiString::from_string(infra_name.to_string()),
            config_json: FfiString::from_string(config_json),
            dry_run,
            verbose,
        }
    }
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

/// Plugin API version for compatibility checking.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Function signatures that plugins must export.
pub type PluginApiVersionFn = unsafe extern "C" fn() -> u32;
pub type PluginMetadataFn = unsafe extern "C" fn() -> FfiPluginMetadata;
pub type PluginValidateConfigFn = unsafe extern "C" fn(config_json: *const c_char) -> FfiResult;
pub type PluginDeployFn = unsafe extern "C" fn(ctx: *const FfiDeploymentContext) -> FfiDeploymentResult;
pub type PluginRollbackFn = unsafe extern "C" fn(ctx: *const FfiDeploymentContext) -> FfiDeploymentResult;
pub type PluginStatusFn = unsafe extern "C" fn(ctx: *const FfiDeploymentContext) -> FfiResult;
pub type PluginLogsFn = unsafe extern "C" fn(ctx: *const FfiDeploymentContext, follow: bool) -> FfiResult;
pub type PluginInitFn = unsafe extern "C" fn() -> FfiResult;
pub type PluginShutdownFn = unsafe extern "C" fn();

/// Symbol names that plugins must export.
pub const SYMBOL_API_VERSION: &[u8] = b"pmp_plugin_api_version\0";
pub const SYMBOL_METADATA: &[u8] = b"pmp_plugin_metadata\0";
pub const SYMBOL_VALIDATE_CONFIG: &[u8] = b"pmp_plugin_validate_config\0";
pub const SYMBOL_DEPLOY: &[u8] = b"pmp_plugin_deploy\0";
pub const SYMBOL_ROLLBACK: &[u8] = b"pmp_plugin_rollback\0";
pub const SYMBOL_STATUS: &[u8] = b"pmp_plugin_status\0";
pub const SYMBOL_LOGS: &[u8] = b"pmp_plugin_logs\0";
pub const SYMBOL_INIT: &[u8] = b"pmp_plugin_init\0";
pub const SYMBOL_SHUTDOWN: &[u8] = b"pmp_plugin_shutdown\0";

/// Helper to convert a Rust string to a C string for FFI calls.
pub fn to_c_string(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap())
}

/// Helper to convert a C string pointer to a Rust string.
///
/// # Safety
/// The pointer must be valid and null-terminated.
pub unsafe fn from_c_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }

    unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}
