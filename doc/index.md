# pmp-deploy Documentation

## Table of Contents

1. [Getting Started](getting-started.md)
2. [Configuration Guide](configuration.md)
3. [Deployment Strategies](deployment-strategies.md)
4. [Infrastructure Providers](infrastructure-providers.md)
5. [Plugin Development](plugin-development.md)
6. [Web UI](web-ui.md)

## Overview

pmp-deploy is a Rust CLI tool for simplified multi-infrastructure application deployments. It supports:

- **Multiple Infrastructure Types**: AWS EKS, AWS ECS, AWS Lambda, Kubernetes, Docker Compose
- **Deployment Strategies**: Rolling Update, Blue/Green, Canary, Recreate, Direct
- **Configuration-Driven**: Define infrastructure and environments in YAML
- **Multi-Project Management**: Manage multiple projects from a global configuration
- **Web UI**: Browser-based interface for deployments
- **Plugin System**: Extend with custom infrastructure providers

## Quick Start

```bash
# Initialize a new project
pmp-deploy init

# List environments
pmp-deploy list

# Deploy to an environment
pmp-deploy deploy staging

# Check status
pmp-deploy status staging

# Rollback if needed
pmp-deploy rollback staging

# Start Web UI
pmp-deploy ui --port 8080
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI / Web UI                         │
├─────────────────────────────────────────────────────────────┤
│                    Deployment Executor                       │
├──────────────┬──────────────┬──────────────┬────────────────┤
│   Rolling    │  Blue/Green  │    Canary    │   Recreate     │
│   Update     │  Strategy    │   Strategy   │   Strategy     │
├──────────────┴──────────────┴──────────────┴────────────────┤
│                Infrastructure Provider Layer                 │
├──────────┬──────────┬──────────┬──────────┬─────────────────┤
│ AWS EKS  │ AWS ECS  │ Lambda   │   K8s    │ Docker Compose  │
└──────────┴──────────┴──────────┴──────────┴─────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │  Plugin System  │
                    │ (Custom Infra)  │
                    └─────────────────┘
```
