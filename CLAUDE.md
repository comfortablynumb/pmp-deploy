# pmp-deploy - Project Context

## Overview
Rust CLI tool for multi-infrastructure deployments (K8s, AWS ECS, Lambda, Docker Compose).

## Project Structure
```
pmp-deploy/
├── Cargo.toml                     # Workspace root
├── crates/
│   └── pmp-deploy-plugin-sdk/     # Plugin SDK crate
├── examples/
│   ├── plugins/echo-plugin/       # Example plugin implementation
│   └── configs/                   # Example configurations (ECS, EKS, Lambda, K8s, Docker, Metrics)
├── src/
│   ├── main.rs                    # CLI entry point
│   ├── lib.rs                     # Library exports
│   ├── cli/                       # Clap CLI definitions
│   │   ├── commands.rs            # Command structs (InitArgs, DeployArgs, ResumeArgs, PluginNewArgs, etc.)
│   │   ├── init/                  # Init command module
│   │   │   ├── handler.rs         # Command handler
│   │   │   └── wizard/            # Interactive wizard (types, prompter, flow, generator)
│   │   ├── resume/                # Resume command module
│   │   │   └── mod.rs             # Resume interrupted deployments
│   │   └── plugin_template/       # Plugin template generator
│   │       ├── mod.rs             # Plugin command handlers
│   │       └── generator.rs       # PluginGenerator for scaffolding
│   ├── config/                    # Config loading, schema, validation, env_vars
│   ├── secrets/                   # Secrets providers (env, AWS SM, Vault)
│   ├── infrastructure/            # Infrastructure providers (EKS, ECS, Lambda, K8s, Docker, Helm, Kustomize)
│   │   ├── provisioning/          # Full infrastructure provisioning (Lambda, ECS)
│   │   ├── manifest.rs            # ManifestRenderer for template-based deployments (Tera)
│   │   ├── k8s_resources.rs       # K8s ConfigMaps, Secrets, HPA, raw manifests
│   │   └── lambda_extended.rs     # Lambda ZIP packaging, layers, event sources
│   ├── deployment/                # Deployment strategies and checkpoint management
│   │   ├── checkpoint.rs          # CheckpointManager for resumable deployments
│   │   ├── executor.rs            # DeploymentExecutor with checkpoint integration
│   │   ├── strategy.rs            # DeploymentStrategy trait, DeploymentType
│   │   └── rolling.rs             # RollingUpdate strategy
│   ├── metrics/                   # Metrics observation (CloudWatch, Prometheus)
│   │   ├── provider.rs            # MetricsProvider trait, StandardMetric enum
│   │   ├── resolver.rs            # MetricsResolver registry
│   │   ├── cloudwatch.rs          # AWS CloudWatch provider
│   │   ├── prometheus.rs          # Prometheus provider
│   │   └── stream.rs              # SSE streaming helpers
│   ├── hooks/                     # Pre/post deployment hooks
│   │   ├── types.rs               # HookConfig, HooksConfig types
│   │   ├── executor.rs            # HookExecutor trait, HookRunner
│   │   ├── container.rs           # Docker container hooks
│   │   ├── http.rs                # HTTP webhook hooks
│   │   ├── ecs.rs                 # AWS ECS task hooks
│   │   ├── k8s.rs                 # Kubernetes Job hooks
│   │   └── lambda.rs              # AWS Lambda invocation hooks
│   ├── storage/                   # Storage abstraction for deployment history & checkpoints
│   │   ├── traits.rs              # Storage trait (incl. checkpoint methods)
│   │   ├── record.rs              # DeploymentRecord, DeploymentCheckpoint, DeploymentPhase
│   │   ├── memory.rs              # In-memory storage
│   │   ├── file.rs                # File-based JSON storage
│   │   ├── sqlite.rs              # SQLite storage
│   │   └── factory.rs             # StorageFactory, StorageConfig
│   ├── error.rs                   # Comprehensive error types
│   ├── retry.rs                   # Retry with exponential backoff
│   ├── logging.rs                 # Structured logging, credential masking
│   ├── signal.rs                  # Graceful shutdown handling
│   ├── security.rs                # File permissions, TLS config
│   ├── plugins/                   # Plugin system (FFI, loader, registry)
│   └── ui/
│       ├── api.rs                 # REST API handlers (incl. metrics endpoints)
│       ├── server.rs              # Axum HTTP/HTTPS server
│       └── state.rs               # AppState for API
├── static/
│   └── index.html                 # Web UI (jQuery + Tailwind + Chart.js)
├── bin/                           # Shell scripts (up.sh, down.sh)
└── doc/
    └── ROADMAP.md
```

## Key Types
- `Config`, `GlobalConfig`, `MetricsConfig`: Configuration types
- `InfrastructureProvider`: Trait for infrastructure backends
- `InfrastructureType`: Enum (AwsEks, AwsEcs, AwsLambda, Kubernetes, DockerCompose, Custom)
- `DeploymentContext`: Context passed to provider methods
- `DeployMode`: Enum (Full, AppOnly) - controls what gets deployed
- `DeploymentType`: Enum (RollingUpdate, AllIn) - deployment strategies
- `MetricsProvider`: Trait for metrics backends (CloudWatch, Prometheus)
- `StandardMetric`: Enum (CpuUtilization, MemoryUtilization, ErrorRate, RequestLatency, RequestCount)
- `MetricsResolver`: Registry for metrics providers
- `HookExecutor`: Trait for hook types (Container, HTTP, ECS, K8s, Lambda)
- `HookRunner`: Orchestrates hook execution with timeout handling
- `HooksConfig`: Pre/post deployment hook configuration
- `Storage`: Trait for deployment history and checkpoint persistence
- `DeploymentRecord`, `DeploymentStatus`: Deployment history types
- `DeploymentCheckpoint`, `DeploymentPhase`, `DeployedResource`: Checkpoint types for resumable deployments
- `CheckpointManager`, `CheckpointConfig`: Checkpoint lifecycle management
- `StorageBackend`: Enum (Memory, File, Sqlite)
- `StorageConfig`, `StorageFactory`: Storage configuration and creation
- `FileStorage`, `SqliteStorage`, `InMemoryStorage`: Storage implementations
- `SecretsProvider`, `SecretsResolver`: Secrets management
- `ManifestRenderer`, `ManifestTemplateConfig`, `BuiltinVariables`: Template-based K8s deployments
- `DeploymentMethod`: Enum (Direct, Helm, Kustomize, Template, RawManifest) - K8s deployment methods
- `K8sConfigMapSpec`, `K8sSecretSpec`, `HpaConfig`: K8s resource specifications
- `K8sResourceManager`, `RawManifestApplier`: K8s resource management
- `ZipPackageConfig`, `LambdaPackager`: Lambda ZIP deployment
- `LayerConfig`, `LayerManager`: Lambda layer management
- `EventSourceConfig`, `EventSourceManager`, `EventSourceType`: Lambda event source mappings
- `PluginGenerator`: Plugin template generator for scaffolding new plugins

## Commands
- `deploy <env>` (with `--skip-hooks`, `--skip-pre-hooks`, `--skip-post-hooks`)
- `status <env>`, `rollback <env>`, `logs <env>`
- `provision <env>`: Create/update infrastructure
- `hooks list|run <env> [hook-name]`: Manage and run hooks
- `resume [deployment-id]`: Resume interrupted deployment
  - `--list`: List resumable deployments
  - `--clear`: Clear checkpoint without resuming
- `list`, `validate`
- `init`: Interactive wizard to create config (select envs, infra, deploy mode)
  - `--infrastructure <type>`: Skip wizard, use template
  - `--non-interactive`: Skip wizard, use defaults
  - `--force`: Overwrite existing config
- `projects list|add|remove`
- `plugin new|list`: Create and manage plugins
  - `new <name>`: Create plugin from template
  - `--dir <path>`: Output directory
  - `--infrastructure-type <type>`: Infrastructure type (default: custom)
  - `list`: List installed plugins
- `ui`: Web UI with metrics visualization

## Plugin System
Plugins are native shared libraries loaded from `~/.pmp-deploy/plugins/`.
Use the SDK crate with `declare_plugin!` macro to create plugins.

## Metrics System
Providers: CloudWatch (AWS infra), Prometheus (K8s/Docker)
Standard metrics: CPU, Memory, Error Rate, Latency, Request Count
Custom metrics via PromQL or CloudWatch JSON queries
SSE streaming for real-time updates
