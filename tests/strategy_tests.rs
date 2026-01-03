use std::collections::HashMap;

use pmp_deploy::config::EnvironmentConfig;
use pmp_deploy::deployment::{
    DeploymentResult, DeploymentStrategy, DeploymentType, RollingUpdateStrategy,
    StrategyConfig, StrategyFactory,
};

#[test]
fn test_strategy_factory_creates_rolling_update() {
    let strategy = StrategyFactory::create(&DeploymentType::RollingUpdate);
    assert_eq!(strategy.deployment_type(), DeploymentType::RollingUpdate);
}

#[test]
fn test_strategy_factory_all_in_uses_rolling() {
    let strategy = StrategyFactory::create(&DeploymentType::AllIn);
    assert_eq!(strategy.deployment_type(), DeploymentType::RollingUpdate);
}

#[test]
fn test_strategy_factory_create_config() {
    let env = EnvironmentConfig {
        infrastructure: "test".to_string(),
        deployment_type: "rolling-update".to_string(),
        image: Some("test:latest".to_string()),
        replicas: Some(3),
        resources: None,
        env: HashMap::new(),
        environment: HashMap::new(),
        hooks: None,
        config: HashMap::new(),
    };

    let config = StrategyFactory::create_config(&env);
    assert_eq!(config.deployment_type, DeploymentType::RollingUpdate);
    assert_eq!(config.batch_size, Some(25));
}

#[test]
fn test_strategy_factory_create_config_all_in() {
    let env = EnvironmentConfig {
        infrastructure: "test".to_string(),
        deployment_type: "all-in".to_string(),
        image: Some("test:latest".to_string()),
        replicas: Some(3),
        resources: None,
        env: HashMap::new(),
        environment: HashMap::new(),
        hooks: None,
        config: HashMap::new(),
    };

    let config = StrategyFactory::create_config(&env);
    assert_eq!(config.deployment_type, DeploymentType::AllIn);
    assert_eq!(config.batch_size, Some(100));
}

#[test]
fn test_strategy_factory_create_config_with_custom_batch() {
    let mut custom_config = HashMap::new();
    custom_config.insert(
        "batch_size".to_string(),
        serde_yaml::Value::Number(serde_yaml::Number::from(50)),
    );

    let env = EnvironmentConfig {
        infrastructure: "test".to_string(),
        deployment_type: "rolling-update".to_string(),
        image: Some("test:latest".to_string()),
        replicas: Some(3),
        resources: None,
        env: HashMap::new(),
        environment: HashMap::new(),
        hooks: None,
        config: custom_config,
    };

    let config = StrategyFactory::create_config(&env);
    assert_eq!(config.batch_size, Some(50));
}

#[test]
fn test_deployment_type_from_str() {
    assert_eq!(
        DeploymentType::from_str("rolling-update"),
        Some(DeploymentType::RollingUpdate)
    );
    assert_eq!(
        DeploymentType::from_str("all-in"),
        Some(DeploymentType::AllIn)
    );
    assert_eq!(DeploymentType::from_str("recreate"), None);
    assert_eq!(DeploymentType::from_str("direct"), None);
    assert_eq!(DeploymentType::from_str("unknown"), None);
    assert_eq!(DeploymentType::from_str("canary"), None);
    assert_eq!(DeploymentType::from_str("blue-green"), None);
}

#[test]
fn test_deployment_type_as_str() {
    assert_eq!(DeploymentType::RollingUpdate.as_str(), "rolling-update");
    assert_eq!(DeploymentType::AllIn.as_str(), "all-in");
}

#[test]
fn test_deployment_result_success() {
    let result = DeploymentResult::success("Deployment completed");

    assert!(result.success);
    assert_eq!(result.message, "Deployment completed");
    assert!(result.version.is_none());
    assert!(result.rollback_version.is_none());
}

#[test]
fn test_deployment_result_failure() {
    let result = DeploymentResult::failure("Deployment failed: timeout");

    assert!(!result.success);
    assert_eq!(result.message, "Deployment failed: timeout");
}

#[test]
fn test_deployment_result_with_version() {
    let result = DeploymentResult::success("Deployed")
        .with_version("v1.2.3")
        .with_rollback_version("v1.2.2");

    assert!(result.success);
    assert_eq!(result.version, Some("v1.2.3".to_string()));
    assert_eq!(result.rollback_version, Some("v1.2.2".to_string()));
}

#[test]
fn test_strategy_config_default() {
    let config = StrategyConfig::default();

    assert_eq!(config.deployment_type, DeploymentType::RollingUpdate);
    assert_eq!(config.batch_size, Some(25));
    assert_eq!(config.health_check_timeout_secs, Some(300));
    assert!(config.rollback_on_failure);
}

#[test]
fn test_rolling_update_strategy_validate_config() {
    let strategy = RollingUpdateStrategy::new();

    let valid_config = StrategyConfig {
        deployment_type: DeploymentType::RollingUpdate,
        batch_size: Some(25),
        health_check_timeout_secs: Some(300),
        rollback_on_failure: true,
    };

    assert!(strategy.validate_config(&valid_config).is_ok());
}

#[test]
fn test_rolling_update_strategy_validate_config_invalid_batch() {
    let strategy = RollingUpdateStrategy::new();

    let invalid_config = StrategyConfig {
        deployment_type: DeploymentType::RollingUpdate,
        batch_size: Some(0),
        health_check_timeout_secs: Some(300),
        rollback_on_failure: true,
    };

    assert!(strategy.validate_config(&invalid_config).is_err());

    let invalid_config_high = StrategyConfig {
        deployment_type: DeploymentType::RollingUpdate,
        batch_size: Some(101),
        health_check_timeout_secs: Some(300),
        rollback_on_failure: true,
    };

    assert!(strategy.validate_config(&invalid_config_high).is_err());
}
