use tempfile::TempDir;

use pmp_deploy::config::{Config, ConfigLoader, ConfigValidator, GlobalConfig};

#[test]
fn test_full_config_parsing() {
    let yaml = r#"
infrastructure:
  aws-prod:
    type: aws-eks
    config:
      cluster_name: production-cluster
      region: us-east-1
      namespace: production
      service_account: deploy-sa

  aws-staging:
    type: aws-ecs
    config:
      cluster: staging-cluster
      region: us-west-2
      launch_type: FARGATE

  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml
      project_name: myapp

environments:
  production:
    infrastructure: aws-prod
    deployment_type: rolling-update
    image: myapp:v2.0.0
    replicas: 5
    resources:
      cpu: "500m"
      memory: "512Mi"
      cpu_limit: "1000m"
      memory_limit: "1Gi"
    env:
      NODE_ENV: production
      LOG_LEVEL: info

  staging:
    infrastructure: aws-staging
    deployment_type: rolling-update
    image: myapp:v2.0.0-rc1
    replicas: 2
    config:
      batch_size: 25

  development:
    infrastructure: local
    deployment_type: all-in
    image: myapp:latest

secrets:
  provider: aws-secrets-manager
  secrets:
    db_password:
      key: prod/database/password
    api_key:
      provider: hashicorp-vault
      key: secret/data/api/key
      version: "2"
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure.len(), 3);
    assert_eq!(config.environments.len(), 3);

    let prod_env = config.get_environment("production").unwrap();
    assert_eq!(prod_env.infrastructure, "aws-prod");
    assert_eq!(prod_env.deployment_type, "rolling-update");
    assert_eq!(prod_env.replicas, Some(5));
    assert_eq!(prod_env.env.get("NODE_ENV"), Some(&"production".to_string()));

    let resources = prod_env.resources.as_ref().unwrap();
    assert_eq!(resources.cpu, Some("500m".to_string()));
    assert_eq!(resources.memory_limit, Some("1Gi".to_string()));

    let staging_env = config.get_environment("staging").unwrap();
    assert_eq!(staging_env.deployment_type, "rolling-update");

    let aws_prod = config.get_infrastructure("aws-prod").unwrap();
    assert_eq!(aws_prod.infrastructure_type, "aws-eks");
}

#[test]
fn test_minimal_config_parsing() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml

environments:
  dev:
    infrastructure: local
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.infrastructure.len(), 1);
    assert_eq!(config.environments.len(), 1);

    let dev_env = config.get_environment("dev").unwrap();
    assert_eq!(dev_env.deployment_type, "rolling-update");
    assert!(dev_env.image.is_none());
    assert!(dev_env.replicas.is_none());
}

#[test]
fn test_config_validation_valid() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml

environments:
  dev:
    infrastructure: local
    deployment_type: rolling-update
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let result = ConfigValidator::validate(&config);

    assert!(result.is_ok());
}

#[test]
fn test_config_validation_missing_infrastructure() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: nonexistent
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let result = ConfigValidator::validate(&config);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.message.contains("does not exist")));
}

#[test]
fn test_config_validation_invalid_deployment_type() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local
    deployment_type: invalid-type
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let result = ConfigValidator::validate(&config);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("Invalid deployment type")));
}

#[test]
fn test_config_loader_with_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".pmp-deploy.yaml");

    let config_content = r#"
infrastructure:
  test:
    type: docker-compose
    config:
      compose_file: docker-compose.yml

environments:
  test:
    infrastructure: test
    image: test:latest
"#;
    std::fs::write(&config_path, config_content).unwrap();

    let loader = ConfigLoader::new().with_project_config(config_path);
    let config = loader.load_project_config().unwrap();

    assert!(config.infrastructure.contains_key("test"));
    assert!(config.environments.contains_key("test"));
}

#[test]
fn test_config_loader_missing_file() {
    let loader =
        ConfigLoader::new().with_project_config(std::path::PathBuf::from("/nonexistent/path.yaml"));
    let result = loader.load_project_config();

    assert!(result.is_err());
}

#[test]
fn test_global_config_parsing() {
    let yaml = r#"
projects:
  - path: /home/user/project1
    name: Project One
  - path: /home/user/project2
  - path: $HOME/project3
    name: Project Three

defaults:
  deployment_type: rolling-update
"#;

    let config: GlobalConfig = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(config.projects.len(), 3);
    assert_eq!(config.projects[0].name, Some("Project One".to_string()));
    assert_eq!(config.projects[0].display_name(), "Project One");
    assert!(config.projects[1].name.is_none());
    assert_eq!(config.projects[1].display_name(), "project2");
}

#[test]
fn test_global_config_find_project() {
    let yaml = r#"
projects:
  - path: /home/user/myproject
    name: my-project
  - path: /home/user/other
"#;

    let config: GlobalConfig = serde_yaml::from_str(yaml).unwrap();

    let found = config.find_project("my-project");
    assert!(found.is_some());

    let found = config.find_project("other");
    assert!(found.is_some());

    let not_found = config.find_project("nonexistent");
    assert!(not_found.is_none());
}

#[test]
fn test_environment_config_list() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local
  staging:
    infrastructure: local
  production:
    infrastructure: local
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let envs = config.list_environments();

    assert_eq!(envs.len(), 3);
    assert!(envs.contains(&"dev"));
    assert!(envs.contains(&"staging"));
    assert!(envs.contains(&"production"));
}

#[test]
fn test_infrastructure_config_list() {
    let yaml = r#"
infrastructure:
  aws:
    type: aws-eks
  k8s:
    type: kubernetes
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let infras = config.list_infrastructures();

    assert_eq!(infras.len(), 3);
    assert!(infras.contains(&"aws"));
    assert!(infras.contains(&"k8s"));
    assert!(infras.contains(&"local"));
}

#[test]
fn test_secrets_config_parsing() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local

secrets:
  provider: aws-secrets-manager
  secrets:
    db_password:
      key: prod/db/password
    api_key:
      provider: hashicorp-vault
      key: secret/api
      version: "3"
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let secrets = config.secrets.as_ref().unwrap();

    assert_eq!(secrets.secrets.len(), 2);

    let db_secret = secrets.secrets.get("db_password").unwrap();
    assert_eq!(db_secret.key, "prod/db/password");
    assert!(db_secret.provider.is_none());

    let api_secret = secrets.secrets.get("api_key").unwrap();
    assert_eq!(api_secret.key, "secret/api");
    assert_eq!(api_secret.version, Some("3".to_string()));
}

#[test]
fn test_deployment_type_parsing() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  rolling:
    infrastructure: local
    deployment_type: rolling-update
  allin:
    infrastructure: local
    deployment_type: all-in
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(
        config.get_environment("rolling").unwrap().deployment_type,
        "rolling-update"
    );
    assert_eq!(
        config.get_environment("allin").unwrap().deployment_type,
        "all-in"
    );
}

#[test]
fn test_resource_config_parsing() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local
    resources:
      cpu: "250m"
      memory: "256Mi"
      cpu_limit: "500m"
      memory_limit: "512Mi"
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let env = config.get_environment("dev").unwrap();
    let resources = env.resources.as_ref().unwrap();

    assert_eq!(resources.cpu, Some("250m".to_string()));
    assert_eq!(resources.memory, Some("256Mi".to_string()));
    assert_eq!(resources.cpu_limit, Some("500m".to_string()));
    assert_eq!(resources.memory_limit, Some("512Mi".to_string()));
}

#[test]
fn test_env_vars_parsing() {
    let yaml = r#"
infrastructure:
  local:
    type: docker-compose

environments:
  dev:
    infrastructure: local
    env:
      DATABASE_URL: postgres://localhost/db
      API_KEY: secret123
      DEBUG: "true"
      PORT: "8080"
"#;

    let config: Config = serde_yaml::from_str(yaml).unwrap();
    let env = config.get_environment("dev").unwrap();

    assert_eq!(env.env.len(), 4);
    assert_eq!(
        env.env.get("DATABASE_URL"),
        Some(&"postgres://localhost/db".to_string())
    );
    assert_eq!(env.env.get("API_KEY"), Some(&"secret123".to_string()));
    assert_eq!(env.env.get("DEBUG"), Some(&"true".to_string()));
    assert_eq!(env.env.get("PORT"), Some(&"8080".to_string()));
}
