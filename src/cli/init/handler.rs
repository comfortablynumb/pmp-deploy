use anyhow::Result;

use super::wizard::{generate_config_from_selections, run_wizard, DialoguerPrompter};
use crate::cli::InitArgs;

/// Execute the init command
pub async fn execute(args: InitArgs) -> Result<()> {
    let config_path = std::env::current_dir()?.join(".pmp-deploy.yaml");

    if config_path.exists() && !args.force {
        anyhow::bail!(
            "Configuration file already exists. Use --force to overwrite."
        );
    }

    let template = generate_config(&args)?;

    std::fs::write(&config_path, template)?;
    println!("Created configuration file: {}", config_path.display());

    Ok(())
}

/// Generate configuration content based on args
fn generate_config(args: &InitArgs) -> Result<String> {
    if args.non_interactive || args.infrastructure.is_some() {
        let infra_type = args.infrastructure.as_deref().unwrap_or("docker-compose");
        Ok(generate_template(infra_type))
    } else {
        let prompter = DialoguerPrompter;
        let selections = run_wizard(&prompter)?;
        Ok(generate_config_from_selections(&selections))
    }
}

/// Generate a basic template for non-interactive mode
fn generate_template(infra_type: &str) -> String {
    match infra_type {
        "aws-eks" => generate_aws_eks_template(),
        "aws-ecs" => generate_aws_ecs_template(),
        "aws-lambda" => generate_aws_lambda_template(),
        "kubernetes" => generate_kubernetes_template(),
        _ => generate_docker_compose_template(),
    }
}

fn generate_docker_compose_template() -> String {
    r#"# pmp-deploy configuration
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml
      project_name: my-app

environments:
  development:
    infrastructure: local
    deployment_type: all-in
    image: my-app:latest
"#
    .to_string()
}

fn generate_aws_eks_template() -> String {
    r#"# pmp-deploy configuration
infrastructure:
  aws-dev:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1
      namespace: default

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:latest
    replicas: 2
"#
    .to_string()
}

fn generate_aws_ecs_template() -> String {
    r#"# pmp-deploy configuration
infrastructure:
  aws-dev:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      launch_type: FARGATE

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:latest
"#
    .to_string()
}

fn generate_aws_lambda_template() -> String {
    r#"# pmp-deploy configuration
infrastructure:
  aws-dev:
    type: aws-lambda
    config:
      region: us-east-1
      function_name_prefix: my-app

environments:
  development:
    infrastructure: aws-dev
    deployment_type: all-in
    image: my-app:latest
"#
    .to_string()
}

fn generate_kubernetes_template() -> String {
    r#"# pmp-deploy configuration
infrastructure:
  k8s-local:
    type: kubernetes
    config:
      context: my-context
      namespace: default

environments:
  development:
    infrastructure: k8s-local
    deployment_type: rolling-update
    image: my-app:latest
    replicas: 2
"#
    .to_string()
}
