use std::collections::HashMap;

use pmp_deploy::config::{EnvironmentConfig, InfrastructureConfig};
use pmp_deploy::infrastructure::{DeployMode, DeploymentContext, InfrastructureType};

#[test]
fn test_infrastructure_type_from_str() {
    assert_eq!(
        InfrastructureType::from_str("aws-eks"),
        InfrastructureType::AwsEks
    );
    assert_eq!(
        InfrastructureType::from_str("aws-ecs"),
        InfrastructureType::AwsEcs
    );
    assert_eq!(
        InfrastructureType::from_str("aws-lambda"),
        InfrastructureType::AwsLambda
    );
    assert_eq!(
        InfrastructureType::from_str("kubernetes"),
        InfrastructureType::Kubernetes
    );
    assert_eq!(
        InfrastructureType::from_str("docker-compose"),
        InfrastructureType::DockerCompose
    );
    assert_eq!(
        InfrastructureType::from_str("custom-provider"),
        InfrastructureType::Custom("custom-provider".to_string())
    );
}

#[test]
fn test_infrastructure_type_as_str() {
    assert_eq!(InfrastructureType::AwsEks.as_str(), "aws-eks");
    assert_eq!(InfrastructureType::AwsEcs.as_str(), "aws-ecs");
    assert_eq!(InfrastructureType::AwsLambda.as_str(), "aws-lambda");
    assert_eq!(InfrastructureType::Kubernetes.as_str(), "kubernetes");
    assert_eq!(InfrastructureType::DockerCompose.as_str(), "docker-compose");
    assert_eq!(
        InfrastructureType::Custom("my-provider".to_string()).as_str(),
        "my-provider"
    );
}

#[test]
fn test_infrastructure_type_serialization() {
    let infra_type = InfrastructureType::AwsEks;
    let serialized = serde_json::to_string(&infra_type).unwrap();
    assert_eq!(serialized, "\"aws-eks\"");

    let deserialized: InfrastructureType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, InfrastructureType::AwsEks);
}

#[test]
fn test_infrastructure_type_custom_serialization() {
    let infra_type = InfrastructureType::Custom("my-custom".to_string());
    let serialized = serde_json::to_string(&infra_type).unwrap();
    assert_eq!(serialized, "\"my-custom\"");

    let deserialized: InfrastructureType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        deserialized,
        InfrastructureType::Custom("my-custom".to_string())
    );
}

#[test]
fn test_deployment_context_creation() {
    let env_config = EnvironmentConfig {
        infrastructure: "aws-prod".to_string(),
        deployment_type: "rolling-update".to_string(),
        image: Some("myapp:v1.0.0".to_string()),
        replicas: Some(3),
        resources: None,
        env: HashMap::new(),
        environment: HashMap::new(),
        hooks: None,
        config: HashMap::new(),
    };

    let ctx = DeploymentContext {
        environment_name: "production".to_string(),
        environment: env_config.clone(),
        dry_run: false,
        verbose: true,
        deploy_mode: DeployMode::Full,
    };

    assert_eq!(ctx.environment_name, "production");
    assert_eq!(ctx.environment.infrastructure, "aws-prod");
    assert!(!ctx.dry_run);
    assert!(ctx.verbose);
    assert!(!ctx.deploy_mode.is_app_only());
}

#[test]
fn test_deployment_context_dry_run() {
    let env_config = EnvironmentConfig {
        infrastructure: "local".to_string(),
        deployment_type: "all-in".to_string(),
        image: Some("myapp:latest".to_string()),
        replicas: None,
        resources: None,
        env: HashMap::new(),
        environment: HashMap::new(),
        hooks: None,
        config: HashMap::new(),
    };

    let ctx = DeploymentContext {
        environment_name: "dev".to_string(),
        environment: env_config,
        dry_run: true,
        verbose: false,
        deploy_mode: DeployMode::AppOnly,
    };

    assert!(ctx.dry_run);
    assert!(!ctx.verbose);
    assert!(ctx.deploy_mode.is_app_only());
}

#[test]
fn test_infrastructure_config_parsing() {
    let yaml = r#"
type: aws-eks
config:
  cluster_name: my-cluster
  region: us-east-1
  namespace: production
"#;

    let config: InfrastructureConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure_type, "aws-eks");
    assert_eq!(
        config.config.get("cluster_name").unwrap(),
        &serde_yaml::Value::String("my-cluster".to_string())
    );
    assert_eq!(
        config.config.get("region").unwrap(),
        &serde_yaml::Value::String("us-east-1".to_string())
    );
}

#[test]
fn test_infrastructure_config_docker_compose() {
    let yaml = r#"
type: docker-compose
config:
  compose_file: docker-compose.yml
  project_name: myapp
  env_file: .env
"#;

    let config: InfrastructureConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure_type, "docker-compose");
    assert_eq!(
        config.config.get("compose_file").unwrap(),
        &serde_yaml::Value::String("docker-compose.yml".to_string())
    );
    assert_eq!(
        config.config.get("project_name").unwrap(),
        &serde_yaml::Value::String("myapp".to_string())
    );
}

#[test]
fn test_infrastructure_config_aws_ecs() {
    let yaml = r#"
type: aws-ecs
config:
  cluster: my-cluster
  region: us-west-2
  launch_type: FARGATE
  vpc_config:
    subnets:
      - subnet-123
      - subnet-456
    security_groups:
      - sg-789
"#;

    let config: InfrastructureConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure_type, "aws-ecs");
    assert!(config.config.contains_key("vpc_config"));
}

#[test]
fn test_infrastructure_config_aws_lambda() {
    let yaml = r#"
type: aws-lambda
config:
  function_name: my-function
  region: us-east-1
  runtime: provided.al2
  memory_mb: 512
  timeout_seconds: 30
  alias: live
"#;

    let config: InfrastructureConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure_type, "aws-lambda");
    assert_eq!(
        config.config.get("memory_mb").unwrap(),
        &serde_yaml::Value::Number(512.into())
    );
}

#[test]
fn test_infrastructure_config_kubernetes() {
    let yaml = r#"
type: kubernetes
config:
  context: my-k8s-context
  namespace: default
  kubeconfig_path: ~/.kube/config
"#;

    let config: InfrastructureConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure_type, "kubernetes");
    assert_eq!(
        config.config.get("context").unwrap(),
        &serde_yaml::Value::String("my-k8s-context".to_string())
    );
}

#[test]
fn test_environment_config_with_resources() {
    let yaml = r#"
infrastructure: aws-prod
deployment_type: rolling-update
image: myapp:v1.0.0
replicas: 5
resources:
  cpu: "500m"
  memory: "512Mi"
  cpu_limit: "1000m"
  memory_limit: "1Gi"
"#;

    let config: EnvironmentConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure, "aws-prod");
    assert_eq!(config.replicas, Some(5));

    let resources = config.resources.unwrap();
    assert_eq!(resources.cpu, Some("500m".to_string()));
    assert_eq!(resources.memory, Some("512Mi".to_string()));
}

#[test]
fn test_environment_config_with_env_vars() {
    let yaml = r#"
infrastructure: local
deployment_type: all-in
image: myapp:latest
env:
  NODE_ENV: production
  DATABASE_URL: postgres://localhost/db
  API_KEY: secret
"#;

    let config: EnvironmentConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.env.len(), 3);
    assert_eq!(
        config.env.get("NODE_ENV"),
        Some(&"production".to_string())
    );
}

#[test]
fn test_environment_config_minimal() {
    let yaml = r#"
infrastructure: local
"#;

    let config: EnvironmentConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure, "local");
    assert_eq!(config.deployment_type, "rolling-update");
    assert!(config.image.is_none());
    assert!(config.replicas.is_none());
    assert!(config.resources.is_none());
    assert!(config.env.is_empty());
}

#[test]
fn test_infrastructure_type_equality() {
    assert_eq!(InfrastructureType::AwsEks, InfrastructureType::AwsEks);
    assert_ne!(InfrastructureType::AwsEks, InfrastructureType::AwsEcs);
    assert_eq!(
        InfrastructureType::Custom("foo".to_string()),
        InfrastructureType::Custom("foo".to_string())
    );
    assert_ne!(
        InfrastructureType::Custom("foo".to_string()),
        InfrastructureType::Custom("bar".to_string())
    );
}

#[test]
fn test_infrastructure_type_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(InfrastructureType::AwsEks);
    set.insert(InfrastructureType::AwsEcs);
    set.insert(InfrastructureType::Custom("my-custom".to_string()));

    assert!(set.contains(&InfrastructureType::AwsEks));
    assert!(set.contains(&InfrastructureType::AwsEcs));
    assert!(set.contains(&InfrastructureType::Custom("my-custom".to_string())));
    assert!(!set.contains(&InfrastructureType::Kubernetes));
}
