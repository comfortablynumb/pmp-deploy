use std::collections::HashMap;

use super::types::{
    ConnectionDetails, EnvironmentSelection, InfrastructureTypeSelection,
};

/// Generate YAML configuration from user selections
pub fn generate_config_from_selections(selections: &[EnvironmentSelection]) -> String {
    let mut output = String::from("# pmp-deploy configuration\n\n");

    let infra_blocks = generate_infrastructure_blocks(selections);
    output.push_str(&infra_blocks);

    let env_blocks = generate_environment_blocks(selections);
    output.push_str(&env_blocks);

    output
}

/// Generate all infrastructure blocks, deduplicating by infrastructure name
fn generate_infrastructure_blocks(selections: &[EnvironmentSelection]) -> String {
    let mut output = String::from("infrastructure:\n");
    let mut seen: HashMap<String, bool> = HashMap::new();

    for selection in selections {
        let infra_name = get_infrastructure_name(selection);

        if seen.contains_key(&infra_name) {
            continue;
        }
        seen.insert(infra_name.clone(), true);

        let block = generate_infrastructure_block(selection, &infra_name);
        output.push_str(&block);
    }

    output.push('\n');
    output
}

/// Get infrastructure name based on environment and type
fn get_infrastructure_name(selection: &EnvironmentSelection) -> String {
    format!(
        "{}-{}",
        selection.name,
        selection.infrastructure_type.display_name().replace('-', "")
    )
}

/// Generate a single infrastructure block
fn generate_infrastructure_block(selection: &EnvironmentSelection, name: &str) -> String {
    let config_type = selection.infrastructure_type.config_type();
    let mut block = format!("  {}:\n    type: {}\n    config:\n", name, config_type);

    let config_lines = generate_infrastructure_config(selection);
    block.push_str(&config_lines);

    block
}

/// Generate infrastructure config based on type and connection details
fn generate_infrastructure_config(selection: &EnvironmentSelection) -> String {
    match &selection.connection_details {
        Some(details) => generate_app_only_config(details),
        None => generate_full_mode_config(selection.infrastructure_type),
    }
}

/// Generate config for AppOnly mode (with connection details)
fn generate_app_only_config(details: &ConnectionDetails) -> String {
    match details {
        ConnectionDetails::DockerCompose(d) => {
            format!(
                "      compose_file: {}\n      project_name: {}\n",
                d.compose_file, d.project_name
            )
        }
        ConnectionDetails::AwsEks(d) => {
            format!(
                "      cluster_name: {}\n      region: {}\n      namespace: {}\n",
                d.cluster_name, d.region, d.namespace
            )
        }
        ConnectionDetails::AwsEcs(d) => {
            format!(
                "      cluster: {}\n      region: {}\n      launch_type: {}\n",
                d.cluster, d.region, d.launch_type
            )
        }
        ConnectionDetails::AwsLambda(d) => {
            format!(
                "      region: {}\n      function_name_prefix: {}\n",
                d.region, d.function_name_prefix
            )
        }
        ConnectionDetails::Kubernetes(d) => {
            format!(
                "      context: {}\n      namespace: {}\n",
                d.context, d.namespace
            )
        }
    }
}

/// Generate config for Full mode (default/placeholder values)
fn generate_full_mode_config(infra_type: InfrastructureTypeSelection) -> String {
    match infra_type {
        InfrastructureTypeSelection::DockerCompose => {
            "      compose_file: docker-compose.yml\n      project_name: my-app\n".to_string()
        }
        InfrastructureTypeSelection::AwsEks => {
            "      cluster_name: my-cluster\n      region: us-east-1\n      namespace: default\n"
                .to_string()
        }
        InfrastructureTypeSelection::AwsEcs => {
            "      cluster: my-cluster\n      region: us-east-1\n      launch_type: FARGATE\n"
                .to_string()
        }
        InfrastructureTypeSelection::AwsLambda => {
            "      region: us-east-1\n      function_name_prefix: my-app\n".to_string()
        }
        InfrastructureTypeSelection::Kubernetes => {
            "      context: my-context\n      namespace: default\n".to_string()
        }
    }
}

/// Generate all environment blocks
fn generate_environment_blocks(selections: &[EnvironmentSelection]) -> String {
    let mut output = String::from("environments:\n");

    for selection in selections {
        let block = generate_environment_block(selection);
        output.push_str(&block);
    }

    output
}

/// Generate a single environment block
fn generate_environment_block(selection: &EnvironmentSelection) -> String {
    let infra_name = get_infrastructure_name(selection);
    let deployment_type = get_deployment_type(selection);
    let replicas = get_replicas(selection);

    let mut block = format!(
        "  {}:\n    infrastructure: {}\n    deployment_type: {}\n    image: my-app:latest\n",
        selection.name, infra_name, deployment_type
    );

    if let Some(r) = replicas {
        block.push_str(&format!("    replicas: {}\n", r));
    }

    block
}

/// Get deployment type based on infrastructure
fn get_deployment_type(selection: &EnvironmentSelection) -> &'static str {
    match selection.infrastructure_type {
        InfrastructureTypeSelection::DockerCompose => "all-in",
        _ => "rolling-update",
    }
}

/// Get replicas count if applicable
fn get_replicas(selection: &EnvironmentSelection) -> Option<u32> {
    match selection.infrastructure_type {
        InfrastructureTypeSelection::AwsEks | InfrastructureTypeSelection::Kubernetes => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::init::wizard::types::{DeployModeSelection, DockerComposeDetails};

    #[test]
    fn test_generate_config_single_env_full_mode() {
        let selections = vec![EnvironmentSelection {
            name: "development".to_string(),
            infrastructure_type: InfrastructureTypeSelection::DockerCompose,
            deploy_mode: DeployModeSelection::Full,
            connection_details: None,
        }];

        let config = generate_config_from_selections(&selections);

        assert!(config.contains("infrastructure:"));
        assert!(config.contains("type: docker-compose"));
        assert!(config.contains("environments:"));
        assert!(config.contains("development:"));
        assert!(config.contains("deployment_type: all-in"));
    }

    #[test]
    fn test_generate_config_app_only_mode() {
        let selections = vec![EnvironmentSelection {
            name: "staging".to_string(),
            infrastructure_type: InfrastructureTypeSelection::DockerCompose,
            deploy_mode: DeployModeSelection::AppOnly,
            connection_details: Some(ConnectionDetails::DockerCompose(DockerComposeDetails {
                compose_file: "./custom-compose.yml".to_string(),
                project_name: "myproject".to_string(),
            })),
        }];

        let config = generate_config_from_selections(&selections);

        assert!(config.contains("compose_file: ./custom-compose.yml"));
        assert!(config.contains("project_name: myproject"));
    }

    #[test]
    fn test_generate_config_multiple_environments() {
        let selections = vec![
            EnvironmentSelection {
                name: "development".to_string(),
                infrastructure_type: InfrastructureTypeSelection::DockerCompose,
                deploy_mode: DeployModeSelection::Full,
                connection_details: None,
            },
            EnvironmentSelection {
                name: "production".to_string(),
                infrastructure_type: InfrastructureTypeSelection::AwsEcs,
                deploy_mode: DeployModeSelection::Full,
                connection_details: None,
            },
        ];

        let config = generate_config_from_selections(&selections);

        assert!(config.contains("development:"));
        assert!(config.contains("production:"));
        assert!(config.contains("type: docker-compose"));
        assert!(config.contains("type: aws-ecs"));
    }

    #[test]
    fn test_eks_includes_replicas() {
        let selections = vec![EnvironmentSelection {
            name: "staging".to_string(),
            infrastructure_type: InfrastructureTypeSelection::AwsEks,
            deploy_mode: DeployModeSelection::Full,
            connection_details: None,
        }];

        let config = generate_config_from_selections(&selections);

        assert!(config.contains("replicas: 2"));
        assert!(config.contains("deployment_type: rolling-update"));
    }
}
