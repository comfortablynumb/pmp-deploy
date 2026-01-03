use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, path::PathBuf, time::Duration};

use crate::config::{ConfigLoader, EnvironmentConfig};
use crate::infrastructure::{DeployMode, DeploymentContext, ProviderFactory};
use crate::metrics::{MetricQueryParams, StandardMetric, TimeRange};
use crate::storage::DeploymentRecord;

use super::state::{AppState, ProjectInfo};

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_handler))
        .route("/projects", get(list_projects))
        .route("/projects/{project_id}", get(get_project))
        .route("/projects/{project_id}/environments", get(list_environments))
        .route(
            "/projects/{project_id}/environments/{env_name}",
            get(get_environment),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/deploy",
            post(deploy_environment),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/status",
            get(get_status),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/rollback",
            post(rollback_environment),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/logs",
            get(stream_logs),
        )
        // Metrics endpoints
        .route(
            "/projects/{project_id}/environments/{env_name}/metrics",
            get(list_metrics),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/metrics/stream",
            get(stream_metrics),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/metrics/{metric_name}",
            get(get_metric),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/metrics/{metric_name}/current",
            get(get_metric_current),
        )
        // Deployment history endpoints
        .route(
            "/projects/{project_id}/deployments",
            get(list_project_deployments),
        )
        .route(
            "/projects/{project_id}/deployments/{deployment_id}",
            get(get_deployment),
        )
        .route(
            "/projects/{project_id}/environments/{env_name}/deployments",
            get(list_environment_deployments),
        )
        .route("/deployments", get(list_all_deployments))
        .route("/deployments/cleanup", post(cleanup_deployments))
}

async fn health_handler() -> &'static str {
    "OK"
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<ProjectsResponse>, ApiError> {
    let projects = state.get_projects().await?;

    Ok(Json(ProjectsResponse { projects }))
}

async fn get_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    Ok(Json(ProjectResponse { project }))
}

async fn list_environments(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> Result<Json<EnvironmentsResponse>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let environments: Vec<EnvironmentInfo> = config
        .environments
        .iter()
        .map(|(name, env)| {
            let infra = config.get_infrastructure(&env.infrastructure);
            let infra_type = infra
                .map(|i| i.infrastructure_type.clone())
                .unwrap_or_else(|| "unknown".to_string());

            EnvironmentInfo {
                name: name.clone(),
                infrastructure: env.infrastructure.clone(),
                infrastructure_type: infra_type,
                deployment_type: env.deployment_type.clone(),
                image: env.image.clone(),
                replicas: env.replicas,
            }
        })
        .collect();

    Ok(Json(EnvironmentsResponse { environments }))
}

async fn get_environment(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
) -> Result<Json<EnvironmentDetailResponse>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let env = config.get_environment(&env_name);

    let Some(env) = env else {
        return Err(ApiError::NotFound("Environment not found".to_string()));
    };

    let infra = config.get_infrastructure(&env.infrastructure);
    let infra_type = infra
        .map(|i| i.infrastructure_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let detail = EnvironmentDetail {
        name: env_name,
        infrastructure: env.infrastructure.clone(),
        infrastructure_type: infra_type,
        deployment_type: env.deployment_type.clone(),
        image: env.image.clone(),
        replicas: env.replicas,
        resources: env.resources.as_ref().map(|r| ResourceInfo {
            cpu: r.cpu.clone(),
            memory: r.memory.clone(),
            cpu_limit: r.cpu_limit.clone(),
            memory_limit: r.memory_limit.clone(),
        }),
        env_vars: env.env.clone(),
    };

    Ok(Json(EnvironmentDetailResponse { environment: detail }))
}

async fn deploy_environment(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
    Json(payload): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let config_path = PathBuf::from(&project.path).join(".pmp-deploy.yaml");
    let loader = ConfigLoader::new().with_project_config(config_path);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&env_name)
        .ok_or_else(|| ApiError::NotFound("Environment not found".to_string()))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| ApiError::NotFound("Infrastructure not found".to_string()))?;

    let mut env_config = env.clone();

    if let Some(image) = &payload.image {
        env_config.image = Some(image.clone());
    }

    let dry_run = payload.dry_run.unwrap_or(false);
    let deploy_mode = payload
        .deploy_mode
        .as_ref()
        .map(|s| DeployMode::from_str(s))
        .unwrap_or_default();

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&env_name, &env_config, dry_run, deploy_mode);

    let result = provider.deploy(&deploy_ctx).await?;

    Ok(Json(DeployResponse {
        success: result.success,
        message: result.message,
        version: result.version,
        dry_run,
    }))
}

async fn get_status(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
) -> Result<Json<StatusResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let config_path = PathBuf::from(&project.path).join(".pmp-deploy.yaml");
    let loader = ConfigLoader::new().with_project_config(config_path);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&env_name)
        .ok_or_else(|| ApiError::NotFound("Environment not found".to_string()))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| ApiError::NotFound("Infrastructure not found".to_string()))?;

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&env_name, env, false, DeployMode::Full);

    let status = provider.status(&deploy_ctx).await?;

    Ok(Json(StatusResponse {
        environment: env_name,
        status,
    }))
}

async fn rollback_environment(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
    Json(_payload): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let config_path = PathBuf::from(&project.path).join(".pmp-deploy.yaml");
    let loader = ConfigLoader::new().with_project_config(config_path);
    let config = loader.load_project_config()?;

    let env = config
        .get_environment(&env_name)
        .ok_or_else(|| ApiError::NotFound("Environment not found".to_string()))?;

    let infra = config
        .get_infrastructure(&env.infrastructure)
        .ok_or_else(|| ApiError::NotFound("Infrastructure not found".to_string()))?;

    let provider = ProviderFactory::create(&env.infrastructure, infra).await?;
    let deploy_ctx = create_deployment_context(&env_name, env, false, DeployMode::Full);

    let result = provider.rollback(&deploy_ctx).await?;

    Ok(Json(RollbackResponse {
        success: result.success,
        message: result.message,
        version: result.version,
    }))
}

async fn stream_logs(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
    Query(_params): Query<LogsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let stream = stream::unfold(
        LogStreamState {
            project_path: project.path.clone(),
            env_name: env_name.clone(),
            counter: 0,
        },
        |mut state| async move {
            tokio::time::sleep(Duration::from_secs(1)).await;

            state.counter += 1;

            let event = Event::default()
                .data(format!(
                    "[{}] Log line {} for environment {}",
                    chrono_like_timestamp(),
                    state.counter,
                    state.env_name
                ))
                .event("log");

            if state.counter > 100 {
                return None;
            }

            Some((Ok(event), state))
        },
    );

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

struct LogStreamState {
    #[allow(dead_code)]
    project_path: String,
    env_name: String,
    counter: u32,
}

// ============================================================================
// Metrics Endpoints
// ============================================================================

async fn list_metrics(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
) -> Result<Json<MetricsListResponse>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let env = config.get_environment(&env_name);

    let Some(env) = env else {
        return Err(ApiError::NotFound("Environment not found".to_string()));
    };

    let infra = config.get_infrastructure(&env.infrastructure);
    let infra_type = infra
        .map(|i| i.infrastructure_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Get standard metrics available for this infrastructure type
    let standard_metrics: Vec<MetricInfo> = StandardMetric::all()
        .into_iter()
        .map(|m| MetricInfo {
            name: format!("{:?}", m).to_lowercase().replace("utilization", "_utilization"),
            display_name: m.display_name().to_string(),
            unit: m.default_unit().to_string(),
            metric_type: "standard".to_string(),
        })
        .collect();

    // Get custom metrics from config
    let custom_metrics: Vec<MetricInfo> = config
        .metrics
        .as_ref()
        .map(|m| {
            m.custom_metrics
                .iter()
                .map(|cm| MetricInfo {
                    name: cm.name.clone(),
                    display_name: cm.display_name.clone(),
                    unit: cm.unit.clone(),
                    metric_type: "custom".to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let mut metrics = standard_metrics;
    metrics.extend(custom_metrics);

    Ok(Json(MetricsListResponse {
        environment: env_name,
        infrastructure_type: infra_type,
        metrics,
    }))
}

async fn get_metric(
    State(state): State<AppState>,
    Path((project_id, env_name, metric_name)): Path<(String, String, String)>,
    Query(params): Query<MetricQueryOptions>,
) -> Result<Json<MetricDataResponse>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let env = config.get_environment(&env_name);

    let Some(env) = env else {
        return Err(ApiError::NotFound("Environment not found".to_string()));
    };

    let infra = config.get_infrastructure(&env.infrastructure);
    let infra_type = infra
        .map(|i| i.infrastructure_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Parse the standard metric
    let standard_metric = parse_standard_metric(&metric_name);

    let Some(metric) = standard_metric else {
        return Err(ApiError::NotFound(format!(
            "Metric '{}' not found",
            metric_name
        )));
    };

    let time_range = match params.range.as_deref() {
        Some("1h") => TimeRange::last_hours(1),
        Some("6h") => TimeRange::last_hours(6),
        Some("24h") => TimeRange::last_hours(24),
        _ => TimeRange::last_minutes(30),
    };

    let period = params.period.unwrap_or(60);

    let query_params = MetricQueryParams::new(&env_name, &infra_type, metric)
        .with_time_range(time_range)
        .with_period(period);

    let resolver = state.get_metrics_resolver().await;
    let result = resolver.query_metric(&query_params).await;

    match result {
        Ok(data) => Ok(Json(MetricDataResponse {
            metric_name: data.metric_name,
            unit: data.unit,
            data_points: data
                .data_points
                .into_iter()
                .map(|dp| DataPointResponse {
                    timestamp: dp.timestamp,
                    value: dp.value,
                })
                .collect(),
        })),
        Err(e) => Err(ApiError::Internal(format!("Failed to query metric: {}", e))),
    }
}

async fn get_metric_current(
    State(state): State<AppState>,
    Path((project_id, env_name, metric_name)): Path<(String, String, String)>,
) -> Result<Json<MetricGaugeResponse>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let env = config.get_environment(&env_name);

    let Some(env) = env else {
        return Err(ApiError::NotFound("Environment not found".to_string()));
    };

    let infra = config.get_infrastructure(&env.infrastructure);
    let infra_type = infra
        .map(|i| i.infrastructure_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let standard_metric = parse_standard_metric(&metric_name);

    let Some(metric) = standard_metric else {
        return Err(ApiError::NotFound(format!(
            "Metric '{}' not found",
            metric_name
        )));
    };

    let query_params = MetricQueryParams::new(&env_name, &infra_type, metric);

    let resolver = state.get_metrics_resolver().await;
    let result = resolver.get_current_value(&query_params).await;

    match result {
        Ok(gauge) => Ok(Json(MetricGaugeResponse {
            metric_name: gauge.metric_name,
            display_name: gauge.display_name,
            current_value: gauge.current_value,
            unit: gauge.unit,
            status: format!("{:?}", gauge.status).to_lowercase(),
            thresholds: gauge.thresholds.map(|t| ThresholdResponse {
                warning: t.warning,
                critical: t.critical,
            }),
        })),
        Err(e) => Err(ApiError::Internal(format!(
            "Failed to get current value: {}",
            e
        ))),
    }
}

async fn stream_metrics(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
    Query(_params): Query<MetricStreamOptions>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let config = state.get_project_config(&project_id).await?;

    let Some(config) = config else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let env = config.get_environment(&env_name);

    let Some(env) = env else {
        return Err(ApiError::NotFound("Environment not found".to_string()));
    };

    let infra = config.get_infrastructure(&env.infrastructure);
    let infra_type = infra
        .map(|i| i.infrastructure_type.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let stream = stream::unfold(
        MetricStreamState {
            env_name: env_name.clone(),
            infra_type,
            counter: 0,
        },
        |mut state| async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            state.counter += 1;

            // Generate mock gauge data for demo
            let gauges: Vec<serde_json::Value> = StandardMetric::all()
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "metric_name": format!("{:?}", m).to_lowercase(),
                        "display_name": m.display_name(),
                        "current_value": rand_value(m),
                        "unit": m.default_unit(),
                        "status": "normal"
                    })
                })
                .collect();

            let event = Event::default()
                .data(serde_json::to_string(&serde_json::json!({
                    "type": "gauges",
                    "data": gauges
                })).unwrap_or_default())
                .event("metrics");

            if state.counter > 1000 {
                return None;
            }

            Some((Ok(event), state))
        },
    );

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

struct MetricStreamState {
    #[allow(dead_code)]
    env_name: String,
    #[allow(dead_code)]
    infra_type: String,
    counter: u32,
}

// ============================================================================
// Deployment History Endpoints
// ============================================================================

async fn list_all_deployments(
    State(state): State<AppState>,
    Query(params): Query<DeploymentListQuery>,
) -> Result<Json<DeploymentsListResponse>, ApiError> {
    let storage = state.get_storage().await?;
    let limit = params.limit.unwrap_or(50);

    let deployments = storage.list_deployments(None, None, Some(limit)).await?;
    let items = deployments.into_iter().map(Into::into).collect();

    Ok(Json(DeploymentsListResponse { deployments: items }))
}

async fn list_project_deployments(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(params): Query<DeploymentListQuery>,
) -> Result<Json<DeploymentsListResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let storage = state.get_storage().await?;
    let limit = params.limit.unwrap_or(50);

    let deployments = storage
        .list_deployments(Some(&project.name), None, Some(limit))
        .await?;

    let items = deployments.into_iter().map(Into::into).collect();

    Ok(Json(DeploymentsListResponse { deployments: items }))
}

async fn list_environment_deployments(
    State(state): State<AppState>,
    Path((project_id, env_name)): Path<(String, String)>,
    Query(params): Query<DeploymentListQuery>,
) -> Result<Json<DeploymentsListResponse>, ApiError> {
    let project = state.get_project_by_id(&project_id).await?;

    let Some(project) = project else {
        return Err(ApiError::NotFound("Project not found".to_string()));
    };

    let storage = state.get_storage().await?;
    let limit = params.limit.unwrap_or(50);

    let deployments = storage
        .list_deployments(Some(&project.name), Some(&env_name), Some(limit))
        .await?;

    let items = deployments.into_iter().map(Into::into).collect();

    Ok(Json(DeploymentsListResponse { deployments: items }))
}

async fn get_deployment(
    State(state): State<AppState>,
    Path((_project_id, deployment_id)): Path<(String, String)>,
) -> Result<Json<DeploymentDetailResponse>, ApiError> {
    let storage = state.get_storage().await?;

    let deployment = storage.get_deployment(&deployment_id).await?;

    let Some(deployment) = deployment else {
        return Err(ApiError::NotFound("Deployment not found".to_string()));
    };

    Ok(Json(DeploymentDetailResponse {
        deployment: deployment.into(),
    }))
}

async fn cleanup_deployments(
    State(state): State<AppState>,
    Json(payload): Json<CleanupRequest>,
) -> Result<Json<CleanupResponse>, ApiError> {
    let storage = state.get_storage().await?;
    let max_age_days = payload.max_age_days.unwrap_or(90);

    let removed = storage.cleanup(max_age_days).await?;

    Ok(Json(CleanupResponse {
        removed_count: removed,
        max_age_days,
    }))
}

fn parse_standard_metric(name: &str) -> Option<StandardMetric> {
    match name.to_lowercase().as_str() {
        "cpu_utilization" | "cpuutilization" | "cpu" => Some(StandardMetric::CpuUtilization),
        "memory_utilization" | "memoryutilization" | "memory" => {
            Some(StandardMetric::MemoryUtilization)
        }
        "error_rate" | "errorrate" | "errors" => Some(StandardMetric::ErrorRate),
        "request_latency" | "requestlatency" | "latency" => Some(StandardMetric::RequestLatency),
        "request_count" | "requestcount" | "requests" => Some(StandardMetric::RequestCount),
        _ => None,
    }
}

fn rand_value(metric: StandardMetric) -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as f64;

    let base = (seed % 100.0) / 100.0;

    match metric {
        StandardMetric::CpuUtilization => 30.0 + base * 50.0,
        StandardMetric::MemoryUtilization => 40.0 + base * 40.0,
        StandardMetric::ErrorRate => base * 2.0,
        StandardMetric::RequestLatency => 50.0 + base * 200.0,
        StandardMetric::RequestCount => 100.0 + base * 900.0,
    }
}

fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn create_deployment_context(
    name: &str,
    env: &EnvironmentConfig,
    dry_run: bool,
    deploy_mode: DeployMode,
) -> DeploymentContext {
    DeploymentContext {
        environment_name: name.to_string(),
        environment: env.clone(),
        dry_run,
        verbose: false,
        deploy_mode,
    }
}

// Request/Response types

#[derive(Debug, Serialize)]
struct ProjectsResponse {
    projects: Vec<ProjectInfo>,
}

#[derive(Debug, Serialize)]
struct ProjectResponse {
    project: ProjectInfo,
}

#[derive(Debug, Serialize)]
struct EnvironmentsResponse {
    environments: Vec<EnvironmentInfo>,
}

#[derive(Debug, Serialize)]
struct EnvironmentInfo {
    name: String,
    infrastructure: String,
    infrastructure_type: String,
    deployment_type: String,
    image: Option<String>,
    replicas: Option<u32>,
}

#[derive(Debug, Serialize)]
struct EnvironmentDetailResponse {
    environment: EnvironmentDetail,
}

#[derive(Debug, Serialize)]
struct EnvironmentDetail {
    name: String,
    infrastructure: String,
    infrastructure_type: String,
    deployment_type: String,
    image: Option<String>,
    replicas: Option<u32>,
    resources: Option<ResourceInfo>,
    env_vars: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ResourceInfo {
    cpu: Option<String>,
    memory: Option<String>,
    cpu_limit: Option<String>,
    memory_limit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeployRequest {
    image: Option<String>,
    dry_run: Option<bool>,
    /// Deploy mode: "full" (default) or "app-only"
    deploy_mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeployResponse {
    success: bool,
    message: String,
    version: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    environment: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RollbackRequest {
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct RollbackResponse {
    success: bool,
    message: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LogsQuery {
    follow: Option<bool>,
    lines: Option<usize>,
}

// Metrics request/response types

#[derive(Debug, Serialize)]
struct MetricsListResponse {
    environment: String,
    infrastructure_type: String,
    metrics: Vec<MetricInfo>,
}

#[derive(Debug, Serialize)]
struct MetricInfo {
    name: String,
    display_name: String,
    unit: String,
    metric_type: String,
}

#[derive(Debug, Deserialize)]
struct MetricQueryOptions {
    range: Option<String>,
    period: Option<u32>,
}

#[derive(Debug, Serialize)]
struct MetricDataResponse {
    metric_name: String,
    unit: String,
    data_points: Vec<DataPointResponse>,
}

#[derive(Debug, Serialize)]
struct DataPointResponse {
    timestamp: u64,
    value: f64,
}

#[derive(Debug, Serialize)]
struct MetricGaugeResponse {
    metric_name: String,
    display_name: String,
    current_value: f64,
    unit: String,
    status: String,
    thresholds: Option<ThresholdResponse>,
}

#[derive(Debug, Serialize)]
struct ThresholdResponse {
    warning: f64,
    critical: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MetricStreamOptions {
    interval: Option<u32>,
}

// Deployment history request/response types

#[derive(Debug, Deserialize)]
struct DeploymentListQuery {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DeploymentsListResponse {
    deployments: Vec<DeploymentItem>,
}

#[derive(Debug, Serialize)]
struct DeploymentItem {
    id: String,
    project: String,
    environment: String,
    infrastructure_type: String,
    image: Option<String>,
    status: String,
    message: String,
    deploy_mode: String,
    started_at: u64,
    completed_at: Option<u64>,
    duration_secs: Option<u64>,
    dry_run: bool,
}

impl From<DeploymentRecord> for DeploymentItem {
    fn from(record: DeploymentRecord) -> Self {
        use std::time::UNIX_EPOCH;

        Self {
            id: record.id,
            project: record.project,
            environment: record.environment,
            infrastructure_type: record.infrastructure_type,
            image: record.image,
            status: record.status.as_str().to_string(),
            message: record.message,
            deploy_mode: record.deploy_mode,
            started_at: record
                .started_at
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            completed_at: record.completed_at.and_then(|t| {
                t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok()
            }),
            duration_secs: record.duration_secs,
            dry_run: record.dry_run,
        }
    }
}

#[derive(Debug, Serialize)]
struct DeploymentDetailResponse {
    deployment: DeploymentItem,
}

#[derive(Debug, Deserialize)]
struct CleanupRequest {
    max_age_days: Option<u32>,
}

#[derive(Debug, Serialize)]
struct CleanupResponse {
    removed_count: usize,
    max_age_days: u32,
}

// Error handling

#[derive(Debug)]
enum ApiError {
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chrono_like_timestamp() {
        let ts = chrono_like_timestamp();
        assert_eq!(ts.len(), 8);
        assert!(ts.contains(':'));
    }
}
