use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::state::RemoteState;

#[derive(Deserialize)]
pub struct MountRequest {
    /// Project name.
    pub project: String,
    /// SSH user@host for the local machine (e.g., "user@192.168.1.10").
    pub ssh_target: String,
    /// Path to the project on the local machine (e.g., "/Users/me/myproject").
    pub remote_path: String,
}

#[derive(Serialize)]
pub struct MountResponse {
    pub mount_path: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct UnmountRequest {
    pub project: String,
}

/// Mount a local machine's project directory via SSHFS.
///
/// Runs: sshfs user@local:/project/path /mnt/coast/project -o reconnect,ServerAliveInterval=15
pub async fn handle_mount(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<MountRequest>,
) -> Result<Json<MountResponse>, (StatusCode, String)> {
    let mount_path = state.project_mount_path(&req.project);

    info!(
        project = %req.project,
        ssh_target = %req.ssh_target,
        remote_path = %req.remote_path,
        mount_path = %mount_path.display(),
        "mounting project via SSHFS"
    );

    // Unmount if already mounted
    if mount_path.exists() {
        let _ = unmount_sshfs(&mount_path).await;
    }

    // Create mount point
    tokio::fs::create_dir_all(&mount_path).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("mkdir failed: {e}"))
    })?;

    // Run sshfs
    let sshfs_source = format!("{}:{}", req.ssh_target, req.remote_path);
    let output = tokio::process::Command::new("sshfs")
        .arg(&sshfs_source)
        .arg(&mount_path)
        .arg("-o")
        .arg("reconnect,ServerAliveInterval=15,ServerAliveCountMax=3,follow_symlinks,allow_other,StrictHostKeyChecking=no,UserKnownHostsFile=/dev/null")
        .output()
        .await
        .map_err(|e| {
            error!(?e, "sshfs command failed to execute");
            (StatusCode::INTERNAL_SERVER_ERROR, format!(
                "sshfs not found or failed to execute: {e}. Install with: apt install sshfs"
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(stderr = %stderr, "sshfs mount failed");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("sshfs failed: {stderr}")));
    }

    // Track the mount
    state.track_mount(&req.project, &req.ssh_target, &req.remote_path).await;

    info!(project = %req.project, path = %mount_path.display(), "SSHFS mount complete");

    Ok(Json(MountResponse {
        mount_path: mount_path.to_string_lossy().to_string(),
        status: "mounted".to_string(),
    }))
}

/// Unmount a project's SSHFS mount.
pub async fn handle_unmount(
    State(state): State<Arc<RemoteState>>,
    Json(req): Json<UnmountRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mount_path = state.project_mount_path(&req.project);

    if !mount_path.exists() {
        return Ok(Json(serde_json::json!({"status": "not_mounted"})));
    }

    unmount_sshfs(&mount_path).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("unmount failed: {e}"))
    })?;

    state.remove_mount(&req.project).await;

    info!(project = %req.project, "SSHFS unmounted");

    Ok(Json(serde_json::json!({"status": "unmounted"})))
}

/// List all active SSHFS mounts.
pub async fn handle_mounts(
    State(state): State<Arc<RemoteState>>,
) -> Json<serde_json::Value> {
    let mounts = state.list_mounts().await;
    Json(serde_json::json!({"mounts": mounts}))
}

/// Unmount an SSHFS mount point. Tries fusermount first, falls back to umount.
async fn unmount_sshfs(mount_path: &std::path::Path) -> Result<(), String> {
    // Try fusermount -u (Linux)
    let result = tokio::process::Command::new("fusermount")
        .arg("-u")
        .arg(mount_path)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => return Ok(()),
        _ => {
            warn!("fusermount failed, trying umount");
        }
    }

    // Fallback: umount (macOS / Linux)
    let result = tokio::process::Command::new("umount")
        .arg(mount_path)
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("umount failed: {stderr}"))
        }
        Err(e) => Err(format!("umount command failed: {e}")),
    }
}
