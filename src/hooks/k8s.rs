//! Kubernetes Job hook executor for one-shot K8s Jobs.

#[cfg(feature = "kubernetes")]
mod inner {
    use async_trait::async_trait;
    use k8s_openapi::api::batch::v1::{Job, JobSpec};
    use k8s_openapi::api::core::v1::{
        Container, EnvFromSource, EnvVar, PodSpec, PodTemplateSpec, ResourceRequirements,
        SecretEnvSource,
    };
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use kube::api::{Api, DeleteParams, PostParams};
    use kube::{Client, Config};
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    use tokio::time::sleep;
    use tracing::{debug, info, warn};

    use crate::error::{InfrastructureError, Result};

    use crate::hooks::executor::{HookContext, HookExecutor, HookResult};
    use crate::hooks::types::{HookConfig, HookType, K8sJobHookConfig};

    /// Kubernetes Job hook executor for running one-shot Jobs.
    pub struct K8sJobHookExecutor {
        client: Option<Client>,
    }

    impl K8sJobHookExecutor {
        pub fn new() -> Self {
            Self { client: None }
        }

        pub fn with_client(client: Client) -> Self {
            Self {
                client: Some(client),
            }
        }

        async fn get_client(&self) -> Result<Client> {
            if let Some(ref client) = self.client {
                return Ok(client.clone());
            }

            let config = Config::infer().await.map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                    provider: "Kubernetes".to_string(),
                    message: format!("Failed to load kubeconfig: {}", e),
                })
            })?;

            Client::try_from(config).map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ConnectionFailed {
                    provider: "Kubernetes".to_string(),
                    message: format!("Failed to create client: {}", e),
                })
            })
        }

        fn get_k8s_config(hook: &HookConfig) -> Option<&K8sJobHookConfig> {
            hook.config.k8s_job.as_ref()
        }

        fn build_job(
            &self,
            config: &K8sJobHookConfig,
            hook_name: &str,
            context: &HookContext,
        ) -> Result<Job> {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let job_name = format!(
                "hook-{}-{}",
                hook_name.to_lowercase().replace('_', "-"),
                timestamp
            );

            // Build image with optional tag override
            let image = if let Some(ref tag) = config.image_tag {
                if config.image.contains(':') {
                    let base = config.image.split(':').next().unwrap_or(&config.image);
                    format!("{}:{}", base, tag)
                } else {
                    format!("{}:{}", config.image, tag)
                }
            } else {
                config.image.clone()
            };

            // Build environment variables
            let mut env_vars = Vec::new();
            for (key, env_config) in &config.env {
                if let Some(ref value) = env_config.value {
                    env_vars.push(EnvVar {
                        name: key.clone(),
                        value: Some(value.clone()),
                        ..Default::default()
                    });
                }
            }

            // Build env_from sources
            let env_from: Vec<EnvFromSource> = config
                .env_from
                .iter()
                .filter_map(|source| match source {
                    crate::hooks::types::EnvFromSource::ConfigMap(name) => Some(EnvFromSource {
                        config_map_ref: Some(k8s_openapi::api::core::v1::ConfigMapEnvSource {
                            name: Some(name.clone()),
                            optional: Some(false),
                        }),
                        ..Default::default()
                    }),
                    crate::hooks::types::EnvFromSource::Secret(name) => Some(EnvFromSource {
                        secret_ref: Some(SecretEnvSource {
                            name: Some(name.clone()),
                            optional: Some(false),
                        }),
                        ..Default::default()
                    }),
                })
                .collect();

            // Build resource requirements
            let resources = config.resources.as_ref().map(|r| {
                let mut req = ResourceRequirements::default();

                if let Some(ref requests) = r.requests {
                    let mut map = BTreeMap::new();

                    if let Some(ref cpu) = requests.cpu {
                        map.insert("cpu".to_string(), Quantity(cpu.clone()));
                    }

                    if let Some(ref memory) = requests.memory {
                        map.insert("memory".to_string(), Quantity(memory.clone()));
                    }

                    if !map.is_empty() {
                        req.requests = Some(map);
                    }
                }

                if let Some(ref limits) = r.limits {
                    let mut map = BTreeMap::new();

                    if let Some(ref cpu) = limits.cpu {
                        map.insert("cpu".to_string(), Quantity(cpu.clone()));
                    }

                    if let Some(ref memory) = limits.memory {
                        map.insert("memory".to_string(), Quantity(memory.clone()));
                    }

                    if !map.is_empty() {
                        req.limits = Some(map);
                    }
                }

                req
            });

            // Build container
            let mut container = Container {
                name: "hook".to_string(),
                image: Some(image),
                env: if env_vars.is_empty() {
                    None
                } else {
                    Some(env_vars)
                },
                env_from: if env_from.is_empty() {
                    None
                } else {
                    Some(env_from)
                },
                resources,
                ..Default::default()
            };

            if !config.command.is_empty() {
                container.command = Some(config.command.clone());
            }

            if !config.args.is_empty() {
                container.args = Some(config.args.clone());
            }

            // Build labels
            let mut labels = config.labels.clone();
            labels.insert(
                "app.kubernetes.io/managed-by".to_string(),
                "pmp-deploy".to_string(),
            );
            labels.insert("pmp-deploy/hook-name".to_string(), hook_name.to_string());
            labels.insert(
                "pmp-deploy/environment".to_string(),
                context.environment.clone(),
            );

            // Build annotations
            let annotations = if config.annotations.is_empty() {
                None
            } else {
                Some(config.annotations.clone())
            };

            // Build Pod spec
            let pod_spec = PodSpec {
                containers: vec![container],
                restart_policy: Some(config.restart_policy.clone()),
                service_account_name: config.service_account.clone(),
                ..Default::default()
            };

            // Build Job
            let namespace = config
                .namespace
                .as_ref()
                .or(context.k8s_namespace.as_ref())
                .cloned()
                .unwrap_or_else(|| "default".to_string());

            let job = Job {
                metadata: ObjectMeta {
                    name: Some(job_name),
                    namespace: Some(namespace),
                    labels: Some(labels),
                    annotations,
                    ..Default::default()
                },
                spec: Some(JobSpec {
                    backoff_limit: Some(config.backoff_limit as i32),
                    active_deadline_seconds: config.active_deadline_seconds.map(|s| s as i64),
                    ttl_seconds_after_finished: config.ttl_seconds_after_finished.map(|s| s as i32),
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            labels: Some(BTreeMap::from([(
                                "pmp-deploy/hook-name".to_string(),
                                hook_name.to_string(),
                            )])),
                            ..Default::default()
                        }),
                        spec: Some(pod_spec),
                    },
                    ..Default::default()
                }),
                ..Default::default()
            };

            Ok(job)
        }

        async fn run_k8s_job(
            &self,
            config: &K8sJobHookConfig,
            hook_name: &str,
            context: &HookContext,
            timeout_secs: u32,
        ) -> Result<HookResult> {
            let start = Instant::now();
            let client = self.get_client().await?;

            let namespace = config
                .namespace
                .as_ref()
                .or(context.k8s_namespace.as_ref())
                .cloned()
                .unwrap_or_else(|| "default".to_string());

            let jobs: Api<Job> = Api::namespaced(client, &namespace);

            // Build and create the Job
            let job = self.build_job(config, hook_name, context)?;
            let job_name = job.metadata.name.clone().unwrap_or_default();

            debug!(
                "Creating Kubernetes Job '{}' in namespace '{}'",
                job_name, namespace
            );

            let created_job = jobs.create(&PostParams::default(), &job).await.map_err(|e| {
                crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                    provider: "Kubernetes".to_string(),
                    message: format!("Failed to create Job: {}", e),
                })
            })?;

            let job_name = created_job
                .metadata
                .name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            info!("Created Kubernetes Job: {}", job_name);

            // Wait for Job completion
            let timeout_duration = Duration::from_secs(timeout_secs as u64);
            let poll_interval = Duration::from_secs(5);

            loop {
                if start.elapsed() > timeout_duration {
                    // Delete the job on timeout
                    if config.cleanup {
                        let _ = jobs.delete(&job_name, &DeleteParams::default()).await;
                    }

                    return Ok(HookResult::failure(
                        hook_name,
                        format!("Kubernetes Job timed out after {} seconds", timeout_secs),
                        start.elapsed(),
                    ));
                }

                let current_job = jobs.get(&job_name).await.map_err(|e| {
                    crate::error::Error::Infrastructure(InfrastructureError::ProviderError {
                        provider: "Kubernetes".to_string(),
                        message: format!("Failed to get Job status: {}", e),
                    })
                })?;

                if let Some(status) = current_job.status {
                    let succeeded = status.succeeded.unwrap_or(0);
                    let failed = status.failed.unwrap_or(0);

                    debug!(
                        "Job '{}' status: succeeded={}, failed={}",
                        job_name, succeeded, failed
                    );

                    if succeeded > 0 {
                        info!("Kubernetes Job '{}' completed successfully", hook_name);

                        // Cleanup if requested
                        if config.cleanup {
                            debug!("Cleaning up Job '{}'", job_name);
                            let _ = jobs.delete(&job_name, &DeleteParams::default()).await;
                        }

                        return Ok(
                            HookResult::success(hook_name, "Job completed successfully".to_string(), start.elapsed())
                                .with_exit_code(0),
                        );
                    }

                    if failed > 0 && failed >= (config.backoff_limit as i32 + 1) {
                        warn!(
                            "Kubernetes Job '{}' failed after {} attempts",
                            hook_name, failed
                        );

                        // Cleanup if requested
                        if config.cleanup {
                            let _ = jobs.delete(&job_name, &DeleteParams::default()).await;
                        }

                        return Ok(HookResult::failure(
                            hook_name,
                            format!("Job failed after {} attempts", failed),
                            start.elapsed(),
                        )
                        .with_exit_code(1));
                    }
                }

                sleep(poll_interval).await;
            }
        }
    }

    impl Default for K8sJobHookExecutor {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl HookExecutor for K8sJobHookExecutor {
        fn hook_type(&self) -> HookType {
            HookType::K8sJob
        }

        async fn execute(&self, hook: &HookConfig, context: &HookContext) -> Result<HookResult> {
            let config = Self::get_k8s_config(hook).ok_or_else(|| {
                crate::error::Error::Deployment(crate::error::DeploymentError::HookFailed {
                    hook_name: hook.name.clone(),
                    message: "Kubernetes Job hook configuration not found".to_string(),
                })
            })?;

            if config.image.is_empty() {
                return Err(crate::error::Error::Deployment(
                    crate::error::DeploymentError::HookFailed {
                        hook_name: hook.name.clone(),
                        message: "Container image is required".to_string(),
                    },
                ));
            }

            self.run_k8s_job(config, &hook.name, context, hook.timeout_secs)
                .await
        }
    }
}

#[cfg(feature = "kubernetes")]
pub use inner::K8sJobHookExecutor;

/// Stub implementation when kubernetes feature is not enabled.
#[cfg(not(feature = "kubernetes"))]
pub struct K8sJobHookExecutor;

#[cfg(not(feature = "kubernetes"))]
impl K8sJobHookExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(feature = "kubernetes"))]
impl Default for K8sJobHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "kubernetes"))]
#[async_trait::async_trait]
impl crate::hooks::executor::HookExecutor for K8sJobHookExecutor {
    fn hook_type(&self) -> crate::hooks::types::HookType {
        crate::hooks::types::HookType::K8sJob
    }

    async fn execute(
        &self,
        hook: &crate::hooks::types::HookConfig,
        _context: &crate::hooks::executor::HookContext,
    ) -> crate::error::Result<crate::hooks::executor::HookResult> {
        Err(crate::error::Error::Deployment(
            crate::error::DeploymentError::HookFailed {
                hook_name: hook.name.clone(),
                message: "Kubernetes hooks require the 'kubernetes' feature".to_string(),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::executor::HookExecutor;
    use crate::hooks::types::{HookConfig, HookType, HookTypeConfig};

    #[test]
    fn test_k8s_executor_hook_type() {
        let executor = K8sJobHookExecutor::new();
        assert_eq!(executor.hook_type(), HookType::K8sJob);
    }

    #[cfg(not(feature = "kubernetes"))]
    #[tokio::test]
    async fn test_k8s_hook_feature_disabled() {
        use crate::hooks::executor::HookContext;

        let executor = K8sJobHookExecutor::new();
        let hook = HookConfig {
            name: "test".to_string(),
            hook_type: HookType::K8sJob,
            config: HookTypeConfig::default(),
            timeout_secs: 60,
            fail_on_error: true,
            description: None,
        };
        let context = HookContext::default();

        let result = executor.execute(&hook, &context).await;
        assert!(result.is_err());
    }
}
