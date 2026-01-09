mod provider;
pub mod aws_ecs;
pub mod aws_lambda;
pub mod docker_compose;
pub mod lambda_extended;
pub mod provisioning;
pub mod registry;

#[cfg(feature = "kubernetes")]
pub mod aws_eks;
#[cfg(feature = "kubernetes")]
pub mod helm;
#[cfg(feature = "kubernetes")]
pub mod k8s_resources;
#[cfg(feature = "kubernetes")]
pub mod kubernetes;
#[cfg(feature = "kubernetes")]
pub mod kustomize;
#[cfg(feature = "kubernetes")]
pub mod manifest;

#[cfg(feature = "kubernetes")]
pub use helm::{HelmConfig, HelmDeployer};
#[cfg(feature = "kubernetes")]
pub use k8s_resources::{
    HpaConfig, HpaCustomMetric, K8sConfigMapSpec, K8sResourceManager, K8sSecretSpec,
    RawManifestApplier, RawManifestConfig,
};
#[cfg(feature = "kubernetes")]
pub use kubernetes::DeploymentMethod;
#[cfg(feature = "kubernetes")]
pub use kustomize::{KustomizeConfig, KustomizeDeployer};
#[cfg(feature = "kubernetes")]
pub use manifest::{BuiltinVariables, ManifestRenderer, ManifestTemplateConfig, RenderedManifest};

pub use provider::{DeployMode, DeploymentContext, InfrastructureProvider, InfrastructureType};
pub use provisioning::{
    compute_lambda_diff, EcsClusterConfig, EcsCurrentState, EcsNetworkConfig,
    EcsProvisioningConfig, EcsServiceConfig, EcsTaskConfig, LambdaCurrentState,
    LambdaProvisioningConfig, LambdaStateDiff, LambdaVpcConfig, PlannedAction, PlannedChange,
    ProvisioningAction, ProvisioningPlan, ProvisioningResult,
};
pub use registry::{ProviderFactory, ProviderRegistry};

// Lambda extended features
pub use lambda_extended::{
    EventSourceConfig, EventSourceManager, EventSourceMapping, EventSourceType, LayerConfig,
    LayerManager, LayerVersion, LambdaPackager, PublishedLayer, S3Location, SourceAccessEntry,
    ZipPackageConfig,
};
