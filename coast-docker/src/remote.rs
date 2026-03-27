/// Remote runtime implementation.
///
/// Implements the `Runtime` trait by forwarding all operations to a
/// `coast-remote` agent over HTTP. The remote agent manages the actual
/// Docker daemon on a separate machine.
use std::net::IpAddr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use coast_core::error::{CoastError, Result};

use crate::runtime::{ContainerConfig, ExecResult, Runtime};

/// A Runtime implementation that delegates to a remote coast-remote agent.
pub struct RemoteRuntime {
    /// Base URL of the remote agent (e.g., "http://192.168.1.50:31416").
    base_url: String,
    /// HTTP client for making requests.
    client: reqwest::Client,
}

impl RemoteRuntime {
    /// Create a new RemoteRuntime targeting the given coast-remote agent URL.
    pub fn new(base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to create HTTP client");

        info!(url = %base_url, "RemoteRuntime targeting remote agent");

        Self { base_url, client }
    }

    /// Check if the remote agent is healthy.
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/api/v1/health", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote agent unreachable: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote agent unhealthy: {e}"),
                source: None,
            })?;
        Ok(())
    }

    /// Mount a local project directory on the remote agent via SSHFS.
    ///
    /// Tells the remote agent to run `sshfs user@local:/project/path /mnt/coast/project`.
    /// After this, all file changes on the local machine are visible to remote containers instantly.
    pub async fn mount_project(
        &self,
        project: &str,
        ssh_target: &str,
        project_path: &std::path::Path,
    ) -> Result<String> {
        info!(project, ssh_target, path = %project_path.display(), "mounting project on remote via SSHFS");

        let url = format!("{}/api/v1/mount", self.base_url);
        let resp: MountResponse = self
            .client
            .post(&url)
            .json(&MountRequest {
                project: project.to_string(),
                ssh_target: ssh_target.to_string(),
                remote_path: project_path.to_string_lossy().to_string(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("mount request failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("mount failed: {e}"),
                source: None,
            })?
            .json()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("mount response parse failed: {e}"),
                source: None,
            })?;

        info!(project, path = %resp.mount_path, "SSHFS mount complete");

        Ok(resp.mount_path)
    }

    /// Unmount a project's SSHFS mount on the remote agent.
    pub async fn unmount_project(&self, project: &str) -> Result<()> {
        let url = format!("{}/api/v1/unmount", self.base_url);
        self.client
            .post(&url)
            .json(&UnmountRequest {
                project: project.to_string(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("unmount request failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("unmount failed: {e}"),
                source: None,
            })?;
        Ok(())
    }

    /// Get the base URL of the remote agent.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the status of all containers on the remote.
    pub async fn status(&self) -> Result<serde_json::Value> {
        let url = format!("{}/api/v1/status", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("status request failed: {e}"),
                source: None,
            })?
            .json()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("status parse failed: {e}"),
                source: None,
            })?;
        Ok(resp)
    }
}

#[derive(Serialize)]
struct MountRequest {
    project: String,
    ssh_target: String,
    remote_path: String,
}

#[derive(Deserialize)]
struct MountResponse {
    mount_path: String,
    #[allow(dead_code)]
    status: String,
}

#[derive(Serialize)]
struct UnmountRequest {
    project: String,
}

#[derive(Deserialize)]
struct RunResponse {
    container_id: String,
}

#[derive(Serialize)]
struct RunRequest {
    config: ContainerConfig,
}

#[derive(Serialize)]
struct ContainerRef {
    project: String,
    instance: String,
}

#[derive(Serialize)]
struct ExecRequest {
    project: String,
    instance: String,
    cmd: Vec<String>,
}

#[derive(Deserialize)]
struct ExecResponse {
    exit_code: i64,
    stdout: String,
    stderr: String,
}

#[async_trait]
impl Runtime for RemoteRuntime {
    fn name(&self) -> &str {
        "remote"
    }

    async fn create_coast_container(&self, config: &ContainerConfig) -> Result<String> {
        let url = format!("{}/api/v1/container/run", self.base_url);
        let body = RunRequest {
            config: config.clone(),
        };

        let resp: RunResponse = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote run failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote run error: {e}"),
                source: None,
            })?
            .json()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote run response parse failed: {e}"),
                source: None,
            })?;

        debug!(id = %resp.container_id, "remote container created");
        Ok(resp.container_id)
    }

    async fn start_coast_container(&self, _container_id: &str) -> Result<()> {
        // The remote /run endpoint already starts the container.
        // This is a no-op for remote since create_and_start is atomic.
        Ok(())
    }

    async fn stop_coast_container(&self, _container_id: &str) -> Result<()> {
        // The remote API uses project/instance names, not container IDs.
        // Callers should use stop_by_name() instead.
        Err(CoastError::Docker {
            message: "use stop_by_name() for remote runtime".to_string(),
            source: None,
        })
    }

    async fn remove_coast_container(&self, _container_id: &str) -> Result<()> {
        Err(CoastError::Docker {
            message: "use remove_by_name() for remote runtime".to_string(),
            source: None,
        })
    }

    async fn exec_in_coast(&self, _container_id: &str, _cmd: &[&str]) -> Result<ExecResult> {
        Err(CoastError::Docker {
            message: "use exec_by_name() for remote runtime".to_string(),
            source: None,
        })
    }

    async fn get_container_ip(&self, _container_id: &str) -> Result<IpAddr> {
        Err(CoastError::Docker {
            message: "use get_ip_by_name() for remote runtime".to_string(),
            source: None,
        })
    }

    fn requires_privileged(&self) -> bool {
        true
    }
}

/// Named-based operations (project + instance) that the daemon can use directly.
/// These bypass the container_id limitation of the Runtime trait.
impl RemoteRuntime {
    pub async fn stop_by_name(&self, project: &str, instance: &str) -> Result<()> {
        let url = format!("{}/api/v1/container/stop", self.base_url);
        self.client
            .post(&url)
            .json(&ContainerRef {
                project: project.to_string(),
                instance: instance.to_string(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote stop failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote stop error: {e}"),
                source: None,
            })?;
        Ok(())
    }

    pub async fn remove_by_name(&self, project: &str, instance: &str) -> Result<()> {
        let url = format!("{}/api/v1/container/rm", self.base_url);
        self.client
            .post(&url)
            .json(&ContainerRef {
                project: project.to_string(),
                instance: instance.to_string(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote rm failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote rm error: {e}"),
                source: None,
            })?;
        Ok(())
    }

    pub async fn exec_by_name(
        &self,
        project: &str,
        instance: &str,
        cmd: &[&str],
    ) -> Result<ExecResult> {
        let url = format!("{}/api/v1/container/exec", self.base_url);
        let resp: ExecResponse = self
            .client
            .post(&url)
            .json(&ExecRequest {
                project: project.to_string(),
                instance: instance.to_string(),
                cmd: cmd.iter().map(|s| s.to_string()).collect(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote exec failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote exec error: {e}"),
                source: None,
            })?
            .json()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote exec parse failed: {e}"),
                source: None,
            })?;

        Ok(ExecResult {
            exit_code: resp.exit_code,
            stdout: resp.stdout,
            stderr: resp.stderr,
        })
    }

    pub async fn get_ip_by_name(&self, project: &str, instance: &str) -> Result<IpAddr> {
        let url = format!("{}/api/v1/container/ip", self.base_url);
        let resp: serde_json::Value = self
            .client
            .post(&url)
            .json(&ContainerRef {
                project: project.to_string(),
                instance: instance.to_string(),
            })
            .send()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote ip failed: {e}"),
                source: None,
            })?
            .error_for_status()
            .map_err(|e| CoastError::Docker {
                message: format!("remote ip error: {e}"),
                source: None,
            })?
            .json()
            .await
            .map_err(|e| CoastError::Docker {
                message: format!("remote ip parse failed: {e}"),
                source: None,
            })?;

        let ip_str = resp["ip"]
            .as_str()
            .ok_or_else(|| CoastError::Docker {
                message: "no ip in response".to_string(),
                source: None,
            })?;

        ip_str.parse().map_err(|e| CoastError::Docker {
            message: format!("invalid IP: {e}"),
            source: None,
        })
    }
}

