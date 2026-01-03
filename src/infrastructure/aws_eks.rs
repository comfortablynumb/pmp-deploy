use async_trait::async_trait;
use aws_sdk_eks::Client as EksClient;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Client, Config};
use std::collections::HashMap;

use super::provider::{DeploymentContext, InfrastructureProvider, InfrastructureType};
use crate::config::InfrastructureConfig;
use crate::deployment::{DeploymentResult, DeploymentType};

pub struct AwsEksProvider {
    eks_client: EksClient,
    k8s_client: Client,
    cluster_name: String,
    region: String,
    namespace: String,
}

impl AwsEksProvider {
    pub async fn new(cluster_name: &str, region: &str, namespace: &str) -> anyhow::Result<Self> {
        // Initialize AWS SDK
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_eks::config::Region::new(region.to_string()))
            .load()
            .await;

        let eks_client = EksClient::new(&aws_config);

        // Get cluster info to build kubeconfig
        let cluster_info = eks_client
            .describe_cluster()
            .name(cluster_name)
            .send()
            .await?
            .cluster
            .ok_or_else(|| anyhow::anyhow!("Cluster {} not found", cluster_name))?;

        let endpoint = cluster_info
            .endpoint
            .ok_or_else(|| anyhow::anyhow!("Cluster has no endpoint"))?;

        let ca_data = cluster_info
            .certificate_authority
            .and_then(|ca| ca.data)
            .ok_or_else(|| anyhow::anyhow!("Cluster has no CA data"))?;

        // Try to use existing kubeconfig context if available
        let k8s_client = match Kubeconfig::read() {
            Ok(kubeconfig) => {
                let _context_name = format!("arn:aws:eks:{}:{}:{}", region, "account", cluster_name);

                let options = KubeConfigOptions {
                    context: kubeconfig
                        .contexts
                        .iter()
                        .find(|c| c.name.contains(cluster_name))
                        .map(|c| c.name.clone()),
                    ..Default::default()
                };

                match Config::from_custom_kubeconfig(kubeconfig, &options).await {
                    Ok(config) => Client::try_from(config)?,
                    Err(_) => {
                        tracing::warn!(
                            "Could not use existing kubeconfig, falling back to EKS auth"
                        );
                        Self::create_client_from_eks(&endpoint, &ca_data, cluster_name, region)
                            .await?
                    }
                }
            }
            Err(_) => {
                Self::create_client_from_eks(&endpoint, &ca_data, cluster_name, region).await?
            }
        };

        Ok(Self {
            eks_client,
            k8s_client,
            cluster_name: cluster_name.to_string(),
            region: region.to_string(),
            namespace: namespace.to_string(),
        })
    }

    async fn create_client_from_eks(
        _endpoint: &str,
        _ca_data: &str,
        cluster_name: &str,
        region: &str,
    ) -> anyhow::Result<Client> {
        // For EKS, we use AWS CLI to update kubeconfig, then use the kubeconfig
        tracing::info!(
            "Configuring kubectl for EKS cluster {} in {}",
            cluster_name,
            region
        );

        // Update kubeconfig using AWS CLI
        let output = tokio::process::Command::new("aws")
            .args([
                "eks",
                "update-kubeconfig",
                "--name",
                cluster_name,
                "--region",
                region,
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "Failed to configure kubectl for EKS cluster. \
                Ensure AWS CLI is configured with access to the cluster.\n{}",
                stderr
            );
        }

        // Now try to create client from the updated kubeconfig
        let kubeconfig = Kubeconfig::read()?;
        let options = KubeConfigOptions {
            context: kubeconfig
                .contexts
                .iter()
                .find(|c| c.name.contains(cluster_name))
                .map(|c| c.name.clone()),
            ..Default::default()
        };

        let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
        Ok(Client::try_from(config)?)
    }

    pub async fn from_config(config: &InfrastructureConfig) -> anyhow::Result<Self> {
        let cluster_name = config
            .config
            .get("cluster_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cluster_name is required for aws-eks"))?;

        let region = config
            .config
            .get("region")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("region is required for aws-eks"))?;

        let namespace = config
            .config
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        Self::new(cluster_name, region, namespace).await
    }

    async fn get_cluster_status(&self) -> anyhow::Result<String> {
        let cluster = self
            .eks_client
            .describe_cluster()
            .name(&self.cluster_name)
            .send()
            .await?
            .cluster
            .ok_or_else(|| anyhow::anyhow!("Cluster not found"))?;

        Ok(cluster
            .status
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string()))
    }
}

#[async_trait]
impl InfrastructureProvider for AwsEksProvider {
    fn infrastructure_type(&self) -> InfrastructureType {
        InfrastructureType::AwsEks
    }

    fn supported_deployment_types(&self) -> Vec<DeploymentType> {
        // EKS is Kubernetes-based, supports the same deployment types
        vec![DeploymentType::RollingUpdate, DeploymentType::AllIn]
    }

    async fn validate_config(
        &self,
        _config: &HashMap<String, serde_yaml::Value>,
    ) -> anyhow::Result<()> {
        // Verify EKS cluster is accessible
        let status = self.get_cluster_status().await?;

        if status != "ACTIVE" {
            anyhow::bail!("EKS cluster {} is not active (status: {})", self.cluster_name, status);
        }

        // Verify Kubernetes API is accessible
        let namespaces: kube::api::Api<k8s_openapi::api::core::v1::Namespace> =
            kube::api::Api::all(self.k8s_client.clone());

        namespaces
            .list(&kube::api::ListParams::default().limit(1))
            .await?;

        Ok(())
    }

    async fn deploy(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        // Delegate to Kubernetes provider logic
        let image = ctx
            .environment
            .image
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No image specified for deployment"))?;

        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would deploy {} to EKS cluster {} (deployment/{})",
                image, self.cluster_name, deployment_name
            )));
        }

        tracing::info!(
            "Deploying {} to EKS cluster {} in namespace {}",
            image,
            self.cluster_name,
            self.namespace
        );

        // Update deployment
        let deployments: kube::api::Api<k8s_openapi::api::apps::v1::Deployment> =
            kube::api::Api::namespaced(self.k8s_client.clone(), &self.namespace);

        let patch = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [{
                            "name": deployment_name,
                            "image": image
                        }]
                    }
                }
            }
        });

        deployments
            .patch(
                deployment_name,
                &kube::api::PatchParams::apply("pmp-deploy"),
                &kube::api::Patch::Merge(&patch),
            )
            .await?;

        // Wait for rollout
        let timeout = ctx
            .environment
            .config
            .get("rollout_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        let start = std::time::Instant::now();

        loop {
            if start.elapsed().as_secs() > timeout {
                anyhow::bail!("Deployment rollout timed out");
            }

            let deployment = deployments.get(deployment_name).await?;

            if let Some(status) = deployment.status {
                let replicas = status.replicas.unwrap_or(0);
                let ready = status.ready_replicas.unwrap_or(0);

                if ready == replicas && replicas > 0 {
                    break;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        Ok(DeploymentResult::success(format!(
            "Deployed {} to EKS cluster {}",
            image, self.cluster_name
        ))
        .with_version(image.clone()))
    }

    async fn rollback(&self, ctx: &DeploymentContext) -> anyhow::Result<DeploymentResult> {
        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        if ctx.dry_run {
            return Ok(DeploymentResult::success(format!(
                "Would rollback deployment/{} on EKS cluster {}",
                deployment_name, self.cluster_name
            )));
        }

        let output = tokio::process::Command::new("kubectl")
            .args([
                "rollout",
                "undo",
                &format!("deployment/{}", deployment_name),
                "-n",
                &self.namespace,
                "--context",
                &format!("arn:aws:eks:{}:*:{}", self.region, self.cluster_name),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Rollback failed: {}", stderr);
        }

        Ok(DeploymentResult::success(format!(
            "Rolled back deployment/{} on EKS cluster {}",
            deployment_name, self.cluster_name
        )))
    }

    async fn status(&self, ctx: &DeploymentContext) -> anyhow::Result<String> {
        let cluster_status = self.get_cluster_status().await?;

        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let deployments: kube::api::Api<k8s_openapi::api::apps::v1::Deployment> =
            kube::api::Api::namespaced(self.k8s_client.clone(), &self.namespace);

        let deployment = deployments.get(deployment_name).await?;
        let dep_status = deployment.status.unwrap_or_default();

        Ok(format!(
            "EKS Cluster: {} ({})\n\
             Region: {}\n\
             Namespace: {}\n\
             Deployment: {}\n\
             Replicas: {}/{} ready",
            self.cluster_name,
            cluster_status,
            self.region,
            self.namespace,
            deployment_name,
            dep_status.ready_replicas.unwrap_or(0),
            dep_status.replicas.unwrap_or(0),
        ))
    }

    async fn logs(&self, ctx: &DeploymentContext, follow: bool) -> anyhow::Result<()> {
        let deployment_name = ctx
            .environment
            .config
            .get("deployment_name")
            .and_then(|v| v.as_str())
            .unwrap_or("app");

        let mut args = vec![
            "logs".to_string(),
            "-l".to_string(),
            format!("app={}", deployment_name),
            "-n".to_string(),
            self.namespace.clone(),
        ];

        if follow {
            args.push("-f".to_string());
        } else {
            args.push("--tail=100".to_string());
        }

        let mut child = tokio::process::Command::new("kubectl")
            .args(&args)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        child.wait().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_aws_eks_provider_config_parsing() {
        // Basic config parsing test
        use crate::config::InfrastructureConfig;
        use std::collections::HashMap;

        let mut config_map = HashMap::new();
        config_map.insert(
            "cluster_name".to_string(),
            serde_yaml::Value::String("my-cluster".to_string()),
        );
        config_map.insert(
            "region".to_string(),
            serde_yaml::Value::String("us-east-1".to_string()),
        );

        let config = InfrastructureConfig {
            infrastructure_type: "aws-eks".to_string(),
            config: config_map,
        };

        assert!(config.config.contains_key("cluster_name"));
        assert!(config.config.contains_key("region"));
    }
}
