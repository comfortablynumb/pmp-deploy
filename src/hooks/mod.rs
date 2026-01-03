//! Pre/Post Deployment Hooks
//!
//! Execute custom tasks before and/or after deployments for database migrations,
//! cache warming, smoke tests, notifications, etc.

mod container;
mod ecs;
pub mod executor;
mod http;
mod k8s;
mod lambda;
mod types;

pub use container::ContainerHookExecutor;
pub use ecs::EcsTaskHookExecutor;
pub use executor::{HookExecutor, HookResult, HookRunner};
pub use http::HttpHookExecutor;
pub use k8s::K8sJobHookExecutor;
pub use lambda::LambdaHookExecutor;
pub use types::{
    ContainerHookConfig, EcsTaskHookConfig, HookConfig, HookTiming, HookType, HooksConfig,
    HttpHookConfig, K8sJobHookConfig, LambdaHookConfig,
};
