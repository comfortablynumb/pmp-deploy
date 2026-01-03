# pmp-deploy

> ⚠️ **WORK IN PROGRESS** - This project is under active development. APIs and configurations may change. Use in production at your own risk.

A Rust CLI tool for simplified multi-infrastructure application deployments.

## Features

- **Multi-Infrastructure Support**: Deploy to AWS EKS, AWS ECS, AWS Lambda, Kubernetes, and Docker Compose
- **Helm & Kustomize**: Native support for Helm charts and Kustomize overlays
- **Deployment Strategies**: Rolling Update and All-In deployment modes
- **Configuration-Driven**: Define infrastructure and environments in a single YAML file
- **Multi-Project Management**: Manage multiple projects from a global configuration
- **Web UI**: Browser-based interface for deployments with HTTPS support and real-time metrics
- **Metrics Observation**: Monitor deployments with CloudWatch and Prometheus integration
- **Deployment Hooks**: Pre/post deployment hooks via containers, HTTP webhooks, ECS tasks, K8s Jobs, or Lambda
- **Deployment History**: Persist deployment records with SQLite, file-based, or in-memory storage
- **Secrets Management**: Integrate with environment variables, AWS Secrets Manager, and HashiCorp Vault
- **Plugin System**: Extend with custom infrastructure providers via native shared libraries
- **Production Ready**: Retry logic, graceful shutdown, structured logging, credential masking

## Installation

```bash
cargo install pmp-deploy
```

Or build from source:

```bash
git clone https://github.com/yourusername/pmp-deploy
cd pmp-deploy
cargo build --release
```

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `kubernetes` | Enable Kubernetes/EKS support (requires kube, k8s-openapi) | Off |

To build with Kubernetes support:

```bash
cargo build --release --features kubernetes
```

## Quick Start

### Initialize a new project

```bash
pmp-deploy init
```

This creates a `.pmp-deploy.yaml` file in the current directory.

### Configure your deployment

Edit `.pmp-deploy.yaml`:

```yaml
infrastructure:
  aws-dev:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1
      namespace: default

environments:
  development:
    infrastructure: aws-dev
    deployment_type: rolling-update
    image: my-app:v1.0.0
    replicas: 3
```

### Deploy

```bash
pmp-deploy deploy development
```

## Commands

| Command | Description |
|---------|-------------|
| `pmp-deploy deploy <env>` | Deploy to the specified environment |
| `pmp-deploy provision <env>` | Provision infrastructure (create if not exists, update if exists) |
| `pmp-deploy status <env>` | Check deployment status |
| `pmp-deploy rollback <env>` | Rollback to previous version |
| `pmp-deploy logs <env>` | Stream or fetch logs |
| `pmp-deploy hooks list <env>` | List configured hooks for an environment |
| `pmp-deploy hooks run <env> [hook]` | Run specific or all hooks |
| `pmp-deploy list` | List available environments |
| `pmp-deploy validate` | Validate configuration file |
| `pmp-deploy init` | Initialize a new configuration file |
| `pmp-deploy projects list` | List all configured projects |
| `pmp-deploy projects add` | Add a project to global configuration |
| `pmp-deploy projects remove` | Remove a project from global configuration |
| `pmp-deploy ui` | Start the web UI server |

## Global Flags

| Flag | Description |
|------|-------------|
| `-f, --file <path>` | Path to configuration file |
| `-v, --verbose` | Enable verbose output |
| `-q, --quiet` | Suppress all output except errors |
| `--output <format>` | Output format: text, json |

## Deploy Command Options

| Flag | Description |
|------|-------------|
| `--dry-run` | Perform a dry run without making changes |
| `-y, --yes` | Skip confirmation prompt |
| `--image <image>` | Override the image to deploy |
| `--deploy-mode <mode>` | Deploy mode: `app-only` (default) or `full` |
| `--skip-hooks` | Skip all pre and post deployment hooks |
| `--skip-pre-hooks` | Skip pre-deployment hooks only |
| `--skip-post-hooks` | Skip post-deployment hooks only |

### Deploy Modes

- **app-only** (default): Only updates the application image/version, leaving infrastructure settings unchanged
- **full**: Updates both the application AND infrastructure settings (auto-scaling, concurrency, etc.)

#### What Gets Skipped in App-Only Mode

| Provider | Skipped in App-Only Mode |
|----------|-------------------------|
| AWS ECS | Auto-scaling policies, scheduled scaling actions |
| AWS Lambda | Provisioned concurrency, reserved concurrency |
| Kubernetes | Uses direct image update instead of Helm/Kustomize |

Example usage:

```bash
# Default: app-only deployment (just updates the image)
pmp-deploy deploy production

# Full deployment: updates app + infrastructure settings
pmp-deploy deploy production --deploy-mode full

# Quick hotfix - only update the image
pmp-deploy deploy staging --image myapp:hotfix-123
```

## Provision Command Options

| Flag | Description |
|------|-------------|
| `--plan` | Show planned changes without applying |
| `--dry-run` | Perform a dry run without making changes |
| `-y, --yes` | Skip confirmation prompt |
| `--image <image>` | Override the image to deploy |

### Infrastructure Provisioning

The `provision` command creates infrastructure resources if they don't exist, or updates them if they do. This is useful for:

- Initial deployment to a new environment
- Infrastructure changes (scaling, configuration updates)
- Recreating resources after deletion

Example usage:

```bash
# Preview changes without applying
pmp-deploy provision production --plan

# Provision with confirmation prompt
pmp-deploy provision production --image myapp:v1.0.0

# Provision without confirmation
pmp-deploy provision production --image myapp:v1.0.0 -y
```

### Lambda Provisioning

Create or update Lambda functions with full configuration:

```yaml
infrastructure:
  aws-lambda-prod:
    type: aws-lambda
    config:
      function_name: my-function
      region: us-east-1
      provision:
        runtime: provided.al2023
        handler: bootstrap
        memory_mb: 512
        timeout_secs: 30
        role_arn: arn:aws:iam::123456789:role/lambda-exec
        architecture: arm64
        vpc:
          subnet_ids:
            - subnet-abc123
          security_group_ids:
            - sg-xyz789
        layers:
          - arn:aws:lambda:us-east-1:123456789:layer:my-layer:1
        tracing_mode: Active
```

### ECS Provisioning

Create or update ECS clusters, task definitions, and services:

```yaml
infrastructure:
  aws-ecs-prod:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: my-service
      provision:
        cluster:
          capacity_providers:
            - FARGATE
            - FARGATE_SPOT
          container_insights: true
        task:
          family: my-app
          cpu: "512"
          memory: "1024"
          execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
          task_role_arn: arn:aws:iam::123456789:role/ecs-task
          containers:
            - name: app
              image: my-app:latest
              essential: true
              port_mappings:
                - container_port: 8080
              log_configuration:
                log_driver: awslogs
                options:
                  awslogs-group: /ecs/my-app
                  awslogs-region: us-east-1
        service:
          desired_count: 3
          launch_type: FARGATE
          deployment:
            minimum_healthy_percent: 100
            maximum_percent: 200
          network:
            subnets:
              - subnet-abc123
            security_groups:
              - sg-xyz789
            assign_public_ip: true
          enable_circuit_breaker: true
```

### Kubernetes Provisioning

For Kubernetes, provisioning is handled automatically by Helm's `upgrade --install` command. Configure your Helm chart as usual and use `pmp-deploy deploy` with Helm configuration.

## Configuration

### Project Configuration (`.pmp-deploy.yaml`)

```yaml
infrastructure:
  <name>:
    type: aws-eks | aws-ecs | aws-lambda | kubernetes | docker-compose
    config:
      # Infrastructure-specific configuration

environments:
  <name>:
    infrastructure: <infrastructure-name>
    deployment_type: rolling-update | all-in
    image: <image:tag>
    replicas: <number>
    env:
      KEY: value
```

### Deployment Type Support Matrix

| Deployment Type | AWS ECS | AWS Lambda | AWS EKS | Kubernetes | Docker Compose |
|-----------------|---------|------------|---------|------------|----------------|
| `rolling-update` | Yes | - | Yes | Yes | - |
| `all-in` | Yes | Yes | Yes | Yes | Yes |

> **Note**: Using an unsupported deployment type for a given infrastructure will result in an error.

### Global Configuration (`$HOME/.pmp-deploy.yaml`)

```yaml
projects:
  - path: $HOME/project1
    name: my-project
  - path: /opt/project2
```

## Infrastructure Types

### AWS EKS

```yaml
infrastructure:
  aws-prod:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1
      namespace: production
      service_account: deploy-sa
```

### AWS ECS

```yaml
infrastructure:
  aws-ecs:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: my-service
```

#### Load Balancer Integration

```yaml
infrastructure:
  ecs-with-alb:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: api-service
      load_balancer:
        target_group_arn: arn:aws:elasticloadbalancing:...
        container_name: app
        container_port: 8080
        health_check:
          path: /health
          interval_seconds: 15
          healthy_threshold: 2
```

#### CloudWatch Logs

```yaml
infrastructure:
  ecs-with-logs:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: my-service
      cloudwatch_logs:
        log_group: /ecs/my-service
        retention_days: 14
        create_log_group: true
```

#### Auto Scaling

```yaml
infrastructure:
  ecs-autoscaled:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: api-service
      autoscaling:
        enabled: true
        min_capacity: 2
        max_capacity: 20
        target_tracking:
          target_value: 70.0
          metric_type: ECSServiceAverageCPUUtilization
          scale_in_cooldown: 300
          scale_out_cooldown: 60
        scheduled:
          - name: scale-up-morning
            schedule: cron(0 8 ? * MON-FRI *)
            min_capacity: 5
```

### AWS Lambda

```yaml
infrastructure:
  lambda:
    type: aws-lambda
    config:
      function_name: my-function
      region: us-east-1
      alias: prod  # Optional, enables versioning
```

#### Concurrency Configuration (Full Mode Only)

```yaml
environments:
  production:
    infrastructure: lambda
    image: 123456789.dkr.ecr.us-east-1.amazonaws.com/my-function:v1.0.0
    config:
      provisioned_concurrency: 10  # Pre-warm instances (requires --deploy-mode full)
      reserved_concurrency: 100    # Max concurrent executions (requires --deploy-mode full)
```

### Kubernetes

```yaml
infrastructure:
  k8s:
    type: kubernetes
    config:
      kubeconfig_path: ~/.kube/config
      context: my-context
      namespace: default
```

#### Helm Support

Deploy using Helm charts:

```yaml
infrastructure:
  k8s-helm:
    type: kubernetes
    config:
      context: my-context
      namespace: default
      helm:
        chart: ./charts/my-app           # Local chart
        # OR: chart: nginx               # From repository
        # repo: https://charts.bitnami.com/bitnami
        # repo_name: bitnami
        release_name: my-app
        values_files:
          - values.yaml
        wait: true
        timeout: "10m"
        create_namespace: true
        atomic: true                     # Rollback on failure

environments:
  production:
    infrastructure: k8s-helm
    image: my-app:v1.0.0
    config:
      helm:
        set:
          replicaCount: "5"
          resources.requests.memory: "512Mi"
        set_string:
          annotations.version: "v1.0.0"
```

#### Kustomize Support

Deploy using Kustomize overlays:

```yaml
infrastructure:
  k8s-kustomize:
    type: kubernetes
    config:
      context: my-context
      namespace: default
      kustomize:
        path: ./k8s/base

environments:
  production:
    infrastructure: k8s-kustomize
    image: my-app:v1.0.0
    config:
      kustomize:
        path: ./k8s/overlays/production
        images:
          - name: my-app
            new_name: prod-registry.example.com/my-app
            new_tag: v1.0.0
        replicas:
          - name: my-app-deployment
            count: 5
        labels:
          environment: production
        prune: true
```

### Docker Compose

```yaml
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml
      project_name: my-app
```

## Deployment Types

| Type | Description |
|------|-------------|
| `rolling-update` | Gradually replace instances with zero downtime |
| `all-in` | Stop all instances, then deploy new version (for K8s uses native Recreate strategy) |

## Web UI

Start the web interface:

```bash
pmp-deploy ui --port 8080
```

Open http://localhost:8080 in your browser.

### HTTPS Support

Enable HTTPS with your own certificates:

```bash
pmp-deploy ui --port 443 --tls-cert /path/to/cert.pem --tls-key /path/to/key.pem
```

For development, generate self-signed certificates automatically:

```bash
pmp-deploy ui --port 8443 --dev-tls
```

### UI Command Options

| Option | Description |
|--------|-------------|
| `-p, --port` | Port to run the server on (default: 8080) |
| `--host` | Host to bind to (default: 127.0.0.1) |
| `--open` | Open browser automatically |
| `--tls-cert` | Path to TLS certificate file |
| `--tls-key` | Path to TLS private key file |
| `--dev-tls` | Generate self-signed development certificate |

### UI Features

- **Project Selector**: Choose from configured projects
- **Environment Dashboard**: View all environments with status indicators
- **Deploy**: Trigger deployments with optional image override and dry-run mode
- **Rollback**: One-click rollback to previous version
- **Status**: Real-time deployment status
- **Logs**: Live log streaming via Server-Sent Events (SSE)

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/projects` | GET | List all projects |
| `/api/projects/{id}` | GET | Get project details |
| `/api/projects/{id}/environments` | GET | List environments |
| `/api/projects/{id}/environments/{env}` | GET | Get environment details |
| `/api/projects/{id}/environments/{env}/deploy` | POST | Trigger deployment |
| `/api/projects/{id}/environments/{env}/status` | GET | Get deployment status |
| `/api/projects/{id}/environments/{env}/rollback` | POST | Trigger rollback |
| `/api/projects/{id}/environments/{env}/logs` | GET | Stream logs (SSE) |

## Environment Variables (System)

| Variable | Description |
|----------|-------------|
| `AWS_ACCESS_KEY_ID` | AWS access key |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key |
| `AWS_REGION` | Default AWS region |
| `KUBECONFIG` | Path to Kubernetes config |
| `VAULT_ADDR` | HashiCorp Vault address |
| `VAULT_TOKEN` | HashiCorp Vault token |

## Environment Variable Configuration

Configure environment variables for your deployments with support for multiple sources:

### Legacy Format (Simple)

```yaml
environments:
  production:
    env:
      SIMPLE_VAR: "static-value"
      DATABASE_HOST: "db.example.com"
```

### New Flexible Format

```yaml
environments:
  production:
    environment:
      # Static value
      DATABASE_URL:
        source: static_value
        value: "postgres://localhost/mydb"

      # From OS environment variable
      API_KEY:
        source: environment
        env_var: MY_API_KEY

      # From AWS Secrets Manager
      DB_PASSWORD:
        source: aws_secrets_manager
        arn: arn:aws:secretsmanager:us-east-1:123456789:secret:prod/db
        json_field: password  # Extract specific field from JSON secret

      # From HashiCorp Vault
      VAULT_SECRET:
        source: vault
        key: secret/data/myapp
        json_field: api_token
```

### Environment Variable Sources

| Source | Description | Required Fields |
|--------|-------------|-----------------|
| `static_value` | Inline static value | `value` |
| `environment` | OS environment variable | `env_var` (or defaults to variable name) |
| `aws_secrets_manager` | AWS Secrets Manager | `arn` or `key` |
| `vault` | HashiCorp Vault | `key` |

### Optional Fields

| Field | Description |
|-------|-------------|
| `json_field` | Extract specific field from JSON-formatted secret |
| `version` | Secret version (for versioned secrets) |

Both legacy `env:` and new `environment:` formats can be used together. The new format values take precedence if the same key exists in both.

## Plugin System

Extend pmp-deploy with custom infrastructure providers by creating plugins.

### Creating a Plugin

1. Create a new Rust library project:

```bash
cargo new --lib my-plugin
```

2. Update `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
pmp-deploy-plugin-sdk = "0.1"
```

3. Implement your plugin in `src/lib.rs`:

```rust
use pmp_deploy_plugin_sdk::*;

declare_plugin!(
    name: "my-plugin",
    version: "0.1.0",
    description: "My custom infrastructure plugin",
    infrastructure_type: "my-infra",
    plugin: MyPlugin,
);

#[derive(Default)]
struct MyPlugin;

impl InfrastructurePlugin for MyPlugin {
    fn validate_config(&self, config: &PluginConfig) -> PluginResult<()> {
        // Validate configuration
        Ok(())
    }

    fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
        // Deploy to your infrastructure
        Ok(DeploymentResult::success("Deployed successfully", "v1.0.0"))
    }

    fn rollback(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
        Ok(DeploymentResult::success("Rolled back", "v0.9.0"))
    }

    fn status(&self, ctx: &DeploymentContext) -> PluginResult<String> {
        Ok("Running".to_string())
    }

    fn logs(&self, ctx: &DeploymentContext, follow: bool) -> PluginResult<String> {
        Ok("Log output...".to_string())
    }
}
```

4. Build and install:

```bash
cargo build --release
# Copy to plugin directory
mkdir -p ~/.pmp-deploy/plugins
cp target/release/libmy_plugin.so ~/.pmp-deploy/plugins/  # Linux
cp target/release/my_plugin.dll ~/.pmp-deploy/plugins/    # Windows
cp target/release/libmy_plugin.dylib ~/.pmp-deploy/plugins/  # macOS
```

5. Use in configuration:

```yaml
infrastructure:
  my-custom:
    type: my-infra  # Matches infrastructure_type in plugin
    config:
      # Plugin-specific configuration
```

### Plugin Directory

Plugins are automatically discovered from `~/.pmp-deploy/plugins/`.

## License

MIT
