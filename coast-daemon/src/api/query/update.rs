use axum::extract::State;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use coast_core::protocol::{
    PrepareForUpdateRequest, PrepareForUpdateResponse, UpdateApplyResponse, UpdateCheckResponse,
    UpdateSafetyRequest, UpdateSafetyResponse,
};

use crate::handlers;
use crate::server::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/update/check", get(check_update))
        .route("/update/is-safe-to-update", get(is_safe_to_update))
        .route("/update/prepare-for-update", post(prepare_for_update))
        .route("/update/apply", post(apply_update))
}

async fn check_update() -> Json<UpdateCheckResponse> {
    let info = coast_update::check_for_updates().await;
    let update_available = info
        .latest_version
        .as_ref()
        .and_then(|latest| {
            let current = coast_update::version::parse_version(&info.current_version).ok()?;
            let latest = coast_update::version::parse_version(latest).ok()?;
            Some(coast_update::version::is_newer(&current, &latest))
        })
        .unwrap_or(false);

    Json(UpdateCheckResponse {
        current_version: info.current_version,
        latest_version: info.latest_version,
        update_available,
    })
}

async fn is_safe_to_update(
    State(state): State<Arc<AppState>>,
) -> Result<Json<UpdateSafetyResponse>, (StatusCode, Json<serde_json::Value>)> {
    match handlers::update_safety::handle_is_safe_to_update(UpdateSafetyRequest::default(), &state)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )),
    }
}

async fn prepare_for_update(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PrepareForUpdateRequest>,
) -> Result<Json<PrepareForUpdateResponse>, (StatusCode, Json<serde_json::Value>)> {
    match handlers::update_safety::handle_prepare_for_update(req, &state).await {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": error.to_string() })),
        )),
    }
}

async fn apply_update(
    State(state): State<Arc<AppState>>,
) -> Result<Json<UpdateApplyResponse>, (StatusCode, Json<serde_json::Value>)> {
    if std::env::var_os("COAST_HOME").is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Self-update is disabled for dev builds. Rebuild from source with ./dev_setup.sh instead."
            })),
        ));
    }

    let info = coast_update::check_for_updates().await;
    let Some(latest_str) = info.latest_version else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Could not determine latest version" })),
        ));
    };

    let current = coast_update::version::current_version().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Invalid current version: {e}") })),
        )
    })?;
    let latest_ver = coast_update::version::parse_version(&latest_str).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Invalid version: {e}") })),
        )
    })?;
    if !coast_update::version::is_newer(&current, &latest_ver) {
        return Ok(Json(UpdateApplyResponse {
            success: true,
            version: latest_str,
        }));
    }

    let tarball =
        coast_update::updater::download_release(&latest_ver, coast_update::DOWNLOAD_TIMEOUT)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Download failed: {e}") })),
                )
            })?;

    let prepare = handlers::update_safety::handle_prepare_for_update(
        PrepareForUpdateRequest::default(),
        &state,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    if !prepare.ready {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "Daemon is not ready to update yet.",
                "report": prepare.report,
            })),
        ));
    }

    coast_update::updater::apply_update(&tarball).map_err(|e| {
        state.set_update_quiescing(false);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Apply failed: {e}") })),
        )
    })?;

    let version = latest_str.clone();

    // Schedule a self-restart so the new binary takes over.
    // The 500ms delay lets the HTTP response reach the client first.
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        restart_daemon();
    });

    Ok(Json(UpdateApplyResponse {
        success: true,
        version,
    }))
}

/// Replace the current process with the new daemon binary.
fn restart_daemon() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("cannot determine current exe for restart: {e}");
            return;
        }
    };

    let args: Vec<String> = std::env::args().collect();

    tracing::info!("restarting daemon: {}", exe.display());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // exec() replaces the process — this never returns on success
        let err = std::process::Command::new(&exe).args(&args[1..]).exec();
        tracing::error!("exec failed: {err}");
    }

    #[cfg(not(unix))]
    {
        // Fallback: spawn new process and exit
        let _ = std::process::Command::new(&exe).args(&args[1..]).spawn();
        std::process::exit(0);
    }
}
