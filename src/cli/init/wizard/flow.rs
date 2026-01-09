use anyhow::Result;

use super::prompter::Prompter;
use super::types::{
    AwsEcsDetails, AwsEksDetails, AwsLambdaDetails, ConnectionDetails, DeployModeSelection,
    DockerComposeDetails, EnvironmentSelection, InfrastructureTypeSelection, KubernetesDetails,
    PREDEFINED_ENVIRONMENTS,
};

/// Run the interactive init wizard and return environment selections
pub fn run_wizard(prompter: &dyn Prompter) -> Result<Vec<EnvironmentSelection>> {
    let env_names = prompt_environment_selection(prompter)?;

    if env_names.is_empty() {
        anyhow::bail!("No environments selected. At least one environment is required.");
    }

    let mut selections = Vec::new();

    for env_name in env_names {
        println!("\n--- Configuring: {} ---", env_name);
        let selection = prompt_environment_config(prompter, env_name)?;
        selections.push(selection);
    }

    Ok(selections)
}

/// Prompt user to select environments (predefined + custom option)
fn prompt_environment_selection(prompter: &dyn Prompter) -> Result<Vec<String>> {
    let mut options: Vec<&str> = PREDEFINED_ENVIRONMENTS.to_vec();
    options.push("Add custom environment...");

    let indices = prompter.multi_select(
        "Select environments to configure (space to select, enter to confirm)",
        &options,
    )?;

    let mut env_names: Vec<String> = indices
        .iter()
        .filter(|&&i| i < PREDEFINED_ENVIRONMENTS.len())
        .map(|&i| PREDEFINED_ENVIRONMENTS[i].to_string())
        .collect();

    let custom_option_index = options.len() - 1;

    if indices.contains(&custom_option_index) {
        let custom_envs = prompt_custom_environments(prompter)?;
        env_names.extend(custom_envs);
    }

    Ok(env_names)
}

/// Prompt for custom environment names
fn prompt_custom_environments(prompter: &dyn Prompter) -> Result<Vec<String>> {
    let input = prompter.input(
        "Enter custom environment names (comma-separated)",
        Some(""),
    )?;

    let names: Vec<String> = input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(names)
}

/// Prompt for complete environment configuration
fn prompt_environment_config(
    prompter: &dyn Prompter,
    env_name: String,
) -> Result<EnvironmentSelection> {
    let infra_type = prompt_infrastructure_type(prompter, &env_name)?;
    let deploy_mode = prompt_deploy_mode(prompter, &env_name)?;

    let connection_details = if deploy_mode == DeployModeSelection::AppOnly {
        Some(prompt_connection_details(prompter, infra_type)?)
    } else {
        None
    };

    Ok(EnvironmentSelection {
        name: env_name,
        infrastructure_type: infra_type,
        deploy_mode,
        connection_details,
    })
}

/// Prompt for infrastructure type selection
fn prompt_infrastructure_type(
    prompter: &dyn Prompter,
    _env_name: &str,
) -> Result<InfrastructureTypeSelection> {
    let options: Vec<String> = InfrastructureTypeSelection::all()
        .iter()
        .map(|t| format!("{} - {}", t.display_name(), t.description()))
        .collect();

    let options_refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
    let index = prompter.select("Select infrastructure type", &options_refs, 0)?;

    Ok(InfrastructureTypeSelection::all()[index])
}

/// Prompt for deploy mode selection
fn prompt_deploy_mode(
    prompter: &dyn Prompter,
    _env_name: &str,
) -> Result<DeployModeSelection> {
    let options = [
        format!(
            "{} - {}",
            DeployModeSelection::Full.display_name(),
            DeployModeSelection::Full.description()
        ),
        format!(
            "{} - {}",
            DeployModeSelection::AppOnly.display_name(),
            DeployModeSelection::AppOnly.description()
        ),
    ];

    let options_refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
    let index = prompter.select("Select deploy mode", &options_refs, 0)?;

    if index == 0 {
        Ok(DeployModeSelection::Full)
    } else {
        Ok(DeployModeSelection::AppOnly)
    }
}

/// Dispatch to infrastructure-specific connection details prompts
fn prompt_connection_details(
    prompter: &dyn Prompter,
    infra_type: InfrastructureTypeSelection,
) -> Result<ConnectionDetails> {
    match infra_type {
        InfrastructureTypeSelection::DockerCompose => {
            Ok(ConnectionDetails::DockerCompose(prompt_docker_compose_details(prompter)?))
        }
        InfrastructureTypeSelection::AwsEks => {
            Ok(ConnectionDetails::AwsEks(prompt_aws_eks_details(prompter)?))
        }
        InfrastructureTypeSelection::AwsEcs => {
            Ok(ConnectionDetails::AwsEcs(prompt_aws_ecs_details(prompter)?))
        }
        InfrastructureTypeSelection::AwsLambda => {
            Ok(ConnectionDetails::AwsLambda(prompt_aws_lambda_details(prompter)?))
        }
        InfrastructureTypeSelection::Kubernetes => {
            Ok(ConnectionDetails::Kubernetes(prompt_kubernetes_details(prompter)?))
        }
    }
}

/// Prompt for Docker Compose connection details
fn prompt_docker_compose_details(prompter: &dyn Prompter) -> Result<DockerComposeDetails> {
    let compose_file = prompter.input("Compose file path", Some("./docker-compose.yml"))?;
    let project_name = prompter.input("Project name", Some("myapp"))?;

    Ok(DockerComposeDetails {
        compose_file,
        project_name,
    })
}

/// Prompt for AWS EKS connection details
fn prompt_aws_eks_details(prompter: &dyn Prompter) -> Result<AwsEksDetails> {
    let cluster_name = prompter.input("Cluster name", Some("my-cluster"))?;
    let region = prompter.input("AWS region", Some("us-east-1"))?;
    let namespace = prompter.input("Kubernetes namespace", Some("default"))?;

    Ok(AwsEksDetails {
        cluster_name,
        region,
        namespace,
    })
}

/// Prompt for AWS ECS connection details
fn prompt_aws_ecs_details(prompter: &dyn Prompter) -> Result<AwsEcsDetails> {
    let cluster = prompter.input("Cluster name", Some("my-cluster"))?;
    let region = prompter.input("AWS region", Some("us-east-1"))?;

    let launch_type_options = ["FARGATE", "EC2"];
    let launch_type_index = prompter.select("Launch type", &launch_type_options, 0)?;
    let launch_type = launch_type_options[launch_type_index].to_string();

    Ok(AwsEcsDetails {
        cluster,
        region,
        launch_type,
    })
}

/// Prompt for AWS Lambda connection details
fn prompt_aws_lambda_details(prompter: &dyn Prompter) -> Result<AwsLambdaDetails> {
    let region = prompter.input("AWS region", Some("us-east-1"))?;
    let function_name_prefix = prompter.input("Function name prefix", Some("myapp"))?;

    Ok(AwsLambdaDetails {
        region,
        function_name_prefix,
    })
}

/// Prompt for Kubernetes connection details
fn prompt_kubernetes_details(prompter: &dyn Prompter) -> Result<KubernetesDetails> {
    let context = prompter.input("Kubernetes context", Some("default"))?;
    let namespace = prompter.input("Namespace", Some("default"))?;

    Ok(KubernetesDetails { context, namespace })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::init::wizard::prompter::tests::MockPrompter;

    #[test]
    fn test_run_wizard_single_env() {
        let prompter = MockPrompter::new()
            .with_multi_select_responses(vec![vec![0]]) // Select development
            .with_select_responses(vec![
                0, // docker-compose
                0, // Full mode
            ]);

        let result = run_wizard(&prompter).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "development");
        assert_eq!(
            result[0].infrastructure_type,
            InfrastructureTypeSelection::DockerCompose
        );
        assert_eq!(result[0].deploy_mode, DeployModeSelection::Full);
        assert!(result[0].connection_details.is_none());
    }

    #[test]
    fn test_run_wizard_app_only_mode() {
        let prompter = MockPrompter::new()
            .with_multi_select_responses(vec![vec![0]]) // Select development
            .with_select_responses(vec![
                0, // docker-compose
                1, // AppOnly mode
            ])
            .with_input_responses(vec![
                "./docker-compose.yml".to_string(),
                "myproject".to_string(),
            ]);

        let result = run_wizard(&prompter).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].deploy_mode, DeployModeSelection::AppOnly);
        assert!(result[0].connection_details.is_some());

        if let Some(ConnectionDetails::DockerCompose(details)) = &result[0].connection_details {
            assert_eq!(details.compose_file, "./docker-compose.yml");
            assert_eq!(details.project_name, "myproject");
        } else {
            panic!("Expected DockerCompose connection details");
        }
    }

    #[test]
    fn test_run_wizard_no_environments_error() {
        let prompter = MockPrompter::new()
            .with_multi_select_responses(vec![vec![]]); // Select nothing

        let result = run_wizard(&prompter);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No environments selected"));
    }
}
