use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;

use super::provider::{DeploymentContext, InfrastructureProvider, InfrastructureType};
use crate::config::InfrastructureConfig;
use crate::deployment::{DeploymentResult, DeploymentType};

pub struct DockerComposeProvider {
    compose_file: String,
    project_name: Option<String>,
    ssh_host: Option<String>,
    working_dir: Option<String>,
}

impl DockerComposeProvider {
    pub fn new(compose_file: &str, project_name: Option<&str>) -> Self {
        Self {
            compose_file: compose_file.to_string(),
            project_name: project_name.map(String::from),
            ssh_host: None,
            working_dir: None,
        }
    }

    pub fn with_ssh_host(mut self, host: &str) -> Self {
        self.ssh_host = Some(host.to_string());
        self
    }

    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    pub fn from_config(config: &InfrastructureConfig) -> anyhow::Result<Self> {
        let compose_file = config
            .config
            .get("compose_file")
            .and_then(|v| v.as_str())
            .unwrap_or("docker-compose.yml")
            .to_string();

        let project_name = config
            .config
            .get("project_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let ssh_host = config
            .config
            .get("ssh_host")
            .and_then(|v| v.as_str())
            .map(String::from);

        let working_dir = config
            .config
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(Self {
            compose_file,
            project_name,
            ssh_host,
            working_dir,
        })
    }

    fn build_base_command(&self) -> Command {
        let mut cmd = if let Some(ssh_host) = &self.ssh_host {
            let mut c = Command::new("ssh");
            c.arg(ssh_host);
            c.arg("docker");
            c.arg("compose");
            c
        } else {
            let mut c = Command::new("docker");
            c.arg("compose");
            c
        };

        cmd.arg("-f").arg(&self.compose_file);

        if let Some(project) = &self.project_name {
            cmd.arg("-p").arg(project);
        }

        if let Some(dir) = &self.working_dir {
            cmd.current_dir(dir);
        }

        cmd
    }

    async fn run_command(&self, args: &[&str]) -> anyhow::Result<String> {
        let mut cmd = self.build_base_command();

        for arg in args {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker Compose command failed: {}", stderr)
        }
    }

    async fn check_docker_available(&self) -> anyhow::Result<()> {
        let docker_path = which::which("docker");

        if docker_path.is_err() {
            anyhow::bail!(
                "Docker is not installed or not in PATH. \
                Please install Docker: https://docs.docker.com/get-docker/"
            );
        }

        let mut cmd = Command::new("docker");
        cmd.arg("info");
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let status = cmd.status().await?;

        if !status.success() {
            anyhow::bail!(
                "Docker daemon is not running. \
                Please start Docker Desktop or the Docker service."
            );
        }

        Ok(())
    }
}

#[async_trait]
impl InfrastructureProvider for DockerComposeProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::DockerCompose
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        // Docker Compose doesn't support rolling updates
        // All containers are stopped and restarted together
        vec![DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        _config: &std::collections::HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        self.check_docker_available().await?;

        let mut cmd = self.build_base_command();
        cmd.arg("config");
        cmd.arg("--quiet");
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Invalid Docker Compose configuration: {}", stderr);
        }

        Ok(())
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        self.check_docker_available().await?;

        let image = ctx
            .environment
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No image specified for deployment"))?;

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would deploy {} using Docker Compose",
                image
            )));
        }

        tracing::info!("Pulling latest images...");
        self.run_command(&["pull"]).await?;

        tracing::info!("Stopping existing containers...");
        let _ = self.run_command(&["down", "--remove-orphans"]).await;

        tracing::info!("Starting containers...");
        self.run_command(&["up", "-d", "--remove-orphans"]).await?;

        tracing::info!("Waiting for containers to be healthy...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let _status = self.run_command(&["ps", "--format", "json"]).await?;

        Ok(DeploymentResult::success(format!(
            "Deployed {} successfully",
            image
        ))
        .with_version(image.clone()))
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        if ctx.dry_run {
            return Ok(DeploymentResult::success(
                "Would rollback Docker Compose deployment",
            ));
        }

        tracing::warn!(
            "Docker Compose rollback requires manually reverting to a previous image version"
        );

        self.run_command(&["down"]).await?;
        self.run_command(&["up", "-d"]).await?;

        Ok(DeploymentResult::success(
            "Restarted containers. Manual image version rollback may be required.",
        ))
    }

    async fn status(&self, _ctx: &DeploymentContext) -> anyhow::Result<String> {
        self.check_docker_available().await?;

        let output = self.run_command(&["ps"]).await?;

        Ok(output)
    }

    async fn logs(&self, _ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        self.check_docker_available().await?;

        let mut cmd = self.build_base_command();
        cmd.arg("logs");

        if follow {
            cmd.arg("-f");
        } else {
            cmd.arg("--tail").arg("100");
        }

        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let mut child = cmd.spawn()?;
        child.wait().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_compose_provider_new() {
        let provider = DockerComposeProvider::new("docker-compose.yml", Some("myapp"));

        assert_eq!(provider.compose_file, "docker-compose.yml");
        assert_eq!(provider.project_name, Some("myapp".to_string()));
    }

    #[test]
    fn test_docker_compose_provider_from_config() {
        use std::collections::HashMap;

        let mut config_map = HashMap::new();
        config_map.insert(
            "compose_file".to_string(),
            serde_yaml::Value::String("custom-compose.yml".to_string()),
        );
        config_map.insert(
            "project_name".to_string(),
            serde_yaml::Value::String("test-project".to_string()),
        );

        let config = InfrastructureConfig {
            infrastructure_type: "docker-compose".to_string(),
            config: config_map,
        };

        let provider = DockerComposeProvider::from_config(&config).unwrap();

        assert_eq!(provider.compose_file, "custom-compose.yml");
        assert_eq!(provider.project_name, Some("test-project".to_string()));
    }
}
