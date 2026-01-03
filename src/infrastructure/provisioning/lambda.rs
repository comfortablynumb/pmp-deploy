use serde::{Deserialize, Serialize};

/// Configuration for fully provisioning a Lambda function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaProvisioningConfig {
    /// Runtime environment (e.g., "provided.al2023", "nodejs20.x", "python3.12")
    #[serde(default)]
    pub runtime: Option<String>,

    /// Handler function (e.g., "bootstrap", "index.handler")
    #[serde(default)]
    pub handler: Option<String>,

    /// Memory size in MB (128-10240)
    #[serde(default = "default_memory_mb")]
    pub memory_mb: i32,

    /// Timeout in seconds (1-900)
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: i32,

    /// IAM role ARN for Lambda execution
    pub role_arn: String,

    /// Description of the function
    #[serde(default)]
    pub description: Option<String>,

    /// VPC configuration for Lambda
    #[serde(default)]
    pub vpc: Option<LambdaVpcConfig>,

    /// Lambda layers ARNs
    #[serde(default)]
    pub layers: Vec<String>,

    /// Dead letter queue configuration
    #[serde(default)]
    pub dead_letter_queue: Option<DeadLetterConfig>,

    /// Architecture (x86_64 or arm64)
    #[serde(default = "default_architecture")]
    pub architecture: String,

    /// Ephemeral storage size in MB (512-10240)
    #[serde(default = "default_ephemeral_storage")]
    pub ephemeral_storage_mb: i32,

    /// Tracing configuration (Active or PassThrough)
    #[serde(default)]
    pub tracing_mode: Option<String>,

    /// Package type (Image or Zip)
    #[serde(default = "default_package_type")]
    pub package_type: String,
}

fn default_memory_mb() -> i32 {
    256
}

fn default_timeout_secs() -> i32 {
    30
}

fn default_architecture() -> String {
    "x86_64".to_string()
}

fn default_ephemeral_storage() -> i32 {
    512
}

fn default_package_type() -> String {
    "Image".to_string()
}

impl Default for LambdaProvisioningConfig {
    fn default() -> Self {
        Self {
            runtime: None,
            handler: None,
            memory_mb: default_memory_mb(),
            timeout_secs: default_timeout_secs(),
            role_arn: String::new(),
            description: None,
            vpc: None,
            layers: vec![],
            dead_letter_queue: None,
            architecture: default_architecture(),
            ephemeral_storage_mb: default_ephemeral_storage(),
            tracing_mode: None,
            package_type: default_package_type(),
        }
    }
}

impl LambdaProvisioningConfig {
    pub fn from_yaml_value(value: &serde_yaml::Value) -> Option<Self> {
        serde_yaml::from_value(value.clone()).ok()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.role_arn.is_empty() {
            anyhow::bail!("Lambda provisioning requires role_arn to be specified");
        }

        if self.memory_mb < 128 || self.memory_mb > 10240 {
            anyhow::bail!("Lambda memory_mb must be between 128 and 10240");
        }

        if self.timeout_secs < 1 || self.timeout_secs > 900 {
            anyhow::bail!("Lambda timeout_secs must be between 1 and 900");
        }

        if self.ephemeral_storage_mb < 512 || self.ephemeral_storage_mb > 10240 {
            anyhow::bail!("Lambda ephemeral_storage_mb must be between 512 and 10240");
        }

        let valid_architectures = ["x86_64", "arm64"];
        if !valid_architectures.contains(&self.architecture.as_str()) {
            anyhow::bail!(
                "Lambda architecture must be one of: {}",
                valid_architectures.join(", ")
            );
        }

        let valid_package_types = ["Image", "Zip"];
        if !valid_package_types.contains(&self.package_type.as_str()) {
            anyhow::bail!(
                "Lambda package_type must be one of: {}",
                valid_package_types.join(", ")
            );
        }

        if let Some(vpc) = &self.vpc {
            vpc.validate()?;
        }

        Ok(())
    }
}

/// VPC configuration for Lambda function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaVpcConfig {
    /// Subnet IDs for the Lambda function
    pub subnet_ids: Vec<String>,

    /// Security group IDs for the Lambda function
    pub security_group_ids: Vec<String>,
}

impl LambdaVpcConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.subnet_ids.is_empty() {
            anyhow::bail!("VPC configuration requires at least one subnet_id");
        }

        if self.security_group_ids.is_empty() {
            anyhow::bail!("VPC configuration requires at least one security_group_id");
        }

        Ok(())
    }
}

/// Dead letter queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterConfig {
    /// ARN of the SQS queue or SNS topic
    pub target_arn: String,
}

/// Represents the current state of a Lambda function for comparison
#[derive(Debug, Clone)]
pub struct LambdaCurrentState {
    pub exists: bool,
    pub runtime: Option<String>,
    pub handler: Option<String>,
    pub memory_size: Option<i32>,
    pub timeout: Option<i32>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub vpc_config: Option<LambdaVpcConfig>,
    pub layers: Vec<String>,
    pub architecture: Option<String>,
    pub ephemeral_storage: Option<i32>,
    pub package_type: Option<String>,
}

impl LambdaCurrentState {
    pub fn not_found() -> Self {
        Self {
            exists: false,
            runtime: None,
            handler: None,
            memory_size: None,
            timeout: None,
            role: None,
            description: None,
            vpc_config: None,
            layers: vec![],
            architecture: None,
            ephemeral_storage: None,
            package_type: None,
        }
    }
}

/// Differences between current and desired state
#[derive(Debug, Clone)]
pub struct LambdaStateDiff {
    pub needs_create: bool,
    pub memory_changed: bool,
    pub timeout_changed: bool,
    pub role_changed: bool,
    pub description_changed: bool,
    pub vpc_changed: bool,
    pub layers_changed: bool,
    pub architecture_changed: bool,
    pub ephemeral_storage_changed: bool,
    pub changes: Vec<(String, String, String)>, // (field, current, desired)
}

impl LambdaStateDiff {
    pub fn needs_update(&self) -> bool {
        self.memory_changed
            || self.timeout_changed
            || self.role_changed
            || self.description_changed
            || self.vpc_changed
            || self.layers_changed
            || self.ephemeral_storage_changed
    }

    pub fn has_immutable_changes(&self) -> bool {
        self.architecture_changed
    }
}

pub fn compute_lambda_diff(
    current: &LambdaCurrentState,
    desired: &LambdaProvisioningConfig,
) -> LambdaStateDiff {
    if !current.exists {
        return LambdaStateDiff {
            needs_create: true,
            memory_changed: false,
            timeout_changed: false,
            role_changed: false,
            description_changed: false,
            vpc_changed: false,
            layers_changed: false,
            architecture_changed: false,
            ephemeral_storage_changed: false,
            changes: vec![],
        };
    }

    let mut changes = Vec::new();

    let memory_changed = current.memory_size != Some(desired.memory_mb);
    if memory_changed {
        changes.push((
            "memory_mb".to_string(),
            current.memory_size.map(|v| v.to_string()).unwrap_or_default(),
            desired.memory_mb.to_string(),
        ));
    }

    let timeout_changed = current.timeout != Some(desired.timeout_secs);
    if timeout_changed {
        changes.push((
            "timeout_secs".to_string(),
            current.timeout.map(|v| v.to_string()).unwrap_or_default(),
            desired.timeout_secs.to_string(),
        ));
    }

    let role_changed = current.role.as_ref() != Some(&desired.role_arn);
    if role_changed {
        changes.push((
            "role_arn".to_string(),
            current.role.clone().unwrap_or_default(),
            desired.role_arn.clone(),
        ));
    }

    let description_changed = current.description != desired.description;
    if description_changed {
        changes.push((
            "description".to_string(),
            current.description.clone().unwrap_or_default(),
            desired.description.clone().unwrap_or_default(),
        ));
    }

    let vpc_changed = match (&current.vpc_config, &desired.vpc) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(curr), Some(des)) => {
            curr.subnet_ids != des.subnet_ids || curr.security_group_ids != des.security_group_ids
        }
    };

    let layers_changed = current.layers != desired.layers;

    let architecture_changed = current.architecture.as_ref() != Some(&desired.architecture);

    let ephemeral_storage_changed =
        current.ephemeral_storage != Some(desired.ephemeral_storage_mb);
    if ephemeral_storage_changed {
        changes.push((
            "ephemeral_storage_mb".to_string(),
            current
                .ephemeral_storage
                .map(|v| v.to_string())
                .unwrap_or_default(),
            desired.ephemeral_storage_mb.to_string(),
        ));
    }

    LambdaStateDiff {
        needs_create: false,
        memory_changed,
        timeout_changed,
        role_changed,
        description_changed,
        vpc_changed,
        layers_changed,
        architecture_changed,
        ephemeral_storage_changed,
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lambda_provisioning_config_defaults() {
        let config = LambdaProvisioningConfig::default();

        assert_eq!(config.memory_mb, 256);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.architecture, "x86_64");
        assert_eq!(config.ephemeral_storage_mb, 512);
        assert_eq!(config.package_type, "Image");
    }

    #[test]
    fn test_lambda_provisioning_config_validation() {
        let mut config = LambdaProvisioningConfig::default();

        // Missing role_arn
        assert!(config.validate().is_err());

        config.role_arn = "arn:aws:iam::123456789:role/lambda-exec".to_string();
        assert!(config.validate().is_ok());

        // Invalid memory
        config.memory_mb = 50;
        assert!(config.validate().is_err());
        config.memory_mb = 256;

        // Invalid timeout
        config.timeout_secs = 1000;
        assert!(config.validate().is_err());
        config.timeout_secs = 30;

        // Invalid architecture
        config.architecture = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_lambda_vpc_config_validation() {
        let vpc = LambdaVpcConfig {
            subnet_ids: vec![],
            security_group_ids: vec!["sg-123".to_string()],
        };
        assert!(vpc.validate().is_err());

        let vpc = LambdaVpcConfig {
            subnet_ids: vec!["subnet-123".to_string()],
            security_group_ids: vec![],
        };
        assert!(vpc.validate().is_err());

        let vpc = LambdaVpcConfig {
            subnet_ids: vec!["subnet-123".to_string()],
            security_group_ids: vec!["sg-123".to_string()],
        };
        assert!(vpc.validate().is_ok());
    }

    #[test]
    fn test_compute_lambda_diff_needs_create() {
        let current = LambdaCurrentState::not_found();
        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(diff.needs_create);
        assert!(!diff.needs_update());
    }

    #[test]
    fn test_compute_lambda_diff_memory_changed() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            ..LambdaCurrentState::not_found()
        };

        let desired = LambdaProvisioningConfig {
            memory_mb: 512,
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(!diff.needs_create);
        assert!(diff.memory_changed);
        assert!(diff.needs_update());
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].0, "memory_mb");
    }

    #[test]
    fn test_from_yaml_value() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
runtime: provided.al2023
handler: bootstrap
memory_mb: 512
timeout_secs: 60
role_arn: arn:aws:iam::123456789:role/lambda-exec
architecture: arm64
vpc:
  subnet_ids:
    - subnet-abc123
  security_group_ids:
    - sg-xyz789
"#,
        )
        .unwrap();

        let config = LambdaProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert_eq!(config.runtime, Some("provided.al2023".to_string()));
        assert_eq!(config.handler, Some("bootstrap".to_string()));
        assert_eq!(config.memory_mb, 512);
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.architecture, "arm64");
        assert!(config.vpc.is_some());

        let vpc = config.vpc.unwrap();
        assert_eq!(vpc.subnet_ids, vec!["subnet-abc123"]);
        assert_eq!(vpc.security_group_ids, vec!["sg-xyz789"]);
    }

    #[test]
    fn test_compute_lambda_diff_no_changes() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            description: None,
            runtime: None,
            handler: None,
            vpc_config: None,
            layers: vec![],
            package_type: Some("Image".to_string()),
        };

        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(!diff.needs_create);
        assert!(!diff.needs_update());
        assert!(!diff.has_immutable_changes());
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn test_compute_lambda_diff_multiple_changes() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/old-role".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            description: Some("Old description".to_string()),
            runtime: None,
            handler: None,
            vpc_config: None,
            layers: vec![],
            package_type: Some("Image".to_string()),
        };

        let desired = LambdaProvisioningConfig {
            memory_mb: 1024,
            timeout_secs: 60,
            role_arn: "arn:aws:iam::123456789:role/new-role".to_string(),
            description: Some("New description".to_string()),
            ephemeral_storage_mb: 1024,
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(!diff.needs_create);
        assert!(diff.memory_changed);
        assert!(diff.timeout_changed);
        assert!(diff.role_changed);
        assert!(diff.description_changed);
        assert!(diff.ephemeral_storage_changed);
        assert!(diff.needs_update());
        assert_eq!(diff.changes.len(), 5);
    }

    #[test]
    fn test_compute_lambda_diff_vpc_added() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            vpc_config: None,
            ..LambdaCurrentState::not_found()
        };

        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            vpc: Some(LambdaVpcConfig {
                subnet_ids: vec!["subnet-123".to_string()],
                security_group_ids: vec!["sg-456".to_string()],
            }),
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(diff.vpc_changed);
        assert!(diff.needs_update());
    }

    #[test]
    fn test_compute_lambda_diff_vpc_removed() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            vpc_config: Some(LambdaVpcConfig {
                subnet_ids: vec!["subnet-123".to_string()],
                security_group_ids: vec!["sg-456".to_string()],
            }),
            ..LambdaCurrentState::not_found()
        };

        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            vpc: None,
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(diff.vpc_changed);
        assert!(diff.needs_update());
    }

    #[test]
    fn test_compute_lambda_diff_layers_changed() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            layers: vec!["arn:aws:lambda:us-east-1:123:layer:old:1".to_string()],
            ..LambdaCurrentState::not_found()
        };

        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            layers: vec!["arn:aws:lambda:us-east-1:123:layer:new:1".to_string()],
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(diff.layers_changed);
        assert!(diff.needs_update());
    }

    #[test]
    fn test_compute_lambda_diff_architecture_changed() {
        let current = LambdaCurrentState {
            exists: true,
            memory_size: Some(256),
            timeout: Some(30),
            role: Some("arn:aws:iam::123456789:role/lambda-exec".to_string()),
            architecture: Some("x86_64".to_string()),
            ephemeral_storage: Some(512),
            ..LambdaCurrentState::not_found()
        };

        let desired = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            architecture: "arm64".to_string(),
            ..Default::default()
        };

        let diff = compute_lambda_diff(&current, &desired);

        assert!(diff.architecture_changed);
        assert!(diff.has_immutable_changes());
        // Architecture change alone doesn't trigger needs_update (it's immutable)
        assert!(!diff.needs_update());
    }

    #[test]
    fn test_lambda_state_diff_needs_update() {
        let diff = LambdaStateDiff {
            needs_create: false,
            memory_changed: false,
            timeout_changed: false,
            role_changed: false,
            description_changed: false,
            vpc_changed: false,
            layers_changed: false,
            architecture_changed: false,
            ephemeral_storage_changed: false,
            changes: vec![],
        };
        assert!(!diff.needs_update());

        let diff_with_memory = LambdaStateDiff {
            memory_changed: true,
            ..diff.clone()
        };
        assert!(diff_with_memory.needs_update());

        let diff_with_vpc = LambdaStateDiff {
            vpc_changed: true,
            ..diff.clone()
        };
        assert!(diff_with_vpc.needs_update());
    }

    #[test]
    fn test_lambda_validation_edge_cases() {
        let mut config = LambdaProvisioningConfig {
            role_arn: "arn:aws:iam::123456789:role/lambda-exec".to_string(),
            ..Default::default()
        };

        // Test memory boundaries
        config.memory_mb = 128;
        assert!(config.validate().is_ok());

        config.memory_mb = 10240;
        assert!(config.validate().is_ok());

        config.memory_mb = 127;
        assert!(config.validate().is_err());

        config.memory_mb = 10241;
        assert!(config.validate().is_err());
        config.memory_mb = 256;

        // Test timeout boundaries
        config.timeout_secs = 1;
        assert!(config.validate().is_ok());

        config.timeout_secs = 900;
        assert!(config.validate().is_ok());

        config.timeout_secs = 0;
        assert!(config.validate().is_err());

        config.timeout_secs = 901;
        assert!(config.validate().is_err());
        config.timeout_secs = 30;

        // Test ephemeral storage boundaries
        config.ephemeral_storage_mb = 512;
        assert!(config.validate().is_ok());

        config.ephemeral_storage_mb = 10240;
        assert!(config.validate().is_ok());

        config.ephemeral_storage_mb = 511;
        assert!(config.validate().is_err());

        config.ephemeral_storage_mb = 10241;
        assert!(config.validate().is_err());
        config.ephemeral_storage_mb = 512;

        // Test package type
        config.package_type = "Zip".to_string();
        assert!(config.validate().is_ok());

        config.package_type = "Invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_dead_letter_config() {
        let yaml = serde_yaml::from_str::<serde_yaml::Value>(
            r#"
role_arn: arn:aws:iam::123456789:role/lambda-exec
dead_letter_queue:
  target_arn: arn:aws:sqs:us-east-1:123456789:dlq
"#,
        )
        .unwrap();

        let config = LambdaProvisioningConfig::from_yaml_value(&yaml).unwrap();

        assert!(config.dead_letter_queue.is_some());
        let dlq = config.dead_letter_queue.unwrap();
        assert_eq!(dlq.target_arn, "arn:aws:sqs:us-east-1:123456789:dlq");
    }
}
