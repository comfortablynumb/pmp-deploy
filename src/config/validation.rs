use super::schema::Config;
use crate::deployment::DeploymentType;
use crate::infrastructure::InfrastructureType;

#[derive(Debug)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate(config: &Config) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        Self::validate_infrastructures(config, &mut errors);
        Self::validate_environments(config, &mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_infrastructures(config: &Config, errors: &mut Vec<ValidationError>) {
        for (name, infra) in &config.infrastructure {
            let infra_type = InfrastructureType::from_str(&infra.infrastructure_type);

            match infra_type {
                InfrastructureType::AwsEks => {
                    Self::validate_aws_eks_config(name, infra, errors);
                }
                InfrastructureType::AwsEcs => {
                    Self::validate_aws_ecs_config(name, infra, errors);
                }
                InfrastructureType::AwsLambda => {
                    Self::validate_aws_lambda_config(name, infra, errors);
                }
                InfrastructureType::Kubernetes => {
                    Self::validate_k8s_config(name, infra, errors);
                }
                InfrastructureType::DockerCompose => {
                    Self::validate_docker_compose_config(name, infra, errors);
                }
                InfrastructureType::Custom(_) => {
                    // Custom types are validated by plugins
                }
            }
        }
    }

    fn validate_environments(config: &Config, errors: &mut Vec<ValidationError>) {
        for (name, env) in &config.environments {
            if !config.infrastructure.contains_key(&env.infrastructure) {
                errors.push(ValidationError {
                    field: format!("environments.{}.infrastructure", name),
                    message: format!(
                        "Referenced infrastructure '{}' does not exist",
                        env.infrastructure
                    ),
                });
            }

            if DeploymentType::from_str(&env.deployment_type).is_none() {
                errors.push(ValidationError {
                    field: format!("environments.{}.deployment_type", name),
                    message: format!(
                        "Invalid deployment type '{}'. Valid options: rolling-update, all-in",
                        env.deployment_type
                    ),
                });
            }
        }
    }

    fn validate_aws_eks_config(
        name: &str,
        infra: &super::schema::InfrastructureConfig,
        errors: &mut Vec<ValidationError>,
    ) {
        let required_fields = ["cluster_name", "region"];

        for field in required_fields {
            if !infra.config.contains_key(field) {
                errors.push(ValidationError {
                    field: format!("infrastructure.{}.config.{}", name, field),
                    message: format!("Required field '{}' is missing for aws-eks", field),
                });
            }
        }
    }

    fn validate_aws_ecs_config(
        name: &str,
        infra: &super::schema::InfrastructureConfig,
        errors: &mut Vec<ValidationError>,
    ) {
        let required_fields = ["cluster", "region"];

        for field in required_fields {
            if !infra.config.contains_key(field) {
                errors.push(ValidationError {
                    field: format!("infrastructure.{}.config.{}", name, field),
                    message: format!("Required field '{}' is missing for aws-ecs", field),
                });
            }
        }
    }

    fn validate_aws_lambda_config(
        name: &str,
        infra: &super::schema::InfrastructureConfig,
        errors: &mut Vec<ValidationError>,
    ) {
        let required_fields = ["region"];

        for field in required_fields {
            if !infra.config.contains_key(field) {
                errors.push(ValidationError {
                    field: format!("infrastructure.{}.config.{}", name, field),
                    message: format!("Required field '{}' is missing for aws-lambda", field),
                });
            }
        }
    }

    fn validate_k8s_config(
        name: &str,
        infra: &super::schema::InfrastructureConfig,
        errors: &mut Vec<ValidationError>,
    ) {
        let has_kubeconfig = infra.config.contains_key("kubeconfig_path");
        let has_context = infra.config.contains_key("context");

        if !has_kubeconfig && !has_context {
            errors.push(ValidationError {
                field: format!("infrastructure.{}.config", name),
                message: "Kubernetes config requires either 'kubeconfig_path' or 'context'"
                    .to_string(),
            });
        }
    }

    fn validate_docker_compose_config(
        name: &str,
        infra: &super::schema::InfrastructureConfig,
        errors: &mut Vec<ValidationError>,
    ) {
        if !infra.config.contains_key("compose_file") {
            errors.push(ValidationError {
                field: format!("infrastructure.{}.config.compose_file", name),
                message: "Required field 'compose_file' is missing for docker-compose".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_validate_missing_infrastructure_reference() {
        let config = Config {
            infrastructure: HashMap::new(),
            environments: HashMap::from([(
                "dev".to_string(),
                super::super::schema::EnvironmentConfig {
                    infrastructure: "nonexistent".to_string(),
                    deployment_type: "rolling-update".to_string(),
                    image: None,
                    replicas: None,
                    resources: None,
                    env: HashMap::new(),
                    environment: HashMap::new(),
                    hooks: None,
                    config: HashMap::new(),
                },
            )]),
            secrets: None,
            metrics: None,
        };

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("does not exist")));
    }

    #[test]
    fn test_validate_invalid_deployment_type() {
        let mut infrastructure = HashMap::new();
        infrastructure.insert(
            "local".to_string(),
            super::super::schema::InfrastructureConfig {
                infrastructure_type: "docker-compose".to_string(),
                config: HashMap::from([("compose_file".to_string(), serde_yaml::Value::String("docker-compose.yml".to_string()))]),
            },
        );

        let config = Config {
            infrastructure,
            environments: HashMap::from([(
                "dev".to_string(),
                super::super::schema::EnvironmentConfig {
                    infrastructure: "local".to_string(),
                    deployment_type: "invalid-type".to_string(),
                    image: None,
                    replicas: None,
                    resources: None,
                    env: HashMap::new(),
                    environment: HashMap::new(),
                    hooks: None,
                    config: HashMap::new(),
                },
            )]),
            secrets: None,
            metrics: None,
        };

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());
    }
}
