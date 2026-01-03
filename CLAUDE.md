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
│   ├── config/                    # Config loading, schema, validation, env_vars
│   ├── secrets/                   # Secrets providers (env, AWS SM, Vault)
│   ├── infrastructure/            # Infrastructure providers (EKS, ECS, Lambda, K8s, Docker, Helm, Kustomize)
│   │   └── provisioning/          # Full infrastructure provisioning (Lambda, ECS)
│   ├── deployment/                # Deployment strategies (rolling-update, all-in)
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
│   ├── storage/                   # Storage abstraction for deployment history
│   │   ├── traits.rs              # Storage trait
│   │   ├── record.rs              # DeploymentRecord, DeploymentStatus
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
- `Storage`: Trait for deployment history persistence
- `DeploymentRecord`, `DeploymentStatus`: Storage types
- `StorageBackend`: Enum (Memory, File, Sqlite)
- `StorageConfig`, `StorageFactory`: Storage configuration and creation
- `FileStorage`, `SqliteStorage`, `InMemoryStorage`: Storage implementations
- `SecretsProvider`, `SecretsResolver`: Secrets management

## Commands
- `deploy <env>` (with `--skip-hooks`, `--skip-pre-hooks`, `--skip-post-hooks`)
- `status <env>`, `rollback <env>`, `logs <env>`
- `provision <env>`: Create/update infrastructure
- `hooks list|run <env> [hook-name]`: Manage and run hooks
- `list`, `validate`, `init`
- `projects list|add|remove`
- `ui`: Web UI with metrics visualization

## Plugin System
Plugins are native shared libraries loaded from `~/.pmp-deploy/plugins/`.
Use the SDK crate with `declare_plugin!` macro to create plugins.

## Metrics System
Providers: CloudWatch (AWS infra), Prometheus (K8s/Docker)
Standard metrics: CPU, Memory, Error Rate, Latency, Request Count
Custom metrics via PromQL or CloudWatch JSON queries
SSE streaming for real-time updates
