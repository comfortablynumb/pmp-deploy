/// Infrastructure type selection for the init wizard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfrastructureTypeSelection {
    DockerCompose,
    AwsEks,
    AwsEcs,
    AwsLambda,
    Kubernetes,
}

impl InfrastructureTypeSelection {
    pub fn all() -> &'static [Self] {
        &[
            Self::DockerCompose,
            Self::AwsEks,
            Self::AwsEcs,
            Self::AwsLambda,
            Self::Kubernetes,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::DockerCompose => "docker-compose",
            Self::AwsEks => "aws-eks",
            Self::AwsEcs => "aws-ecs",
            Self::AwsLambda => "aws-lambda",
            Self::Kubernetes => "kubernetes",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::DockerCompose => "Local development with Docker Compose",
            Self::AwsEks => "AWS Elastic Kubernetes Service",
            Self::AwsEcs => "AWS Elastic Container Service",
            Self::AwsLambda => "AWS Lambda Functions",
            Self::Kubernetes => "Generic Kubernetes cluster",
        }
    }

    pub fn config_type(&self) -> &'static str {
        match self {
            Self::DockerCompose => "docker-compose",
            Self::AwsEks => "aws-eks",
            Self::AwsEcs => "aws-ecs",
            Self::AwsLambda => "aws-lambda",
            Self::Kubernetes => "kubernetes",
        }
    }
}

/// Deploy mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployModeSelection {
    /// Create new infrastructure
    Full,
    /// Use existing infrastructure
    AppOnly,
}

impl DeployModeSelection {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::AppOnly => "AppOnly",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Full => "Create new infrastructure (pmp-deploy will provision resources)",
            Self::AppOnly => "Use existing infrastructure (provide connection details)",
        }
    }
}

/// Docker Compose connection details
#[derive(Debug, Clone)]
pub struct DockerComposeDetails {
    pub compose_file: String,
    pub project_name: String,
}

/// AWS EKS connection details
#[derive(Debug, Clone)]
pub struct AwsEksDetails {
    pub cluster_name: String,
    pub region: String,
    pub namespace: String,
}

/// AWS ECS connection details
#[derive(Debug, Clone)]
pub struct AwsEcsDetails {
    pub cluster: String,
    pub region: String,
    pub launch_type: String,
}

/// AWS Lambda connection details
#[derive(Debug, Clone)]
pub struct AwsLambdaDetails {
    pub region: String,
    pub function_name_prefix: String,
}

/// Kubernetes connection details
#[derive(Debug, Clone)]
pub struct KubernetesDetails {
    pub context: String,
    pub namespace: String,
}

/// Connection details for AppOnly mode (one variant per infrastructure type)
#[derive(Debug, Clone)]
pub enum ConnectionDetails {
    DockerCompose(DockerComposeDetails),
    AwsEks(AwsEksDetails),
    AwsEcs(AwsEcsDetails),
    AwsLambda(AwsLambdaDetails),
    Kubernetes(KubernetesDetails),
}

/// Configuration for an environment being initialized
#[derive(Debug, Clone)]
pub struct EnvironmentSelection {
    pub name: String,
    pub infrastructure_type: InfrastructureTypeSelection,
    pub deploy_mode: DeployModeSelection,
    pub connection_details: Option<ConnectionDetails>,
}

/// Predefined environment options
pub const PREDEFINED_ENVIRONMENTS: &[&str] = &["development", "staging", "production"];
