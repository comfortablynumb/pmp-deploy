# Deployment Strategies

pmp-deploy supports multiple deployment strategies to match your needs.

## Rolling Update

Gradually replaces instances in batches with zero downtime.

```yaml
environments:
  staging:
    infrastructure: k8s-cluster
    deployment_type: rolling-update
    config:
      batch_size: 25  # 25% per batch
      health_check_timeout_secs: 120
      rollback_on_failure: true
```

**Best for:**
- Most applications
- When downtime is unacceptable
- Moderate traffic applications

**Process:**
1. Deploy new version to X% of instances
2. Wait for health checks to pass
3. Repeat until 100% updated
4. Automatic rollback if health checks fail

## All-In

Stops all instances, then deploys the new version.

```yaml
environments:
  development:
    infrastructure: local
    deployment_type: all-in
```

**Best for:**
- Development environments
- Applications that cannot run multiple versions
- Database migrations that require downtime
- Lambda functions (atomic updates)

**Process:**
1. Stop all existing instances
2. Deploy new version
3. Start new instances

**Warning:** This causes downtime. Only use in development or when downtime is acceptable.

**Note:** For Kubernetes, this uses the native Recreate strategy which terminates all existing pods before creating new ones.

## Strategy Comparison

| Strategy | Downtime | Rollback Speed | Complexity | Use Case |
|----------|----------|----------------|------------|----------|
| Rolling Update | None | Medium | Low | General purpose |
| All-In | Yes | Medium | Low | Dev environments, Lambda |

## Configuration Options

### Common Options

```yaml
config:
  # Health check timeout in seconds
  health_check_timeout_secs: 300

  # Auto-rollback on failure
  rollback_on_failure: true
```

### Rolling Update Options

```yaml
config:
  # Percentage of instances per batch
  batch_size: 25
```
