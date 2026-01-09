//! Plugin template generator.

use crate::cli::commands::PluginNewArgs;
use std::fs;
use std::path::{Path, PathBuf};

/// Generates plugin scaffolding.
pub struct PluginGenerator {
    name: String,
    output_dir: PathBuf,
    infrastructure_type: String,
    description: String,
}

impl PluginGenerator {
    pub fn new(args: &PluginNewArgs) -> Self {
        let name = args.name.clone();
        let output_dir = args
            .output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(&name));

        let description = args
            .description
            .clone()
            .unwrap_or_else(|| format!("{} infrastructure plugin for pmp-deploy", name));

        Self {
            name,
            output_dir,
            infrastructure_type: args.infrastructure_type.clone(),
            description,
        }
    }

    pub fn plugin_name(&self) -> &str {
        &self.name
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn crate_name(&self) -> String {
        format!("pmp-deploy-{}-plugin", self.name.replace('-', "_"))
    }

    pub fn generate(&self) -> anyhow::Result<()> {
        self.create_directories()?;
        self.generate_cargo_toml()?;
        self.generate_lib_rs()?;
        self.generate_readme()?;

        Ok(())
    }

    fn create_directories(&self) -> anyhow::Result<()> {
        let src_dir = self.output_dir.join("src");
        fs::create_dir_all(&src_dir)?;
        Ok(())
    }

    fn generate_cargo_toml(&self) -> anyhow::Result<()> {
        let content = format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
description = "{description}"
license = "MIT"

[lib]
crate-type = ["cdylib"]

[dependencies]
pmp-deploy-plugin-sdk = {{ git = "https://github.com/yourusername/pmp-deploy", package = "pmp-deploy-plugin-sdk" }}
serde_json = "1"
"#,
            crate_name = self.crate_name(),
            description = self.description,
        );

        let path = self.output_dir.join("Cargo.toml");
        fs::write(&path, content)?;

        Ok(())
    }

    fn generate_lib_rs(&self) -> anyhow::Result<()> {
        let struct_name = to_pascal_case(&self.name);

        let content = format!(
            r#"//! {description}

use pmp_deploy_plugin_sdk::*;

/// {struct_name} plugin implementation.
#[derive(Default)]
pub struct {struct_name}Plugin;

impl InfrastructurePlugin for {struct_name}Plugin {{
    fn validate_config(&self, config: &PluginConfig) -> PluginResult<()> {{
        // TODO: Implement configuration validation
        // Example: Check for required fields
        // if !config.contains_key("required_field") {{
        //     return Err(PluginError::ConfigError(
        //         "Missing required field: 'required_field'".to_string(),
        //     ));
        // }}
        Ok(())
    }}

    fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {{
        if ctx.dry_run {{
            return Ok(DeploymentResult::success(
                &format!(
                    "[DRY RUN] Would deploy to environment '{{}}'",
                    ctx.environment_name
                ),
                "v0.0.0-dry-run",
            ));
        }}

        // TODO: Implement deployment logic
        // Example:
        // let target = ctx.config.get("target")
        //     .and_then(|v| v.as_str())
        //     .unwrap_or("default");
        //
        // deploy_to_infrastructure(target)?;

        Ok(DeploymentResult::success(
            &format!(
                "Successfully deployed to environment '{{}}'",
                ctx.environment_name
            ),
            "v1.0.0",
        ))
    }}

    fn rollback(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {{
        // TODO: Implement rollback logic
        Ok(DeploymentResult::success(
            &format!(
                "Successfully rolled back environment '{{}}'",
                ctx.environment_name
            ),
            "v0.9.0",
        ))
    }}

    fn status(&self, ctx: &DeploymentContext) -> PluginResult<String> {{
        // TODO: Implement status check
        Ok(format!(
            "Environment '{{}}': Running",
            ctx.environment_name
        ))
    }}

    fn logs(&self, ctx: &DeploymentContext, follow: bool) -> PluginResult<String> {{
        // TODO: Implement log retrieval
        let follow_str = if follow {{ " (following)" }} else {{ "" }};
        Ok(format!(
            "Logs for environment '{{}}'{{}}:\\n[No logs available yet]",
            ctx.environment_name, follow_str
        ))
    }}

    fn init(&self) -> PluginResult<()> {{
        // TODO: Implement plugin initialization (optional)
        Ok(())
    }}

    fn shutdown(&self) {{
        // TODO: Implement plugin cleanup (optional)
    }}
}}

// Export the plugin using the declare_plugin! macro
declare_plugin!(
    name: "{name}-plugin",
    version: "0.1.0",
    description: "{description}",
    infrastructure_type: "{infrastructure_type}",
    plugin: {struct_name}Plugin,
);

#[cfg(test)]
mod tests {{
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_validate_config() {{
        let plugin = {struct_name}Plugin;
        let config: PluginConfig = HashMap::new();
        assert!(plugin.validate_config(&config).is_ok());
    }}

    #[test]
    fn test_deploy_dry_run() {{
        let plugin = {struct_name}Plugin;
        let ctx = DeploymentContext {{
            environment_name: "test".to_string(),
            infrastructure_name: "test-infra".to_string(),
            config: HashMap::new(),
            dry_run: true,
            verbose: false,
        }};

        let result = plugin.deploy(&ctx).unwrap();
        assert!(result.success);
        assert!(result.message.contains("DRY RUN"));
    }}
}}
"#,
            name = self.name,
            struct_name = struct_name,
            description = self.description,
            infrastructure_type = self.infrastructure_type,
        );

        let path = self.output_dir.join("src").join("lib.rs");
        fs::write(&path, content)?;

        Ok(())
    }

    fn generate_readme(&self) -> anyhow::Result<()> {
        let struct_name = to_pascal_case(&self.name);

        let content = format!(
            r#"# {name}-plugin

{description}

## Building

```bash
cargo build --release
```

## Installation

Copy the built library to the pmp-deploy plugins directory:

### Windows
```bash
copy target\release\{crate_name}.dll %USERPROFILE%\.pmp-deploy\plugins\
```

### Linux/macOS
```bash
cp target/release/lib{crate_name}.so ~/.pmp-deploy/plugins/
# or on macOS:
cp target/release/lib{crate_name}.dylib ~/.pmp-deploy/plugins/
```

## Usage

In your `pmp-deploy.yaml`:

```yaml
environments:
  my-env:
    infrastructure: my-{name}
    image: my-image:tag

infrastructure:
  my-{name}:
    type: {infrastructure_type}
    config:
      # Add your configuration here
```

## Configuration Options

| Option | Required | Description |
|--------|----------|-------------|
| TBD    | TBD      | TBD         |

## Development

Run tests:

```bash
cargo test
```

## License

MIT
"#,
            name = self.name,
            description = self.description,
            crate_name = self.crate_name(),
            infrastructure_type = self.infrastructure_type,
        );

        let path = self.output_dir.join("README.md");
        fs::write(&path, content)?;

        Ok(())
    }
}

/// Convert a kebab-case or snake_case string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split(|c| c == '-' || c == '_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("my-plugin"), "MyPlugin");
        assert_eq!(to_pascal_case("my_plugin"), "MyPlugin");
        assert_eq!(to_pascal_case("hello-world-test"), "HelloWorldTest");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_crate_name() {
        let args = PluginNewArgs {
            name: "my-cloud".to_string(),
            output_dir: None,
            infrastructure_type: "my-cloud".to_string(),
            description: None,
        };
        let generator = PluginGenerator::new(&args);
        assert_eq!(generator.crate_name(), "pmp-deploy-my_cloud-plugin");
    }
}
