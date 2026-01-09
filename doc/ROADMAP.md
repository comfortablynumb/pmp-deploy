# pmp-deploy Roadmap

A Rust CLI tool for simplified multi-infrastructure application deployments.

---

## Milestone 1: Project Foundation [COMPLETED]

### 1.1 Project Scaffolding
- [x] Initialize Cargo project with workspace structure
- [x] Set up project directory structure:
  ```
  pmp-deploy/
  ├── Cargo.toml
  ├── src/
  │   ├── main.rs
  │   ├── lib.rs
  │   ├── cli/
  │   ├── config/
  │   ├── infrastructure/
  │   ├── deployment/
  │   ├── plugins/
  │   └── ui/
  ├── bin/
  │   ├── up.sh / up.bat
  │   └── down.sh / down.bat
  └── doc/
  ```
- [x] Configure essential dependencies:
  - `clap` - CLI argument parsing
  - `serde` / `serde_yaml` - Configuration parsing
  - `tokio` - Async runtime
  - `anyhow` / `thiserror` - Error handling
  - `tracing` - Logging

### 1.2 Configuration System
- [x] Define `.pmp-deploy.yaml` schema
- [x] Implement config file discovery (current dir, --file flag)
- [x] Implement `$HOME/.pmp-deploy.yaml` for multi-project support
- [x] Environment variable expansion in configs
- [x] Config validation with helpful error messages

### 1.3 Core Traits & Abstractions
- [x] `InfrastructureProvider` trait
- [x] `DeploymentStrategy` trait
- [x] `SecretsProvider` trait (schema defined)
- [x] `PluginLoader` trait

---

## Milestone 2: Authentication & Secrets [COMPLETED]

### 2.1 Environment Variables Provider
- [x] AWS credentials (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
- [x] Kubernetes (KUBECONFIG)
- [x] Generic environment variable injection with optional prefix

### 2.2 External Secret Managers
- [x] AWS Secrets Manager integration (`aws-sdk-secretsmanager`)
- [x] HashiCorp Vault integration (KV v2 secrets engine)
- [x] Secret reference syntax in config files:
  ```yaml
  secrets:
    db_password:
      provider: aws-secrets-manager
      key: prod/db/password
  ```

### 2.3 Implementation Details
- `SecretsProvider` trait with `get_secret()` and `health_check()`
- `SecretsResolver` for managing multiple providers
- `SecretValue` wrapper using `secrecy` crate for safe handling
- Support for secret versioning

---

## Milestone 3: Infrastructure Providers [COMPLETED]

### 3.1 AWS EKS (Elastic Kubernetes Service)
**Description**: Managed Kubernetes on AWS

**Configuration Schema**:
```yaml
infrastructure:
  aws-prod:
    type: aws-eks
    config:
      cluster_name: my-cluster
      region: us-east-1
      namespace: default
      service_account: deployment-sa
```

**Supported Deployment Types**:
| Strategy | Implementation |
|----------|----------------|
| Rolling Update | Uses K8s native `RollingUpdate` strategy with `maxUnavailable` and `maxSurge` |
| Blue/Green | Creates new deployment, switches Service selector, deletes old deployment |
| Canary | Uses Ingress weight-based routing or Istio VirtualService |
| Recreate | Scales down to 0, then scales up new version |

**Implementation Details**:
- [x] AWS SDK for Rust (`aws-sdk-eks`) for cluster authentication
- [x] `kube-rs` for Kubernetes API operations
- [x] Generate kubeconfig from EKS cluster details (via `aws eks update-kubeconfig`)
- [x] Support for Helm charts via `helm` subprocess (see Milestone 10)
- [x] Kustomize support (see Milestone 10)
- [x] Manifest templating with Tera (see Milestone 19)

**Files**: `src/infrastructure/aws_eks.rs`

### 3.2 AWS ECS (Elastic Container Service)
**Description**: AWS-native container orchestration

**Configuration Schema**:
```yaml
infrastructure:
  aws-ecs-prod:
    type: aws-ecs
    config:
      cluster: my-cluster
      region: us-east-1
      service_name: my-service
```

**Supported Deployment Types**:
| Strategy | Implementation |
|----------|----------------|
| Rolling Update | ECS native deployment with `minimumHealthyPercent` and `maximumPercent` |
| Blue/Green | CodeDeploy integration with ALB target group switching |
| Canary | CodeDeploy with traffic shifting (10% -> 100%) |

**Implementation Details**:
- [x] AWS SDK for Rust (`aws-sdk-ecs`)
- [x] Task definition registration and versioning
- [x] Service update with deployment configuration
- [x] Wait for service stability
- [x] ALB/NLB target group management (see Milestone 11)
- [x] CloudWatch Logs integration for deployment monitoring (see Milestone 11)
- [x] Auto Scaling configuration (see Milestone 11)

**Files**: `src/infrastructure/aws_ecs.rs`

### 3.3 AWS Lambda
**Description**: Serverless function deployments

**Configuration Schema**:
```yaml
infrastructure:
  aws-lambda-prod:
    type: aws-lambda
    config:
      function_name: my-function
      region: us-east-1
      alias: prod  # Optional, enables versioning
```

**Supported Deployment Types**:
| Strategy | Implementation |
|----------|----------------|
| Direct | Update function code directly (instant switch) |
| Alias Blue/Green | Create new version, shift alias from old to new |
| Canary | Alias with weighted routing (e.g., 10% new, 90% old) |
| Provisioned | Pre-warm instances before traffic shift |

**Implementation Details**:
- [x] AWS SDK for Rust (`aws-sdk-lambda`)
- [x] Container image support (ECR)
- [x] Function versioning and alias management
- [x] ZIP packaging for deployment artifacts (see Milestone 20)
- [x] Environment variable configuration
- [x] Layer management for shared dependencies (see Milestone 20)
- [x] Event source mappings (SQS, SNS, Kinesis, DynamoDB, Kafka, etc.) (see Milestone 20)
- [x] Provisioned concurrency configuration

**Files**: `src/infrastructure/aws_lambda.rs`, `src/infrastructure/lambda_extended.rs`

### 3.4 Kubernetes (Generic)
**Description**: Self-hosted or cloud-agnostic Kubernetes clusters

**Configuration Schema**:
```yaml
infrastructure:
  k8s-onprem:
    type: kubernetes
    config:
      context: my-cluster-context
      namespace: default
```

**Supported Deployment Types**:
| Strategy | Implementation |
|----------|----------------|
| Rolling Update | Native K8s Deployment strategy |
| Blue/Green | Dual Deployment with Service selector switch |
| Canary | Ingress annotations or service mesh routing |
| Recreate | Deployment with `Recreate` strategy |

**Implementation Details**:
- [x] `kube-rs` for all Kubernetes operations
- [x] Support multiple kubeconfig contexts
- [x] Deployment image updates via patch
- [x] Wait for rollout completion
- [x] Helm chart deployment support (see Milestone 10)
- [x] Kustomize support (see Milestone 10)
- [x] Raw manifest application (see Milestone 19)
- [x] ConfigMap and Secret management (see Milestone 19)
- [x] Horizontal Pod Autoscaler configuration (see Milestone 19)
- [x] Manifest templating with Tera (see Milestone 19)

**Files**: `src/infrastructure/kubernetes.rs`, `src/infrastructure/helm.rs`, `src/infrastructure/kustomize.rs`, `src/infrastructure/k8s_resources.rs`, `src/infrastructure/manifest.rs`

### 3.5 Docker Compose (Local/Simple)
**Description**: For local development and simple single-server deployments

**Configuration Schema**:
```yaml
infrastructure:
  local-dev:
    type: docker-compose
    config:
      compose_file: docker-compose.yml
      project_name: my-app
      ssh_host: deploy@server.com  # Optional for remote
      working_dir: /path/to/project  # Optional
```

**Supported Deployment Types**:
| Strategy | Implementation |
|----------|----------------|
| Recreate | `docker compose down && docker compose up -d` |
| Rolling (per-service) | Update services one at a time with health checks |

**Implementation Details**:
- [x] Docker Compose CLI wrapper
- [x] Remote deployment via SSH
- [x] Health check validation
- [x] Volume and network management
- [x] Environment file support

**Files**: `src/infrastructure/docker_compose.rs`

### 3.6 Provider Registry
- [x] `ProviderRegistry` for managing multiple providers
- [x] `ProviderFactory` for creating providers from config
- [x] CLI integration with all providers (deploy, status, rollback, logs)

**Files**: `src/infrastructure/registry.rs`, `src/infrastructure/mod.rs`

---

## Milestone 4: Deployment Strategies (Core) [COMPLETED]

### 4.1 Rolling Update
- [x] Configurable batch size / percentage
- [x] Health check validation between batches
- [x] Automatic rollback on failure threshold
- [x] Progress reporting

**Files**: `src/deployment/rolling.rs`

**Configuration**:
```yaml
environments:
  production:
    deployment_type: rolling-update
    config:
      batch_size: 25  # percentage per batch
      health_check_timeout_secs: 300
      rollback_on_failure: true
```

### 4.2 All-In (Stop-Start)
- [x] Full stop of existing workload (100% batch)
- [x] Start new version
- [x] Uses RollingUpdate with 100% batch internally
- [x] For K8s, uses native Recreate strategy

### 4.3 Strategy Infrastructure
- [x] `StrategyFactory` for creating strategies from config
- [x] `DeploymentExecutor` for strategy orchestration
- [x] `StrategyConfig` for strategy-specific parameters

**Files**: `src/deployment/executor.rs`, `src/deployment/strategy.rs`

---

## Milestone 5: Plugin System [COMPLETED]

### 5.1 Plugin Architecture
- [x] Define plugin interface (Rust traits via `libloading`)
- [x] Plugin discovery from `~/.pmp-deploy/plugins/`
- [x] Plugin configuration schema registration
- [x] Plugin lifecycle management (init/shutdown hooks)

**Implementation Details**:
- FFI-safe types in `src/plugins/ffi.rs` for cross-boundary communication
- `DynamicPlugin` wrapper using `libloading` for native shared libraries
- `PluginRegistry` for managing loaded plugins
- Plugin API version checking for compatibility
- Supports `.so` (Linux), `.dll` (Windows), `.dylib` (macOS)

**Files**: `src/plugins/ffi.rs`, `src/plugins/loader.rs`, `src/plugins/registry.rs`

### 5.2 Plugin SDK
- [x] Create `pmp-deploy-plugin-sdk` crate
- [x] `declare_plugin!` macro for easy plugin creation
- [x] `InfrastructurePlugin` trait with all required methods
- [x] Example plugin implementation (`examples/plugins/echo-plugin`)
- [x] Plugin template generator (see Milestone 21)

**SDK Location**: `crates/pmp-deploy-plugin-sdk/`

**Example Usage**:
```rust
use pmp_deploy_plugin_sdk::*;

declare_plugin!(
    name: "my-plugin",
    version: "0.1.0",
    description: "My infrastructure plugin",
    infrastructure_type: "my-infra",
    plugin: MyPlugin,
);

#[derive(Default)]
struct MyPlugin;

impl InfrastructurePlugin for MyPlugin {
    fn validate_config(&self, config: &PluginConfig) -> PluginResult<()> { Ok(()) }
    fn deploy(&self, ctx: &DeploymentContext) -> PluginResult<DeploymentResult> {
        Ok(DeploymentResult::success("Deployed", "v1.0.0"))
    }
    // ... other methods
}
```

### 5.3 Built-in Plugins (Optional Infrastructure)
- [ ] Google Cloud Run
- [ ] Azure Container Apps
- [ ] DigitalOcean App Platform
- [ ] Fly.io

*Note: These can be implemented as separate plugin crates in the future.*

---

## Milestone 6: CLI Interface [COMPLETED]

### 6.1 Core Commands
```
pmp-deploy deploy <environment>      Deploy to specified environment
pmp-deploy status <environment>      Check deployment status
pmp-deploy rollback <environment>    Rollback to previous version
pmp-deploy logs <environment>        Stream/fetch logs
pmp-deploy list                      List available environments
pmp-deploy validate                  Validate configuration file
pmp-deploy init                      Interactive wizard to configure project
```
**Status**: All core commands fully integrated with infrastructure providers

### 6.1.1 Init Command Interactive Wizard [COMPLETED]
- [x] Interactive environment selection (development, staging, production, custom)
- [x] Infrastructure type selection per environment
- [x] Deploy mode selection (Full: create infrastructure, AppOnly: use existing)
- [x] Connection details prompts for AppOnly mode:
  - Docker Compose: compose_file, project_name
  - AWS EKS: cluster_name, region, namespace
  - AWS ECS: cluster, region, launch_type
  - AWS Lambda: region, function_name_prefix
  - Kubernetes: context, namespace
- [x] `--infrastructure <type>` flag to skip wizard with template
- [x] `--non-interactive` flag to skip wizard with defaults
- [x] `--force` flag to overwrite existing config
- [x] `Prompter` trait for testable prompts (DialoguerPrompter implementation)
- [x] Unit tests with MockPrompter

**Files**: `src/cli/init/handler.rs`, `src/cli/init/wizard/` (types.rs, prompter.rs, flow.rs, generator.rs)

### 6.2 Multi-Project Support
```
pmp-deploy projects list             List all configured projects
pmp-deploy projects add <path>       Add project to global config
pmp-deploy projects remove <path>    Remove project from global config
pmp-deploy                          Interactive project selector (when in non-project dir)
```
**Status**: List command complete, add/remove pending full implementation

### 6.3 Additional Features
- [x] `--dry-run` flag for all deployment operations
- [x] `--verbose` / `--quiet` output modes
- [x] JSON output format (`--output json`)
- [x] Interactive confirmation prompts
- [x] Progress bars and spinners (using `indicatif` crate)

---

## Milestone 7: Web UI [COMPLETED]

### 7.1 HTTP Server
- [x] `pmp-deploy ui` command to start server
- [x] Configurable port (`--port 8080`) and host (`--host`)
- [x] Serve static assets (embedded in binary)
- [x] REST API for all CLI operations
- [x] CORS support for cross-origin requests

### 7.2 Frontend (jQuery + Tailwind CSS)
- [x] Project selector dashboard
- [x] Environment overview with status indicators
- [x] Deploy button with confirmation modal
- [x] Image override and dry-run options
- [x] Log viewer with SSE streaming
- [x] Rollback interface with confirmation
- [x] Environment details viewer (resources, env vars)
- [x] Toast notifications for feedback

### 7.3 API Endpoints
```
GET  /api/health                                    Health check
GET  /api/projects                                  List projects
GET  /api/projects/{id}                             Get project details
GET  /api/projects/{id}/environments                List environments
GET  /api/projects/{id}/environments/{env}          Get environment details
POST /api/projects/{id}/environments/{env}/deploy   Trigger deployment
GET  /api/projects/{id}/environments/{env}/status   Get deployment status
POST /api/projects/{id}/environments/{env}/rollback Trigger rollback
GET  /api/projects/{id}/environments/{env}/logs     Stream logs (SSE)
```

**Implementation Details**:
- `src/ui/api.rs` - API route handlers
- `src/ui/state.rs` - AppState for shared state
- `src/ui/server.rs` - Axum server setup
- `static/index.html` - Embedded frontend

---

## Milestone 8: Testing & Quality [COMPLETED]

### 8.1 Unit Tests
- [x] Configuration parsing tests (15 tests in `tests/config_tests.rs`)
- [x] Strategy selection logic tests (20 tests in `tests/strategy_tests.rs`)
- [x] Plugin loading tests (13 tests in `tests/plugin_tests.rs`)
- [x] Infrastructure tests (16 tests in `tests/infrastructure_tests.rs`)
- [x] Mock external dependencies with tempfile and test fixtures

### 8.2 Integration Tests
- [x] Docker Compose configuration validation tests
- [x] Configuration loader tests with real files
- [x] Strategy validation tests

### 8.3 Documentation
- [x] README.md with comprehensive quick start
- [x] CLAUDE.md for project context
- [x] doc/ folder with detailed guides:
  - `index.md` - Documentation index
  - `getting-started.md` - Installation and first steps
  - `deployment-strategies.md` - Strategy comparison and usage
- [x] Example configuration files in `examples/configs/`:
  - `docker-compose.yaml` - Local development
  - `kubernetes.yaml` - K8s clusters
  - `aws-eks.yaml` - AWS EKS
  - `aws-ecs.yaml` - AWS ECS Fargate
  - `aws-lambda.yaml` - Serverless
  - `complete-example.yaml` - Multi-infrastructure
  - `global-config.yaml` - Multi-project setup

**Test Summary**: 154 tests total (90 unit + 64 integration)

---

## Milestone 9: Production Readiness [COMPLETED]

### 9.1 Error Handling & Resilience
- [x] Comprehensive error messages (`src/error.rs`)
  - `ConfigError`, `InfrastructureError`, `DeploymentError`
  - `PluginError`, `SecretsError`, `IoError`, `NetworkError`, `AuthError`
  - Automatic error conversion with `From` implementations
- [x] Retry logic with exponential backoff (`src/retry.rs`)
  - Configurable `RetryConfig` with max attempts, delays, jitter
  - `retry()` and `retry_if()` async functions
  - Presets: `local()`, `api()`, `critical()`, `patient()`
- [x] Graceful interruption handling (`src/signal.rs`)
  - `SignalHandler` with Ctrl+C and SIGTERM support
  - `InterruptibleContext` for async operation interruption
  - `check_interrupt!` macro for loop interruption
- [x] State recovery for interrupted deployments (see Milestone 18)

### 9.2 Observability
- [x] Structured logging (`src/logging.rs`)
  - `LogFormat`: Text, JSON, Compact
  - `LogConfig` with timestamps, file info, span events
  - Presets: `verbose()`, `quiet()`, `json()`
- [ ] Deployment event history (future)
- [ ] Metrics export (optional Prometheus endpoint) (future)

### 9.3 Security
- [x] Credential masking in logs (`src/logging.rs`)
  - `mask_sensitive()` function for redacting secrets
  - `is_sensitive_key()` detection
  - `SensitiveValue` wrapper for safe logging
- [x] Config file permission validation (`src/security.rs`)
  - `SecurityCheck` struct with warnings and errors
  - `validate_file_permissions()` (Unix/Windows)
  - `check_sensitive_content()` for detecting hardcoded secrets
  - `validate_secret_references()` for config validation
- [x] HTTPS for UI server
  - `axum-server` with TLS rustls support
  - `--tls-cert` and `--tls-key` CLI options
  - `--dev-tls` for self-signed development certificates
  - `TlsConfig` validation before server start
- [ ] Authentication for UI (optional) (future)

---

## Milestone 10: Helm & Kustomize Support [COMPLETED]

### 10.1 Helm Integration
- [x] `HelmConfig` struct for Helm chart configuration
  - Chart path (local or OCI registry)
  - Repository configuration (name, URL)
  - Version pinning
  - Values files and inline `--set` values
  - Deployment options (wait, atomic, timeout, create-namespace)
- [x] `HelmDeployer` for Helm operations
  - Repository management (add, update)
  - `helm upgrade --install` for deployments
  - `helm rollback` for rollbacks
  - `helm status` and `helm history` for status
  - Dry-run support
- [x] Configuration merging (infrastructure + environment level)

**Files**: `src/infrastructure/helm.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-helm:
    type: kubernetes
    config:
      namespace: default
      helm:
        chart: ./charts/my-app
        release_name: my-app
        values_files:
          - values.yaml
        wait: true
        atomic: true

environments:
  production:
    infrastructure: k8s-helm
    config:
      helm:
        set:
          replicaCount: "5"
```

### 10.2 Kustomize Integration
- [x] `KustomizeConfig` struct for Kustomize configuration
  - Overlay path
  - Image overrides (name, new_name, new_tag, digest)
  - Replica overrides
  - Labels and annotations
  - Namespace override
  - Prune support with whitelist
- [x] `KustomizeDeployer` for Kustomize operations
  - `kubectl kustomize` for building manifests
  - `kubectl apply` with kustomization
  - `kubectl diff` for dry-run
  - Resource counting
- [x] Configuration merging (infrastructure + environment level)

**Files**: `src/infrastructure/kustomize.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-kustomize:
    type: kubernetes
    config:
      namespace: default
      kustomize:
        path: ./k8s/base

environments:
  production:
    infrastructure: k8s-kustomize
    config:
      kustomize:
        path: ./k8s/overlays/production
        images:
          - name: my-app
            new_tag: v1.0.0
        replicas:
          - name: my-deployment
            count: 5
```

### 10.3 Deployment Method Selection
- [x] `DeploymentMethod` enum (Direct, Helm, Kustomize)
- [x] Automatic detection from configuration
- [x] Explicit override via `deployment_method` config key
- [x] Method-specific deploy, rollback, and status operations

**Files**: `src/infrastructure/kubernetes.rs`

### 10.4 Documentation
- [x] Example configurations in `examples/configs/`
  - `helm-deployment.yaml` - Helm chart deployments
  - `kustomize-deployment.yaml` - Kustomize overlay deployments
- [x] Updated README with Helm and Kustomize usage
- [x] Updated CLAUDE.md with new file references

---

## Milestone 11: AWS ECS Enhancements [COMPLETED]

### 11.1 Load Balancer Integration
- [x] `LoadBalancerConfig` struct for ALB/NLB configuration
  - Target group ARN
  - Container name and port mapping
  - Health check configuration (path, intervals, thresholds)
- [x] `get_target_group_health()` - Monitor target health during deployment
- [x] `update_target_group_health_check()` - Configure health check settings
- [x] Integration with deployment workflow (health reporting after deploy)

**Configuration Example**:
```yaml
load_balancer:
  target_group_arn: arn:aws:elasticloadbalancing:...
  container_name: app
  container_port: 8080
  health_check:
    path: /health
    interval_seconds: 15
    healthy_threshold: 2
```

### 11.2 CloudWatch Logs Integration
- [x] `CloudWatchLogsConfig` struct for log configuration
  - Log group name
  - Log stream prefix
  - Retention policy (days)
  - Auto-create log group option
- [x] `ensure_log_group()` - Create log group if needed
- [x] `get_recent_logs()` - Fetch logs from CloudWatch directly
- [x] Log streaming support via AWS CLI
- [x] Integration with `logs` command

**Configuration Example**:
```yaml
cloudwatch_logs:
  log_group: /ecs/my-service
  retention_days: 14
  create_log_group: true
```

### 11.3 Auto Scaling Support
- [x] `AutoScalingConfig` struct for scaling configuration
  - Min/max capacity
  - Target tracking policies
  - Scheduled scaling actions
- [x] Target tracking metrics support:
  - ECSServiceAverageCPUUtilization
  - ECSServiceAverageMemoryUtilization
  - ALBRequestCountPerTarget
- [x] `configure_autoscaling()` - Register scalable target and policies
- [x] `get_autoscaling_status()` - Query current scaling configuration
- [x] Integration with deployment (configure scaling after deploy)
- [x] Status command shows auto-scaling info

**Configuration Example**:
```yaml
autoscaling:
  enabled: true
  min_capacity: 2
  max_capacity: 20
  target_tracking:
    target_value: 70.0
    metric_type: ECSServiceAverageCPUUtilization
    scale_in_cooldown: 300
  scheduled:
    - name: scale-up-morning
      schedule: cron(0 8 ? * MON-FRI *)
      min_capacity: 5
```

### 11.4 Documentation
- [x] Example configuration in `examples/configs/ecs-deployment.yaml`
- [x] Updated README with ECS feature documentation
- [x] Updated ROADMAP with Milestone 11

**Files**: `src/infrastructure/aws_ecs.rs`

---

## Milestone 12: App-Only Deployment Mode [COMPLETED]

### 12.1 Deploy Mode Enum
- [x] `DeployMode` enum with `Full` and `AppOnly` variants
- [x] Serde serialization/deserialization (`full`, `app-only`)
- [x] `from_str()` parsing with multiple aliases (`app-only`, `apponly`, `app`, `image-only`)
- [x] `is_app_only()` helper method
- [x] Default is `AppOnly` mode (safe by default - only updates application code)

**Files**: `src/infrastructure/provider.rs`

### 12.2 Provider Integration
- [x] `DeploymentContext` includes `deploy_mode` field
- [x] AWS ECS provider: app-only mode skips auto-scaling configuration
- [x] AWS Lambda provider: app-only mode skips provisioned/reserved concurrency
- [x] Kubernetes provider: app-only mode bypasses Helm/Kustomize, uses direct image update
- [x] All other operations (rollback, status, logs) use Full mode by default

**Files**: `src/infrastructure/aws_ecs.rs`, `src/infrastructure/aws_lambda.rs`, `src/infrastructure/kubernetes.rs`

### 12.3 AWS Lambda Infrastructure Settings (Full Mode)
- [x] Provisioned concurrency configuration via `provisioned_concurrency` config
- [x] Reserved concurrency configuration via `reserved_concurrency` config
- [x] These settings are only applied in `--deploy-mode full`

### 12.4 CLI Integration
- [x] `--deploy-mode` flag on deploy command (default: `app-only`)
- [x] Web UI API accepts `deploy_mode` in deploy request body
- [x] `DeploymentExecutor` propagates deploy mode to providers

**Files**: `src/cli/commands.rs`, `src/main.rs`, `src/ui/api.rs`, `src/deployment/executor.rs`

### 12.5 What Gets Skipped in App-Only Mode

| Provider | Skipped in App-Only Mode |
|----------|-------------------------|
| AWS ECS | Auto-scaling policies, scheduled scaling actions |
| AWS Lambda | Provisioned concurrency, reserved concurrency |
| Kubernetes | Uses direct image update instead of Helm/Kustomize |

### 12.6 Usage Examples
```bash
# App-only deployment (default) - only updates the container image
pmp-deploy deploy production

# Full deployment - updates app + infrastructure settings
pmp-deploy deploy production --deploy-mode full

# Quick hotfix without touching infrastructure settings
pmp-deploy deploy staging --image myapp:hotfix-123
```

### 12.7 Documentation & Tests
- [x] Unit tests for DeployMode (parsing, serialization, helper methods)
- [x] Updated README with deploy mode documentation
- [x] Updated ROADMAP with Milestone 12

**Test Count**: 178 tests passing (+ 1 ignored)

---

## Milestone 13: Environment Variables & Storage [COMPLETED]

### 13.1 Environment Variable Configuration System
- [x] `EnvVarSource` enum for variable sources:
  - `StaticValue` - Inline static values
  - `Environment` - OS environment variables
  - `AwsSecretsManager` - AWS Secrets Manager secrets
  - `Vault` - HashiCorp Vault secrets
- [x] `EnvVarConfig` struct with flexible configuration:
  - `source` - Variable source type
  - `value` - Static value (when source is static_value)
  - `arn` / `key` - Secret identifier for secret managers
  - `env_var` - Environment variable name to read from
  - `json_field` - Extract specific field from JSON secrets
  - `version` - Secret version (optional)
- [x] `EnvVarResolver` for resolving variables from multiple sources
- [x] Backward compatible with legacy `env:` format

**Files**: `src/config/env_vars.rs`, `src/config/schema.rs`

**Configuration Example**:
```yaml
environments:
  production:
    # Legacy format (still supported)
    env:
      SIMPLE_VAR: "static-value"

    # New flexible format
    environment:
      DATABASE_URL:
        source: static_value
        value: "postgres://localhost/mydb"

      API_KEY:
        source: environment
        env_var: MY_API_KEY

      DB_PASSWORD:
        source: aws_secrets_manager
        arn: arn:aws:secretsmanager:us-east-1:123456789:secret:prod/db
        json_field: password

      VAULT_SECRET:
        source: vault
        key: secret/data/myapp
        json_field: api_token
```

### 13.2 Provider Integration
- [x] `DeploymentContext.resolve_env_vars()` method for resolving all variables
- [x] `DeploymentContext.resolve_env_vars_with()` for custom resolver
- [x] AWS ECS: Environment variables set in task definition container
- [x] AWS Lambda: Environment variables set via `update_function_configuration`
- [x] Automatic resolution at deploy time

**Files**: `src/infrastructure/provider.rs`, `src/infrastructure/aws_ecs.rs`, `src/infrastructure/aws_lambda.rs`

### 13.3 Storage Abstraction
- [x] `Storage` trait for deployment history persistence
  - `save_deployment()` - Store deployment record
  - `get_deployment()` - Retrieve by ID
  - `list_deployments()` - List with filters (project, environment, limit)
  - `get_latest_deployment()` - Get most recent for project/environment
  - `cleanup()` - Remove old records (retention policy)
  - `health_check()` - Verify storage backend health
- [x] `DeploymentRecord` struct for tracking deployments:
  - ID, project, environment, infrastructure type
  - Image, previous image (for rollback)
  - Status (InProgress, Success, Failed, RolledBack)
  - Timestamps, duration, triggered by
  - Deploy mode (full/app-only)
  - Metadata for custom fields
- [x] `DeploymentStatus` enum with `is_terminal()` helper
- [x] `InMemoryStorage` default implementation

**Files**: `src/storage/mod.rs`, `src/storage/traits.rs`, `src/storage/record.rs`, `src/storage/memory.rs`

### 13.4 Documentation & Tests
- [x] 16 new storage tests (memory, record)
- [x] 8 new env_vars tests
- [x] Updated exports in `src/lib.rs`
- [x] Updated ROADMAP with Milestone 13

**Test Count**: 204 tests passing (+ 1 ignored) → Updated to 256 tests in Milestone 14

---

## Milestone 14: Full Infrastructure Provisioning [COMPLETED]

### 14.1 Overview
Enable optional full provisioning of infrastructure resources when they don't exist, and manage state differences when they do.

### 14.2 AWS Lambda Full Provisioning
- [x] `LambdaProvisioningConfig` struct:
  - Runtime, handler, memory, timeout
  - Role ARN or auto-create execution role
  - VPC configuration (subnets, security groups)
  - Environment variables
  - Layers
  - Dead letter queue configuration
- [x] State detection: Check if function exists via `get_function()`
- [x] Create flow: `create_function()` with full configuration
- [x] Update flow: Compare current vs desired config, apply differences via `update_function_configuration()`
- [x] State diff reporting: Show what will change before applying

**Files**: `src/infrastructure/provisioning/lambda.rs`, `src/infrastructure/aws_lambda.rs`

**Configuration Example**:
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
        memory_mb: 256
        timeout_secs: 30
        role_arn: arn:aws:iam::123456789:role/lambda-exec
        vpc:
          subnet_ids:
            - subnet-abc123
          security_group_ids:
            - sg-xyz789
```

### 14.3 AWS ECS Full Provisioning
- [x] `EcsProvisioningConfig` struct:
  - Cluster configuration (capacity providers, settings)
  - Task definition (CPU, memory, container definitions)
  - Service configuration (desired count, deployment config, placement)
  - Network configuration (subnets, security groups, assign public IP)
  - Load balancer configuration
- [x] State detection: Check if cluster/service exists
- [x] Create flow:
  - `create_cluster()` if cluster doesn't exist
  - `register_task_definition()` for new task definition
  - `create_service()` if service doesn't exist
- [x] Update flow:
  - Compare cluster settings, update if needed
  - Register new task definition revision
  - `update_service()` with new configuration
- [x] State diff reporting: Show cluster, service, and task definition changes

**Files**: `src/infrastructure/provisioning/ecs.rs`, `src/infrastructure/aws_ecs.rs`

**Configuration Example**:
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
          default_capacity_provider: FARGATE
        task:
          cpu: "256"
          memory: "512"
          execution_role_arn: arn:aws:iam::123456789:role/ecs-exec
          task_role_arn: arn:aws:iam::123456789:role/ecs-task
        service:
          desired_count: 2
          deployment:
            minimum_healthy_percent: 100
            maximum_percent: 200
          network:
            subnets:
              - subnet-abc123
            security_groups:
              - sg-xyz789
            assign_public_ip: true
```

### 14.4 Kubernetes (via Helm)
- [x] Already handled by Helm's `upgrade --install` pattern
- [x] Helm creates resources if they don't exist, updates if they do
- [x] No additional work needed

### 14.5 Provisioning CLI Integration
- [x] `pmp-deploy provision <environment>` standalone command
- [x] `pmp-deploy provision --plan <environment>` to show planned changes without applying
- [x] `pmp-deploy provision --dry-run <environment>` for dry-run mode
- [x] `pmp-deploy provision --image <uri> <environment>` to override image
- [x] `pmp-deploy provision -y <environment>` to skip confirmation
- [x] Confirmation prompts for provisioning changes

**Files**: `src/cli/commands.rs`, `src/main.rs`

### 14.6 Testing & Documentation
- [x] Lambda provisioning unit tests (16 tests in `src/infrastructure/provisioning/lambda.rs`)
- [x] ECS provisioning unit tests (20 tests in `src/infrastructure/provisioning/ecs.rs`)
- [x] CLI provision command tests (33 tests in `tests/cli_tests.rs`)
- [x] Fixed CLI argument conflicts:
  - Changed logs `-f` to `-F` (avoid conflict with global `-f` file flag)
  - Changed rollback `--version` to `--to-version` (avoid conflict with clap version flag)
- [x] Updated README with provisioning documentation
- [x] Updated CLAUDE.md with provisioning types

### 14.7 Deployment Type Validation
- [x] Removed Canary deployment strategy (not supported by any infrastructure)
- [x] Removed Blue-Green from Kubernetes/EKS (was not actually implemented - just used RollingUpdate)
- [x] Added `supported_deployment_types()` method to `InfrastructureProvider` trait
- [x] Added `validate_deployment_type()` method with default implementation
- [x] Providers now return error if unsupported deployment type is used:
  - **AWS ECS**: `rolling-update`, `recreate`, `direct`
  - **AWS Lambda**: `direct` only (deployments are atomic)
  - **AWS EKS**: `rolling-update`, `recreate`, `direct`
  - **Kubernetes**: `rolling-update`, `recreate`, `direct`
  - **Docker Compose**: `recreate`, `direct` only
  - **Plugins**: All types (plugins handle validation internally)

**Test Count**: 248 tests passing (+ 1 ignored)
- 155 lib tests
- 33 CLI tests
- 15 config tests
- 16 infrastructure tests
- 13 plugin tests
- 16 strategy tests

**Files**: `tests/cli_tests.rs`, `src/infrastructure/provisioning/lambda.rs`, `src/infrastructure/provisioning/ecs.rs`, `src/infrastructure/provider.rs`

---

## Milestone 15: Metrics Observation [COMPLETED]

### 15.1 Overview
Real-time metrics observation during deployments to monitor health, performance, and detect issues early.

### 15.2 Metrics Provider System
- [x] `MetricsProvider` trait for metrics backends:
  - `query_metric()` - Query time-series data
  - `get_current_value()` - Get current gauge value
  - `query_custom()` - Custom provider-specific queries
  - `health_check()` - Verify provider connectivity
- [x] `MetricsResolver` registry pattern (follows SecretsResolver pattern)
- [x] `StandardMetric` enum: CpuUtilization, MemoryUtilization, ErrorRate, RequestLatency, RequestCount
- [x] `TimeRange` helper for common time windows (30m, 1h, 6h, 24h)
- [x] `MetricGauge` with thresholds and status (Normal, Warning, Critical)

**Files**: `src/metrics/mod.rs`, `src/metrics/provider.rs`, `src/metrics/resolver.rs`

### 15.3 CloudWatch Provider
- [x] `CloudWatchProvider` for AWS infrastructure (ECS, EKS, Lambda)
- [x] `CloudWatchProviderConfig` with cluster/service/function configuration
- [x] Standard metric queries:
  - ECS: CPUUtilization, MemoryUtilization from AWS/ECS namespace
  - EKS: pod_cpu_utilization, pod_memory_utilization from ContainerInsights
  - Lambda: Errors, Duration, Invocations from AWS/Lambda
- [x] Custom metric queries via JSON configuration
- [x] Health check via `list_metrics()` API

**Files**: `src/metrics/cloudwatch.rs`

### 15.4 Prometheus Provider
- [x] `PrometheusProvider` for Kubernetes and Docker infrastructure
- [x] `PrometheusProviderConfig` with URL, auth, namespace filtering
- [x] PromQL query building for standard metrics
- [x] Custom metric queries via PromQL strings
- [x] Query range and instant query support
- [x] Basic auth support for secured Prometheus instances
- [x] Health check via `/-/healthy` endpoint

**Files**: `src/metrics/prometheus.rs`

### 15.5 Configuration Schema
- [x] `MetricsConfig` in config schema:
  - `cloudwatch` - CloudWatch provider configuration
  - `prometheus` - Prometheus provider configuration
  - `infrastructure_mapping` - Map infra types to providers
  - `thresholds` - Global threshold configuration
  - `custom_metrics` - Custom metric definitions
- [x] `ThresholdPair` for warning/critical thresholds
- [x] `CustomMetricDefinition` for user-defined metrics

**Files**: `src/config/schema.rs`

**Configuration Example**:
```yaml
metrics:
  cloudwatch:
    region: us-east-1
    ecs_cluster: my-cluster
    ecs_service: my-service

  prometheus:
    url: http://prometheus:9090
    timeout_seconds: 30

  infrastructure_mapping:
    aws-ecs: cloudwatch
    aws-eks: cloudwatch
    kubernetes: prometheus

  thresholds:
    cpu_utilization: { warning: 70, critical: 90 }
    memory_utilization: { warning: 80, critical: 95 }
    error_rate: { warning: 1, critical: 5 }

  custom_metrics:
    - name: queue-depth
      display_name: "Queue Depth"
      provider: cloudwatch
      query: '{"MetricName": "ApproximateNumberOfMessagesVisible", ...}'
      unit: messages
      thresholds: { warning: 100, critical: 1000 }
```

### 15.6 API Endpoints
- [x] `GET /api/projects/{id}/environments/{env}/metrics` - List available metrics
- [x] `GET /api/projects/{id}/environments/{env}/metrics/{name}` - Get time-series data
- [x] `GET /api/projects/{id}/environments/{env}/metrics/{name}/current` - Get current value
- [x] `GET /api/projects/{id}/environments/{env}/metrics/stream` - SSE real-time stream

**Files**: `src/ui/api.rs`, `src/ui/state.rs`

### 15.7 SSE Streaming
- [x] `MetricBroadcaster` for pub/sub metric events
- [x] `MetricEvent` enum: TimeSeries, Gauge, AllGauges, Error, Heartbeat
- [x] `MetricStreamConfig` with configurable intervals
- [x] Keep-alive heartbeats for connection persistence

**Files**: `src/metrics/stream.rs`

### 15.8 Web UI
- [x] Chart.js integration for time-series visualization
- [x] 5 gauge cards for standard metrics with color-coded status
- [x] 2 time-series charts (CPU/Memory, Requests/Latency)
- [x] Time range selector (30min, 1h, 6h, 24h)
- [x] Live streaming toggle with SSE
- [x] Automatic refresh on time range change

**Files**: `static/index.html`

### 15.9 Example Configurations
- [x] `examples/configs/metrics-cloudwatch.yaml` - CloudWatch-only setup
- [x] `examples/configs/metrics-prometheus.yaml` - Prometheus-only setup
- [x] `examples/configs/metrics-complete.yaml` - Multi-provider setup
- [x] Updated app-only/full examples for all infrastructure types

**Test Count**: 255 tests passing (+ 1 ignored)
- 164 lib tests (incl. 12 new metrics tests)
- 33 CLI tests
- 15 config tests
- 16 infrastructure tests
- 13 plugin tests
- 13 strategy tests

---

## Milestone 16: Pre/Post Deployment Hooks [COMPLETED]

### 16.1 Overview
Execute custom tasks before and/or after deployments for database migrations, cache warming, smoke tests, notifications, etc.

### 16.2 Hook Configuration Schema
- [x] `HookConfig` struct:
  - `name` - Hook identifier
  - `type` - Hook type (container, http, ecs_task, k8s_job, lambda)
  - `when` - Execution timing (pre_deploy, post_deploy, on_failure, on_success)
  - `timeout_secs` - Maximum execution time
  - `fail_on_error` - Whether hook failure should fail deployment
  - Type-specific configuration

**Configuration Example**:
```yaml
environments:
  production:
    hooks:
      pre_deploy:
        - name: run-migrations
          type: container
          config:
            image: myapp/migrations:latest
            command: ["./migrate.sh", "--env", "production"]
            env:
              DATABASE_URL:
                source: aws_secrets_manager
                arn: arn:aws:secretsmanager:...
          timeout_secs: 300
          fail_on_error: true

        - name: notify-start
          type: http
          config:
            url: https://slack.com/webhook/...
            method: POST
            body: '{"text": "Deployment starting for production"}'

      post_deploy:
        - name: smoke-tests
          type: container
          config:
            image: myapp/tests:latest
            command: ["./smoke-tests.sh"]
          timeout_secs: 120
          fail_on_error: false

        - name: warm-cache
          type: http
          config:
            url: https://api.example.com/warm-cache
            method: POST
```

### 16.3 AWS Lambda Hooks
Lambda functions are invoked directly without persistent compute, so hooks require special handling:

- [x] **Pre-deploy hooks**:
  - Invoke another Lambda function synchronously
  - Make HTTP request to external endpoint
  - Run container via ECS RunTask (one-shot)
- [x] **Post-deploy hooks**:
  - Same options as pre-deploy
  - Invoke the deployed function itself with a test event
- [x] `LambdaHookConfig`:
  - `function_arn` - ARN of Lambda to invoke
  - `invoke_self` - Invoke the deployed function itself
  - `payload` - JSON payload for invocation
  - `invocation_type` - RequestResponse or Event
  - `qualifier` - Alias or version

**Configuration Example**:
```yaml
environments:
  production:
    infrastructure: aws-lambda-prod
    hooks:
      pre_deploy:
        - name: check-dependencies
          type: lambda
          config:
            function_arn: arn:aws:lambda:us-east-1:123456789:function:check-deps
            payload: '{"env": "production"}'

      post_deploy:
        - name: smoke-test
          type: lambda
          config:
            invoke_self: true
            payload: '{"action": "health-check"}'
```

### 16.4 AWS ECS Hooks
- [x] **One-shot ECS tasks** for pre/post deployment:
  - `run_task()` API for executing containers
  - Wait for task completion with polling
  - Capture exit code and stop reason
- [x] `EcsTaskHookConfig`:
  - `image` - Container image (defaults to same as service)
  - `image_tag` - Override tag
  - `command` - Command override
  - `entrypoint` - Entrypoint override
  - `cpu` / `memory` - Resource overrides
  - `environment` - Environment variables
  - `task_role_arn` - Override task role
  - `subnets` / `security_groups` - VPC settings
  - `capacity_provider_strategy` - ECS capacity providers
  - `region` - AWS region override

**Configuration Example**:
```yaml
environments:
  production:
    infrastructure: aws-ecs-prod
    hooks:
      pre_deploy:
        - name: run-migrations
          type: ecs_task
          config:
            image: myapp/api
            image_tag: "${IMAGE_TAG}"
            command: ["./migrate.sh", "--apply"]
            memory: "512"
            environment:
              RUN_MODE: migration
          timeout_secs: 600
          fail_on_error: true

      post_deploy:
        - name: clear-cache
          type: ecs_task
          config:
            image: redis:alpine
            command: ["redis-cli", "-h", "cache.internal", "FLUSHALL"]
          timeout_secs: 30
```

### 16.5 Kubernetes Hooks
- [x] **One-shot Kubernetes Jobs** for pre/post deployment:
  - Create Job resource via kube-rs
  - Wait for completion with polling
  - Track succeeded/failed pod counts
  - Clean up Job after completion (configurable)
- [x] `K8sJobHookConfig`:
  - `image` - Container image
  - `image_tag` - Override tag
  - `command` - Command array
  - `args` - Argument array
  - `env` - Environment variables
  - `env_from` - ConfigMap/Secret references
  - `resources` - CPU/memory requests/limits
  - `service_account` - ServiceAccount to use
  - `cleanup` - Whether to delete Job after completion
  - `backoff_limit` - Number of retries
  - `restart_policy` - Never or OnFailure
  - `labels` / `annotations` - Job metadata

**Configuration Example**:
```yaml
environments:
  production:
    infrastructure: k8s-prod
    hooks:
      pre_deploy:
        - name: db-migration
          type: k8s_job
          config:
            image: myapp/migrations
            image_tag: "${IMAGE_TAG}"
            command: ["./migrate.sh"]
            env:
              RAILS_ENV: production
            env_from:
              - secret: db-credentials
            resources:
              limits:
                memory: "512Mi"
            service_account: migration-runner
            backoff_limit: 2
            cleanup: true
          timeout_secs: 600
          fail_on_error: true

      post_deploy:
        - name: integration-tests
          type: k8s_job
          config:
            image: myapp/e2e-tests
            command: ["pytest", "-v", "--env=production"]
          timeout_secs: 300
          fail_on_error: false
```

### 16.6 Hook Execution Engine
- [x] `HookExecutor` trait for different hook types
- [x] `HookRunner` for orchestrating hook execution
- [x] `HookContext` with environment and infrastructure info
- [x] Sequential execution with timeout handling
- [x] Dry-run support for testing hooks
- [x] Failure handling (fail_on_error flag)

**Files**: `src/hooks/mod.rs`, `src/hooks/executor.rs`, `src/hooks/types.rs`, `src/hooks/container.rs`, `src/hooks/http.rs`, `src/hooks/ecs.rs`, `src/hooks/k8s.rs`, `src/hooks/lambda.rs`

### 16.7 CLI Integration
- [x] `--skip-hooks` flag to bypass all hooks
- [x] `--skip-pre-hooks` / `--skip-post-hooks` for selective skip
- [x] `pmp-deploy hooks list <environment>` to view configured hooks
- [x] `pmp-deploy hooks run <environment> <hook-name>` for manual execution

### 16.8 Additional Hook Types
- [x] **Container hooks** (`ContainerHookExecutor`): Run Docker containers locally
- [x] **HTTP hooks** (`HttpHookExecutor`): Make HTTP/HTTPS requests for webhooks

**Test Count**: 279 tests passing (+ 1 ignored)
- 189 lib tests (including hooks tests)
- 33 CLI tests
- 15 config tests
- 16 infrastructure tests
- 13 plugin tests
- 13 strategy tests

---

## Milestone 17: Deployment History Persistence [COMPLETED]

### 17.1 Overview
Persistent storage for deployment history across sessions with configurable backends (file-based and SQLite).

### 17.2 Storage Backends

#### File-Based Storage
- [x] `FileStorage` implementation with JSON files
- [x] Directory structure: `<base_path>/deployments/*.json` + `index.json`
- [x] Atomic writes using temp file + rename pattern
- [x] Index file for fast listing and filtering
- [x] Automatic parent directory creation
- [x] Default location: `~/.pmp-deploy/history/`

**Files**: `src/storage/file.rs`

#### SQLite Storage
- [x] `SqliteStorage` implementation with bundled SQLite
- [x] Schema: `deployments` table with proper indexes
- [x] ACID-compliant transactions
- [x] Full-text search ready structure
- [x] In-memory mode for testing
- [x] Default location: `~/.pmp-deploy/history.db`

**Files**: `src/storage/sqlite.rs`

### 17.3 Storage Factory
- [x] `StorageBackend` enum: `Memory`, `File`, `Sqlite`
- [x] `StorageConfig` with:
  - `backend` - Which storage to use
  - `path` - Custom storage location (optional)
  - `retention_days` - Cleanup policy (default: 90 days)
- [x] `StorageFactory::create()` - Create storage from config
- [x] `StorageFactory::create_default()` - SQLite at default location
- [x] `StorageFactory::create_memory()` - In-memory for testing

**Files**: `src/storage/factory.rs`, `src/storage/mod.rs`

### 17.4 Global Configuration
- [x] `GlobalConfig.storage` field for storage configuration
- [x] YAML configuration support:
```yaml
storage:
  backend: sqlite
  path: /custom/path/history.db
  retention_days: 60
```

**Files**: `src/config/schema.rs`

### 17.5 UI/API Integration
- [x] `AppState.get_storage()` - Lazy initialization from global config
- [x] `AppState.init_storage()` - Manual initialization with custom config

**API Endpoints**:
- [x] `GET /api/deployments` - List all deployments
- [x] `GET /api/projects/{id}/deployments` - List project deployments
- [x] `GET /api/projects/{id}/environments/{env}/deployments` - List environment deployments
- [x] `GET /api/projects/{id}/deployments/{deployment_id}` - Get deployment details
- [x] `POST /api/deployments/cleanup` - Cleanup old deployments

**Files**: `src/ui/state.rs`, `src/ui/api.rs`

### 17.6 API Response Types
- [x] `DeploymentItem` - Summary view of a deployment
- [x] `DeploymentDetailResponse` - Full deployment details
- [x] `DeploymentsListResponse` - Paginated list of deployments
- [x] `CleanupRequest` / `CleanupResponse` - Cleanup operation

### 17.7 Tests
- [x] FileStorage tests (9 tests)
- [x] SqliteStorage tests (12 tests)
- [x] StorageFactory tests (8 tests)
- [x] All existing tests continue to pass

**Test Count**: 219 lib tests passing (+45 storage tests)

---

## Milestone 18: State Recovery for Interrupted Deployments [COMPLETED]

### 18.1 Overview
Enable resumption of interrupted deployments through checkpoint persistence, allowing deployments to continue from where they left off after Ctrl+C or system failures.

### 18.2 Checkpoint Data Model
- [x] `DeploymentPhase` enum for tracking deployment progress:
  - PreHooks, InfrastructureProvisioning, AppDeployment, HealthCheck, PostHooks, Completed
  - `ordinal()` method for phase comparison
  - `next()` for phase advancement
- [x] `DeployedResource` struct for tracking deployed resources:
  - Resource type, name, namespace
  - Timestamp and metadata
- [x] `DeploymentCheckpoint` struct:
  - Deployment ID and current phase
  - Completed phases list
  - Deployed resources list
  - Pre/post hooks completed
  - Context snapshot (JSON) for resumption
  - Error message if failed

**Files**: `src/storage/record.rs`

### 18.3 Storage Trait Extension
- [x] Extended `Storage` trait with checkpoint methods:
  - `save_checkpoint()` - Persist checkpoint state
  - `get_checkpoint()` - Retrieve checkpoint by deployment ID
  - `delete_checkpoint()` - Remove checkpoint (on completion)
  - `list_active_checkpoints()` - List resumable deployments
  - `cleanup_checkpoints()` - Remove old checkpoints

**Files**: `src/storage/traits.rs`

### 18.4 Storage Backend Implementation
- [x] **InMemoryStorage**: HashMap-based checkpoint storage
- [x] **FileStorage**: JSON files in `<base_path>/checkpoints/` directory
- [x] **SqliteStorage**: `checkpoints` table with proper indexes

**Files**: `src/storage/memory.rs`, `src/storage/file.rs`, `src/storage/sqlite.rs`

### 18.5 CheckpointManager
- [x] `CheckpointManager` for checkpoint lifecycle management:
  - Create checkpoints at deployment start
  - Update checkpoints as phases complete
  - Save on interrupt (SIGINT/SIGTERM)
  - Clear on successful completion
- [x] `CheckpointConfig` for enabling/disabling checkpoints
- [x] `InterruptedError` for interrupt handling
- [x] Signal handler integration for automatic checkpoint save

**Files**: `src/deployment/checkpoint.rs`

### 18.6 Executor Integration
- [x] `HookExecutionConfig` for selective hook execution
- [x] `DeploymentExecutor.execute_with_hooks()` - Phased execution with checkpoints
- [x] `DeploymentExecutor.resume_from_checkpoint()` - Resume interrupted deployment
- [x] Phase-aware execution:
  - Skip completed phases on resume
  - Skip completed hooks on resume
  - Track deployed resources

**Files**: `src/deployment/executor.rs`, `src/deployment/mod.rs`

### 18.7 CLI Integration
- [x] `pmp-deploy resume <deployment-id>` - Resume interrupted deployment
- [x] `pmp-deploy resume --list` - List resumable deployments
- [x] `pmp-deploy resume --clear <id>` - Clear checkpoint without resuming
- [x] `-y` flag to skip confirmation

**Files**: `src/cli/commands.rs`, `src/cli/resume/mod.rs`, `src/main.rs`

### 18.8 Usage Example
```bash
# Start a deployment, interrupt with Ctrl+C
pmp-deploy deploy production
^C  # Checkpoint saved automatically

# List resumable deployments
pmp-deploy resume --list
# Output:
# Resumable deployments:
#   ID: dep_production_1704672000000
#     Phase: app_deployment
#     Completed phases: pre_hooks
#     Resources deployed: 2
#     Last updated: ...

# Resume the deployment
pmp-deploy resume dep_production_1704672000000
```

### 18.9 Tests
- [x] Checkpoint type tests in `src/storage/record.rs`
- [x] Storage backend checkpoint tests (Memory, File, SQLite)
- [x] CheckpointManager tests in `src/deployment/checkpoint.rs`
- [x] Resume CLI tests in `src/cli/resume/mod.rs`

**Test Count**: 355 tests passing

---

## Milestone 19: Kubernetes Extended Features [COMPLETED]

### 19.1 Overview
Extended Kubernetes deployment options including manifest templating, raw manifest application, ConfigMap/Secret management, and Horizontal Pod Autoscaler (HPA) configuration.

### 19.2 Manifest Templating
- [x] `ManifestTemplateConfig` for template configuration:
  - Template path (file or directory)
  - Custom variables map
  - Environment variable inclusion with optional prefix
- [x] `BuiltinVariables` for standard template variables:
  - `IMAGE`, `IMAGE_TAG`, `NAMESPACE`, `ENVIRONMENT`, `DEPLOYMENT_NAME`
- [x] `ManifestRenderer` using Tera template engine:
  - Single file or directory pattern support
  - `render_all()` for batch rendering
  - Variable substitution from config, environment, and builtins
- [x] `RenderedManifest` with multi-document YAML parsing

**Files**: `src/infrastructure/manifest.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-template:
    type: kubernetes
    config:
      namespace: production
      deployment_method: template
      manifest_template:
        path: ./k8s/templates
        variables:
          REPLICAS: "3"
          LOG_LEVEL: info
        include_env: true
        env_prefix: APP_
```

### 19.3 Raw Manifest Application
- [x] `RawManifestConfig` for raw manifest configuration:
  - File list (files and/or directories)
  - Recursive flag for directory traversal
  - Prune support with label selector
- [x] `RawManifestApplier` for kubectl operations:
  - `apply()` for file-based manifests
  - `apply_content()` for rendered content (stdin pipe)
  - `diff()` for dry-run comparison
  - Namespace and context support

**Files**: `src/infrastructure/k8s_resources.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-raw:
    type: kubernetes
    config:
      namespace: production
      deployment_method: raw_manifest
      raw_manifests:
        files:
          - ./k8s/deployment.yaml
          - ./k8s/service.yaml
        recursive: true
        prune: true
        prune_selector: app=myapp
```

### 19.4 ConfigMap and Secret Management
- [x] `K8sConfigMapSpec` for ConfigMap creation:
  - Name, namespace, data, binary_data
  - File loading support (`data_from_files`)
  - Labels and annotations
- [x] `K8sSecretSpec` for Secret creation:
  - Name, namespace, secret type
  - Data with `EnvVarConfig` for flexible sources (static, env, AWS SM, Vault)
  - String data (auto base64 encoded)
  - Labels and annotations
- [x] `K8sResourceManager` for resource operations:
  - `apply_config_map()` - Create/update ConfigMaps
  - `apply_secret()` - Create/update Secrets with resolved values
  - Delete operations for cleanup
- [x] Pre-deployment application (ConfigMaps/Secrets applied BEFORE deployment)
- [x] Secret resolution via `EnvVarResolver`

**Files**: `src/infrastructure/k8s_resources.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-prod:
    type: kubernetes
    config:
      namespace: production
      config_maps:
        - name: app-config
          data:
            LOG_LEVEL: info
            API_URL: https://api.example.com
          labels:
            app: my-app

      secrets:
        - name: db-credentials
          secret_type: Opaque
          data:
            password:
              source: aws_secrets_manager
              arn: arn:aws:secretsmanager:us-east-1:123456789:secret:prod/db
              json_field: password
          string_data:
            username: admin
```

### 19.5 Horizontal Pod Autoscaler (HPA)
- [x] `HpaConfig` for HPA configuration:
  - Target deployment name
  - Min/max replicas
  - Target CPU/memory utilization percentages
  - Custom metrics support
  - Scale down/up stabilization windows
- [x] `HpaCustomMetric` for custom scaling metrics
- [x] HPA applied AFTER deployment completes
- [x] kube-rs HPA resource creation/update

**Files**: `src/infrastructure/k8s_resources.rs`

**Configuration Example**:
```yaml
infrastructure:
  k8s-prod:
    type: kubernetes
    config:
      namespace: production
      hpa:
        target_deployment: my-app
        min_replicas: 2
        max_replicas: 20
        target_cpu_utilization: 70
        target_memory_utilization: 80
        scale_down_stabilization_secs: 300
```

### 19.6 Deployment Method Enum
- [x] Extended `DeploymentMethod` enum:
  - `Direct` - Direct image patch (default)
  - `Helm` - Helm chart deployment
  - `Kustomize` - Kustomize overlay deployment
  - `Template` - Tera template rendering
  - `RawManifest` - Raw YAML file application
- [x] Automatic detection from configuration
- [x] Explicit override via `deployment_method` config key

### 19.7 Integration Flow
1. Parse extended config (manifest_template, raw_manifests, config_maps, secrets, hpa)
2. Apply ConfigMaps and Secrets (pre-deployment)
3. Deploy using selected method (Direct, Helm, Kustomize, Template, RawManifest)
4. Apply HPA (post-deployment)

### 19.8 Tests
- [x] ManifestRenderer tests in `src/infrastructure/manifest.rs`
- [x] K8sResourceManager tests in `src/infrastructure/k8s_resources.rs`
- [x] KubernetesProvider extended config tests
- [x] DeploymentMethod parsing tests

**Test Count**: 90 tests passing (with kubernetes feature)

---

## Milestone 20: AWS Lambda Extended Features [COMPLETED]

### 20.1 Overview
Extended Lambda deployment options including ZIP packaging, layer management, and event source mappings for triggers.

### 20.2 ZIP Packaging
- [x] `ZipPackageConfig` for package configuration:
  - Source path (file or directory)
  - S3 bucket and prefix for large packages (>50MB)
  - Exclude patterns for files/directories
  - Hidden file handling
- [x] `LambdaPackager` for ZIP creation:
  - `create_zip()` - Create ZIP from source
  - `upload_to_s3()` - Upload to S3 for large packages
  - Automatic file/directory handling
  - Pattern-based exclusions
- [x] Direct ZIP deployment (up to 50MB)
- [x] S3-based ZIP deployment (unlimited size)

**Files**: `src/infrastructure/lambda_extended.rs`

**Configuration Example**:
```yaml
infrastructure:
  lambda-zip:
    type: aws-lambda
    config:
      function_name: my-function
      region: us-east-1
      zip_package:
        source_path: ./dist
        s3_bucket: my-lambda-artifacts
        s3_prefix: deployments
        exclude:
          - "*.pyc"
          - "__pycache__"
          - ".git"
        include_hidden: false
```

### 20.3 Layer Management
- [x] `LayerConfig` for layer configuration:
  - Layer name and description
  - Source path or S3 location
  - Compatible runtimes (python3.9, nodejs18.x, etc.)
  - Compatible architectures (x86_64, arm64)
  - License information
- [x] `LayerManager` for layer operations:
  - `publish_layer()` - Publish new layer version
  - `list_layer_versions()` - List all versions
  - `cleanup_old_versions()` - Delete old versions (keep N most recent)
  - `get_latest_version_arn()` - Get latest version ARN
- [x] Layer publishing and function attachment
- [x] Layer version cleanup

**Configuration Example**:
```yaml
infrastructure:
  lambda-with-layers:
    type: aws-lambda
    config:
      function_name: my-function
      region: us-east-1
      layers:
        - name: common-deps
          description: Shared Python dependencies
          source_path: ./layers/common
          compatible_runtimes:
            - python3.9
            - python3.10
          compatible_architectures:
            - x86_64
        - name: utils-layer
          s3_bucket: my-layers
          s3_key: utils/v1.0.0.zip
```

### 20.4 Event Source Mappings
- [x] `EventSourceType` enum supporting:
  - SQS queues
  - SNS topics (via SQS subscription)
  - DynamoDB Streams
  - Kinesis Streams
  - Apache Kafka (self-managed)
  - Amazon MSK (Managed Kafka)
  - ActiveMQ
  - RabbitMQ
- [x] `EventSourceConfig` for mapping configuration:
  - Source type and ARN
  - Batch size and batching window
  - Starting position (for streams)
  - Filter patterns (event filtering)
  - Retry configuration
  - Parallelization factor
  - On-failure destinations
  - Source access (for Kafka/MQ)
- [x] `EventSourceManager` for mapping operations:
  - `configure_event_source()` - Create or update mapping
  - `list_event_sources()` - List all mappings
  - `delete_event_source()` - Delete mapping
  - `delete_all_event_sources()` - Cleanup

**Files**: `src/infrastructure/lambda_extended.rs`

**Configuration Example**:
```yaml
infrastructure:
  lambda-triggered:
    type: aws-lambda
    config:
      function_name: my-processor
      region: us-east-1
      event_sources:
        - source_type: sqs
          source_arn: arn:aws:sqs:us-east-1:123456789:orders-queue
          batch_size: 10
          maximum_batching_window_secs: 30
          filter_patterns:
            - '{"body": {"type": ["order"]}}'

        - source_type: kinesis_stream
          source_arn: arn:aws:kinesis:us-east-1:123456789:stream/events
          batch_size: 100
          starting_position: LATEST
          parallelization_factor: 2
          maximum_record_age_secs: 3600
          maximum_retry_attempts: 3
          on_failure_destination_arn: arn:aws:sqs:us-east-1:123456789:dlq
```

### 20.5 Integration with Deploy Flow
- [x] ZIP or container image deployment (auto-detected from config)
- [x] Layer publishing in full deployment mode
- [x] Event source configuration in full deployment mode
- [x] App-only mode skips infrastructure changes (layers, event sources)

### 20.6 Tests
- [x] ZipPackageConfig parsing tests
- [x] LayerConfig parsing tests
- [x] EventSourceConfig parsing tests
- [x] EventSourceType tests

**Test Count**: 77 tests passing

---

## Milestone 21: Plugin Template Generator [COMPLETED]

### 21.1 Overview
CLI command to scaffold new plugins with all necessary files and boilerplate code, making it easy to create custom infrastructure plugins.

### 21.2 CLI Commands
- [x] `pmp-deploy plugin new <name>` - Create plugin in current directory
- [x] `pmp-deploy plugin new <name> --dir <path>` - Create plugin in specified directory
- [x] `pmp-deploy plugin new <name> --infrastructure-type <type>` - Set infrastructure type (default: custom)
- [x] `pmp-deploy plugin new <name> --description <desc>` - Set plugin description
- [x] `pmp-deploy plugin list` - List installed plugins

**Files**: `src/cli/commands.rs`, `src/cli/plugin_template/mod.rs`, `src/cli/plugin_template/generator.rs`

### 21.3 Generated Files
- [x] `Cargo.toml` - Project configuration with SDK dependency, cdylib crate type
- [x] `src/lib.rs` - Plugin struct with `InfrastructurePlugin` trait implementation stubs
- [x] `README.md` - Build and installation instructions

### 21.4 Template Features
- [x] `PluginGenerator` struct for scaffolding:
  - PascalCase struct name generation from kebab-case plugin name
  - Crate naming convention: `pmp-deploy-{name}-plugin`
  - Platform-specific library names (.dll, .so, .dylib)
- [x] Generated `lib.rs` includes:
  - All trait method implementations with TODO stubs
  - `declare_plugin!` macro invocation
  - Unit tests for validation and dry-run
- [x] Generated `README.md` includes:
  - Build instructions
  - Installation paths for Windows/Linux/macOS
  - Configuration example for pmp-deploy.yaml

### 21.5 Usage Example
```bash
# Create a new plugin for custom cloud provider
pmp-deploy plugin new my-cloud --infrastructure-type my-cloud --description "My Cloud infrastructure plugin"

# Output:
# Created plugin 'my-cloud' successfully!
#
# Next steps:
#   1. cd my-cloud
#   2. cargo build --release
#   3. Copy target/release/pmp-deploy-my_cloud-plugin.dll (Windows) or libpmp-deploy-my_cloud-plugin.so (Linux) to ~/.pmp-deploy/plugins/
#
# Then use infrastructure type 'my-cloud' in your pmp-deploy.yaml.

# List installed plugins
pmp-deploy plugin list
```

### 21.6 Tests
- [x] `test_to_pascal_case()` - PascalCase conversion
- [x] `test_crate_name()` - Crate naming convention

---

## Future Milestones

### Built-in Cloud Plugins
- [ ] Google Cloud Run
- [ ] Azure Container Apps
- [ ] DigitalOcean App Platform
- [ ] Fly.io

### Advanced Features
- [ ] Metrics-based canary promotion/rollback
- [ ] Manual approval gates for deployments
- [ ] Prometheus metrics export
- [ ] UI authentication
