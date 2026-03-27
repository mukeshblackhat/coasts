use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use coast_docker::runtime::ContainerConfig;

use crate::docker;
use crate::state::RemoteState;
use crate::sync;

pub fn router() -> Router<Arc<RemoteState>> {
    Router::new()
        .route("/mount", post(sync::handle_mount))
        .route("/unmount", post(sync::handle_unmount))
        .route("/mounts", get(sync::handle_mounts))
        .route("/container/run", post(handle_run))
        .route("/container/stop", post(handle_stop))
        .route("/container/rm", post(handle_rm))
        .route("/container/exec", post(handle_exec))
        .route("/container/ip", post(handle_ip))
        .route("/status", get(handle_status))
        .route("/health", get(handle_health))
}

// --- Run ---

#[derive(Deserialize)]
pub struct RunRequest {
    pub config: ContainerConfig,
}

#[derive(Serialize)]
pub struct RunResponse {
    pub container_id: String,
}

async fn handle_run(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    let config = &req.config;
    info!(project = %config.project, instance = %config.instance_name, "run request");

    let container_id = docker::create_and_start(&state, config)
        .await
        .map_err(|e| {
            error!(?e, "run failed");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    state
        .track_container(
            &config.project,
            &config.instance_name,
            &container_id,
            "running",
        )
        .await;

    Ok(Json(RunResponse { container_id }))
}

// --- Stop ---

#[derive(Deserialize)]
pub struct ContainerRef {
    pub project: String,
    pub instance: String,
}

async fn handle_stop(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<ContainerRef>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let container = state
        .get_container(&req.project, &req.instance)
        .await
        .ok_or((StatusCode::NOT_FOUND, "container not found".to_string()))?;

    docker::stop_container(&state, &container.container_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state
        .track_container(&req.project, &req.instance, &container.container_id, "stopped")
        .await;

    Ok(Json(serde_json::json!({"status": "stopped"})))
}

// --- Remove ---

async fn handle_rm(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<ContainerRef>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let container = state
        .get_container(&req.project, &req.instance)
        .await
        .ok_or((StatusCode::NOT_FOUND, "container not found".to_string()))?;

    docker::remove_container(&state, &container.container_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state.remove_tracked(&req.project, &req.instance).await;

    Ok(Json(serde_json::json!({"status": "removed"})))
}

// --- Exec ---

#[derive(Deserialize)]
pub struct ExecRequest {
    pub project: String,
    pub instance: String,
    pub cmd: Vec<String>,
}

#[derive(Serialize)]
pub struct ExecResponse {
    pub exit_code: i64,
    pub stdout: String,
    pub stderr: String,
}

async fn handle_exec(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, (StatusCode, String)> {
    let container = state
        .get_container(&req.project, &req.instance)
        .await
        .ok_or((StatusCode::NOT_FOUND, "container not found".to_string()))?;

    let result = docker::exec_in_container(&state, &container.container_id, req.cmd)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ExecResponse {
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
    }))
}

// --- IP ---

async fn handle_ip(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<ContainerRef>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let container = state
        .get_container(&req.project, &req.instance)
        .await
        .ok_or((StatusCode::NOT_FOUND, "container not found".to_string()))?;

    let ip = docker::get_container_ip(&state, &container.container_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"ip": ip})))
}

// --- Status ---

async fn handle_status(
    State(state): State<Arc<RemoteState>>,
) -> Json<serde_json::Value> {
    let containers = state.list_containers().await;
    let mounts = state.list_mounts().await;
    Json(serde_json::json!({
        "containers": containers,
        "mounts": mounts,
        "mount_dir": state.mount_dir.to_string_lossy(),
    }))
}

// --- Health ---

async fn handle_health(
    State(state): State<Arc<RemoteState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .docker
        .ping()
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, format!("docker unreachable: {e}")))?;

    Ok(Json(serde_json::json!({"status": "ok", "docker": "connected"})))
}
