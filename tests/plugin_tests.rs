use std::path::PathBuf;
use tempfile::TempDir;

use pmp_deploy::plugins::{DefaultPluginLoader, PluginLoader, PluginRegistry, PluginRegistryTrait};

#[test]
fn test_plugin_registry_new() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

    assert!(registry.list_plugins().is_empty());
}

#[test]
fn test_plugin_registry_has_plugin() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

    assert!(!registry.has_plugin("nonexistent"));
}

#[test]
fn test_plugin_registry_get_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

    assert!(registry.get_plugin("nonexistent").is_none());
}

#[test]
fn test_plugin_registry_clear() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

    registry.clear();
    assert!(registry.list_plugins().is_empty());
}

#[test]
fn test_plugin_registry_plugin_directory() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().to_path_buf();
    let registry = PluginRegistry::with_plugin_directory(path.clone());

    assert_eq!(registry.plugin_directory(), path);
}

#[test]
fn test_plugin_loader_nonexistent_directory() {
    let loader = DefaultPluginLoader::with_directory(PathBuf::from("/nonexistent/path"));
    let plugins = loader.discover_plugins();

    assert!(plugins.is_ok());
    assert!(plugins.unwrap().is_empty());
}

#[test]
fn test_plugin_loader_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins();

    assert!(plugins.is_ok());
    assert!(plugins.unwrap().is_empty());
}

#[test]
fn test_plugin_loader_filters_non_plugin_files() {
    let temp_dir = TempDir::new().unwrap();

    std::fs::write(temp_dir.path().join("readme.txt"), "Not a plugin").unwrap();
    std::fs::write(temp_dir.path().join("config.yaml"), "Not a plugin").unwrap();
    std::fs::write(temp_dir.path().join("script.sh"), "Not a plugin").unwrap();

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert!(plugins.is_empty());
}

#[cfg(target_os = "windows")]
#[test]
fn test_plugin_loader_discovers_dll_files() {
    let temp_dir = TempDir::new().unwrap();

    std::fs::write(temp_dir.path().join("myplugin.dll"), "fake dll content").unwrap();
    std::fs::write(temp_dir.path().join("readme.txt"), "Not a plugin").unwrap();

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert_eq!(plugins.len(), 1);
    assert!(plugins[0].ends_with("myplugin.dll"));
}

#[cfg(target_os = "linux")]
#[test]
fn test_plugin_loader_discovers_so_files() {
    let temp_dir = TempDir::new().unwrap();

    std::fs::write(temp_dir.path().join("libmyplugin.so"), "fake so content").unwrap();
    std::fs::write(temp_dir.path().join("readme.txt"), "Not a plugin").unwrap();

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert_eq!(plugins.len(), 1);
    assert!(plugins[0].ends_with("libmyplugin.so"));
}

#[cfg(target_os = "macos")]
#[test]
fn test_plugin_loader_discovers_dylib_files() {
    let temp_dir = TempDir::new().unwrap();

    std::fs::write(
        temp_dir.path().join("libmyplugin.dylib"),
        "fake dylib content",
    )
    .unwrap();
    std::fs::write(temp_dir.path().join("readme.txt"), "Not a plugin").unwrap();

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert_eq!(plugins.len(), 1);
    assert!(plugins[0].ends_with("libmyplugin.dylib"));
}

#[test]
fn test_plugin_registry_list_empty() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());
    let plugins = registry.list_plugins();

    assert!(plugins.is_empty());
}

#[test]
fn test_plugin_loader_multiple_plugins() {
    let temp_dir = TempDir::new().unwrap();

    #[cfg(target_os = "windows")]
    {
        std::fs::write(temp_dir.path().join("plugin1.dll"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("plugin2.dll"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("plugin3.dll"), "fake").unwrap();
    }

    #[cfg(target_os = "linux")]
    {
        std::fs::write(temp_dir.path().join("libplugin1.so"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("libplugin2.so"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("libplugin3.so"), "fake").unwrap();
    }

    #[cfg(target_os = "macos")]
    {
        std::fs::write(temp_dir.path().join("libplugin1.dylib"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("libplugin2.dylib"), "fake").unwrap();
        std::fs::write(temp_dir.path().join("libplugin3.dylib"), "fake").unwrap();
    }

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert_eq!(plugins.len(), 3);
}

#[test]
fn test_plugin_loader_subdirectories_ignored() {
    let temp_dir = TempDir::new().unwrap();

    let subdir = temp_dir.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();

    #[cfg(target_os = "windows")]
    {
        std::fs::write(subdir.join("nested.dll"), "fake").unwrap();
    }

    #[cfg(target_os = "linux")]
    {
        std::fs::write(subdir.join("libnested.so"), "fake").unwrap();
    }

    #[cfg(target_os = "macos")]
    {
        std::fs::write(subdir.join("libnested.dylib"), "fake").unwrap();
    }

    let loader = DefaultPluginLoader::with_directory(temp_dir.path().to_path_buf());
    let plugins = loader.discover_plugins().unwrap();

    assert!(plugins.is_empty());
}

#[test]
fn test_plugin_count() {
    let temp_dir = TempDir::new().unwrap();
    let registry = PluginRegistry::with_plugin_directory(temp_dir.path().to_path_buf());

    assert_eq!(registry.plugin_count(), 0);
}
