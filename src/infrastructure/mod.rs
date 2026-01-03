mod provider;
pub mod aws_ecs;
pub mod aws_lambda;
pub mod docker_compose;
pub mod provisioning;
pub mod registry;

#[cfg(feature = "kubernetes")]
pub mod aws_eks;
#[cfg(feature = "kubernetes")]
pub mod helm;
#[cfg(feature = "kubernetes")]
pub mod kubernetes;
#[cfg(feature = "kubernetes")]
pub mod kustomize;

#[cfg(feature = "kubernetes")]
pub use helm::{HelmConfig, HelmDeployer};
#[cfg(feature = "kubernetes")]
pub use kubernetes::DeploymentMethod;
#[cfg(feature = "kubernetes")]
pub use kustomize::{KustomizeConfig, KustomizeDeployer};

pub use provider::{DeployMode, DeploymentContext, InfrastructureProvider, InfrastructureType};
pub use provisioning::{
    compute_lambda_diff, EcsClusterConfig, EcsCurrentState, EcsNetworkConfig,
    EcsProvisioningConfig, EcsServiceConfig, EcsTaskConfig, LambdaCurrentState,
    LambdaProvisioningConfig, LambdaStateDiff, LambdaVpcConfig, PlannedAction, PlannedChange,
    ProvisioningAction, ProvisioningPlan, ProvisioningResult,
};
pub use registry::{ProviderFactory, ProviderRegistry};
