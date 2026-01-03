# Getting Started

## Installation

### From Cargo

```bash
cargo install pmp-deploy
```

### From Source

```bash
git clone https://github.com/yourusername/pmp-deploy
cd pmp-deploy
cargo build --release
cp target/release/pmp-deploy ~/.local/bin/
```

## Initialize a Project

Create a new `.pmp-deploy.yaml` configuration file:

```bash
pmp-deploy init
```

This creates a template configuration with common settings.

## Basic Configuration

Edit `.pmp-deploy.yaml`:

```yaml
infrastructure:
  local:
    type: docker-compose
    config:
      compose_file: docker-compose.yml

environments:
  development:
    infrastructure: local
    deployment_type: all-in
    image: myapp:latest
```

## Your First Deployment

```bash
# Validate configuration
pmp-deploy validate

# List available environments
pmp-deploy list

# Deploy
pmp-deploy deploy development

# Check status
pmp-deploy status development
```

## Multi-Project Setup

Create a global config at `~/.pmp-deploy.yaml`:

```yaml
projects:
  - path: ~/projects/frontend
    name: frontend
  - path: ~/projects/api
    name: api-service
```

Then manage projects:

```bash
# List all projects
pmp-deploy projects list

# Add a project
pmp-deploy projects add ~/projects/new-service --name new-service

# Remove a project
pmp-deploy projects remove new-service
```

## Common Workflows

### Development

```bash
# Quick rebuild and deploy
pmp-deploy deploy development

# View logs
pmp-deploy logs development --follow
```

### Staging

```bash
# Deploy with verbose output
pmp-deploy deploy staging -v

# Check deployment status
pmp-deploy status staging
```

### Production

```bash
# Dry run first
pmp-deploy deploy production --dry-run

# Actual deployment
pmp-deploy deploy production

# Rollback if issues
pmp-deploy rollback production
```

## Web UI

Start the web interface for a visual deployment experience:

```bash
pmp-deploy ui --port 8080
```

Open http://localhost:8080 in your browser.
