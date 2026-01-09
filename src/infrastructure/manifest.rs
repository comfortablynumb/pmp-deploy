//! Kubernetes manifest templating using Tera template engine.
//!
//! This module provides manifest rendering with variable substitution
//! from configuration, environment variables, and built-in values.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tera::{Context, Tera};

/// Configuration for manifest templating.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestTemplateConfig {
    /// Path to the template file or directory.
    pub path: String,

    /// Additional variables to pass to templates.
    #[serde(default)]
    pub variables: HashMap<String, String>,

    /// Whether to include environment variables in template context.
    #[serde(default = "default_true")]
    pub include_env: bool,

    /// Prefix for environment variables to include (e.g., "APP_").
    pub env_prefix: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ManifestTemplateConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }
}

/// Built-in variables available in templates.
#[derive(Debug, Clone)]
pub struct BuiltinVariables {
    pub image: Option<String>,
    pub image_tag: Option<String>,
    pub namespace: String,
    pub environment: String,
    pub deployment_name: Option<String>,
}

impl BuiltinVariables {
    pub fn new(namespace: &str, environment: &str) -> Self {
        Self {
            image: None,
            image_tag: None,
            namespace: namespace.to_string(),
            environment: environment.to_string(),
            deployment_name: None,
        }
    }

    pub fn with_image(mut self, image: &str) -> Self {
        self.image = Some(image.to_string());

        // Extract tag from image if present (e.g., "myapp:v1.0.0" -> "v1.0.0")
        if let Some(tag) = image.rsplit(':').next() {
            if !tag.contains('/') {
                self.image_tag = Some(tag.to_string());
            }
        }

        self
    }

    pub fn with_deployment_name(mut self, name: &str) -> Self {
        self.deployment_name = Some(name.to_string());
        self
    }
}

/// Renders Kubernetes manifests from templates.
pub struct ManifestRenderer {
    tera: Tera,
}

impl ManifestRenderer {
    /// Create a new renderer from a template path.
    ///
    /// The path can be a single file or a directory with glob pattern.
    pub fn new(template_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(template_path);

        let tera = if path.is_dir() {
            // Directory: load all YAML files
            let pattern = format!("{}/**/*.yaml", template_path);
            Tera::new(&pattern)?
        } else if path.is_file() {
            // Single file: create Tera with just this file
            let mut tera = Tera::default();
            let content = std::fs::read_to_string(path)?;
            let template_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("template.yaml");
            tera.add_raw_template(template_name, &content)?;
            tera
        } else {
            // Try as glob pattern directly
            Tera::new(template_path)?
        };

        Ok(Self { tera })
    }

    /// Create a renderer from raw template content.
    pub fn from_string(name: &str, content: &str) -> anyhow::Result<Self> {
        let mut tera = Tera::default();
        tera.add_raw_template(name, content)?;
        Ok(Self { tera })
    }

    /// Render all templates with the given context.
    pub fn render_all(
        &self,
        config: &ManifestTemplateConfig,
        builtins: &BuiltinVariables,
    ) -> anyhow::Result<Vec<RenderedManifest>> {
        let context = self.build_context(config, builtins);
        let mut results = Vec::new();

        for template_name in self.tera.get_template_names() {
            let rendered = self.tera.render(template_name, &context)?;
            results.push(RenderedManifest {
                name: template_name.to_string(),
                content: rendered,
            });
        }

        Ok(results)
    }

    /// Render a specific template by name.
    pub fn render(
        &self,
        template_name: &str,
        config: &ManifestTemplateConfig,
        builtins: &BuiltinVariables,
    ) -> anyhow::Result<String> {
        let context = self.build_context(config, builtins);
        let rendered = self.tera.render(template_name, &context)?;
        Ok(rendered)
    }

    fn build_context(
        &self,
        config: &ManifestTemplateConfig,
        builtins: &BuiltinVariables,
    ) -> Context {
        let mut context = Context::new();

        // Add built-in variables
        if let Some(image) = &builtins.image {
            context.insert("IMAGE", image);
        }
        if let Some(tag) = &builtins.image_tag {
            context.insert("IMAGE_TAG", tag);
        }
        context.insert("NAMESPACE", &builtins.namespace);
        context.insert("ENVIRONMENT", &builtins.environment);
        if let Some(name) = &builtins.deployment_name {
            context.insert("DEPLOYMENT_NAME", name);
        }

        // Add config variables
        for (key, value) in &config.variables {
            context.insert(key, value);
        }

        // Add environment variables if enabled
        if config.include_env {
            for (key, value) in std::env::vars() {
                let should_include = config
                    .env_prefix
                    .as_ref()
                    .map(|prefix| key.starts_with(prefix))
                    .unwrap_or(true);

                if should_include {
                    context.insert(&key, &value);
                }
            }
        }

        context
    }

    /// List all available template names.
    pub fn template_names(&self) -> Vec<&str> {
        self.tera.get_template_names().collect()
    }
}

/// A rendered manifest ready for application.
#[derive(Debug, Clone)]
pub struct RenderedManifest {
    /// Template name or file name.
    pub name: String,
    /// Rendered YAML content.
    pub content: String,
}

impl RenderedManifest {
    /// Parse the rendered content as multiple YAML documents.
    pub fn parse_documents(&self) -> anyhow::Result<Vec<serde_yaml::Value>> {
        let mut docs = Vec::new();

        for doc in serde_yaml::Deserializer::from_str(&self.content) {
            let value = serde_yaml::Value::deserialize(doc)?;
            if !value.is_null() {
                docs.push(value);
            }
        }

        Ok(docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_template_config_default() {
        let config = ManifestTemplateConfig::default();
        assert!(config.path.is_empty());
        assert!(config.variables.is_empty());
        assert!(!config.include_env);
    }

    #[test]
    fn test_builtin_variables_new() {
        let builtins = BuiltinVariables::new("production", "prod");
        assert_eq!(builtins.namespace, "production");
        assert_eq!(builtins.environment, "prod");
        assert!(builtins.image.is_none());
    }

    #[test]
    fn test_builtin_variables_with_image() {
        let builtins = BuiltinVariables::new("default", "staging")
            .with_image("myapp:v1.2.3");

        assert_eq!(builtins.image, Some("myapp:v1.2.3".to_string()));
        assert_eq!(builtins.image_tag, Some("v1.2.3".to_string()));
    }

    #[test]
    fn test_builtin_variables_image_without_tag() {
        let builtins = BuiltinVariables::new("default", "staging")
            .with_image("myapp");

        assert_eq!(builtins.image, Some("myapp".to_string()));
        // No tag extracted if there's no colon
        assert_eq!(builtins.image_tag, Some("myapp".to_string()));
    }

    #[test]
    fn test_manifest_renderer_from_string() {
        let template = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ DEPLOYMENT_NAME }}
  namespace: {{ NAMESPACE }}
spec:
  template:
    spec:
      containers:
        - name: app
          image: {{ IMAGE }}
"#;

        let renderer = ManifestRenderer::from_string("deployment.yaml", template).unwrap();
        let config = ManifestTemplateConfig {
            path: String::new(),
            variables: HashMap::new(),
            include_env: false,
            env_prefix: None,
        };
        let builtins = BuiltinVariables::new("production", "prod")
            .with_image("myapp:v1.0.0")
            .with_deployment_name("my-app");

        let rendered = renderer.render("deployment.yaml", &config, &builtins).unwrap();

        assert!(rendered.contains("name: my-app"));
        assert!(rendered.contains("namespace: production"));
        assert!(rendered.contains("image: myapp:v1.0.0"));
    }

    #[test]
    fn test_manifest_renderer_with_variables() {
        let template = r#"
replicas: {{ REPLICAS }}
memory: {{ MEMORY_LIMIT }}
"#;

        let renderer = ManifestRenderer::from_string("config.yaml", template).unwrap();
        let mut variables = HashMap::new();
        variables.insert("REPLICAS".to_string(), "3".to_string());
        variables.insert("MEMORY_LIMIT".to_string(), "512Mi".to_string());

        let config = ManifestTemplateConfig {
            path: String::new(),
            variables,
            include_env: false,
            env_prefix: None,
        };
        let builtins = BuiltinVariables::new("default", "dev");

        let rendered = renderer.render("config.yaml", &config, &builtins).unwrap();

        assert!(rendered.contains("replicas: 3"));
        assert!(rendered.contains("memory: 512Mi"));
    }

    #[test]
    fn test_rendered_manifest_parse_documents() {
        let manifest = RenderedManifest {
            name: "test.yaml".to_string(),
            content: r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
---
apiVersion: v1
kind: Secret
metadata:
  name: test-secret
"#.to_string(),
        };

        let docs = manifest.parse_documents().unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_manifest_template_config_from_yaml() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(r#"
path: ./manifests
variables:
  REPLICAS: "3"
  ENV: production
include_env: true
env_prefix: APP_
"#).unwrap();

        let config = ManifestTemplateConfig::from_yaml_value(&yaml).unwrap();
        assert_eq!(config.path, "./manifests");
        assert_eq!(config.variables.get("REPLICAS"), Some(&"3".to_string()));
        assert!(config.include_env);
        assert_eq!(config.env_prefix, Some("APP_".to_string()));
    }

    #[test]
    fn test_template_names() {
        let renderer = ManifestRenderer::from_string("test.yaml", "content").unwrap();
        let names: Vec<_> = renderer.template_names();
        assert_eq!(names, vec!["test.yaml"]);
    }
}
